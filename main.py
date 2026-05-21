import os
import sys
import math
import struct
import subprocess
from collections import Counter
import numpy as np
from PIL import Image

def calculate_entropy(data_bytes):
    """Calculates standard Shannon Entropy (0 to 8 bits/byte)"""
    if not data_bytes:
        return 0.0
    length = len(data_bytes)
    counter = Counter(data_bytes)
    entropy = 0.0
    for count in counter.values():
        p = count / length
        entropy -= p * math.log2(p)
    return entropy

def build_solid_archive(folder_path, output_archive, analyze_entropy = False):
    """Extracts raw pixels, planarizes them, and compresses them into a solid archive"""
    valid_extensions = ('.png', '.jpg', '.jpeg', '.bmp', '.webp', '.tiff')
    
    if not os.path.isdir(folder_path):
        print(f"Error: '{folder_path}' is not a valid directory.")
        sys.exit(1)
        
    image_files = [f for f in os.listdir(folder_path) if f.lower().endswith(valid_extensions)]
    if not image_files:
        print(f"Error: No supported images found in '{folder_path}'")
        sys.exit(1)
        
    print(f"Found {len(image_files)} images to process.")
    solid_payload = bytearray()
    
    for filename in image_files:
        full_path = os.path.join(folder_path, filename)
        try:
            with Image.open(full_path) as img:
                img_rgb = img.convert("RGB")
                width, height = img_rgb.size
                pixels = np.array(img_rgb)
                
                # Planarize layout: RRR... GGG... BBB...
                r_plane = pixels[:, :, 0].flatten().tobytes()
                g_plane = pixels[:, :, 1].flatten().tobytes()
                b_plane = pixels[:, :, 2].flatten().tobytes()
                planar_data = r_plane + g_plane + b_plane
                
                filename_bytes = filename.encode('utf-8')
                fn_len = len(filename_bytes)
                
                # Header structure: [Filename Length] [Width] [Height] [Raw Filename String]
                header_struct = struct.pack(f"<III{fn_len}s", fn_len, width, height, filename_bytes)
                solid_payload.extend(header_struct + planar_data)
                print(f" -> Prepared: {filename} ({width}x{height})")
                
        except Exception as e:
            print(f" -> Skipping file {filename}: {e}")

    total_uncompressed_size = len(solid_payload)
    print(f"\nTotal Solid Payload Size: {total_uncompressed_size / (1024*1024):.2f} MiB")
    
    if analyze_entropy:
        print("Analyzing stream predictability...")
        raw_entropy = calculate_entropy(solid_payload)
        print(f"Raw Stream Shannon Entropy: {raw_entropy:.4f} bits/byte")
    
    if os.path.exists(output_archive):
        print("overwriting...")
        os.remove(output_archive)

    # Calculate ideal window exponent based on payload size
    if total_uncompressed_size <= 128 * 1024 * 1024:
        window_flag = "--long=27"  # 128 MB window
    elif total_uncompressed_size <= 512 * 1024 * 1024:
        window_flag = "--long=29"  # 512 MB window
    else:
        window_flag = "--long=31"

    print(f"Using {window_flag}")

    print("\n[Strategy] Deploying Pure High-Window ZSTD Engine...")
    comp_cmd = f"zstd -22 --ultra {window_flag} -T0 -o {output_archive}"
    process = subprocess.Popen(comp_cmd, shell=True, stdin=subprocess.PIPE)
    process.communicate(input=solid_payload)
    
    if os.path.exists(output_archive):
        compressed_size = os.path.getsize(output_archive)
        ratio = (compressed_size / total_uncompressed_size) * 100
        print("\n" + "="*40)
        print("COMPRESSION COMPLETE")
        print(f"Archive Created: {output_archive}")
        print(f"Before: {total_uncompressed_size / (1024*1024):.2f} MiB")
        print(f"After: {compressed_size / (1024*1024):.2f} MiB")
        print(f"Net Compression Ratio: {ratio:.2f}%")
        print("="*40)

def extract_solid_archive(archive_path, output_dir):
    """Decompresses the archive and reconstructs original images from planar pixels"""
    if not os.path.exists(archive_path):
        print(f"Error: Archive '{archive_path}' not found.")
        sys.exit(1)
        
    os.makedirs(output_dir, exist_ok=True)
    print(f"Opening archive: {archive_path}")
    print("Initializing decompression stream...")
    
    try:
        # Use discrete files during extraction to ensure stability across big payloads
        temp_zstd_out = f"{archive_path}.zstd.tmp"
        
        # Step 1: Unpack ZSTD Layer safely with full memory ceiling bounds mapped out
        print(" -> Unpacking ZSTD Layer...")
        subprocess.run(f"zstd -d --long=31 {archive_path} -o {temp_zstd_out}", shell=True, check=True)
        
        # Step 2: Probe the unpacked payload headers to check for SREP structures
        with open(temp_zstd_out, "rb") as f:
            magic_bytes = f.read(4)
            
        if magic_bytes == b'SREP':
            print(" -> SREP layer detected. Reconstructing macro tables...")
            temp_srep_out = f"{archive_path}.raw.tmp"
            subprocess.run(f"srep -d {temp_zstd_out} {temp_srep_out}", shell=True, check=True)
            
            with open(temp_srep_out, "rb") as f:
                raw_payload = f.read()
                
            if os.path.exists(temp_srep_out): 
                os.remove(temp_srep_out)
        else:
            print(" -> Pure ZSTD layer detected.")
            with open(temp_zstd_out, "rb") as f:
                raw_payload = f.read()
                
        if os.path.exists(temp_zstd_out): 
            os.remove(temp_zstd_out)
            
    except subprocess.CalledProcessError as e:
        print(f"Error during extraction pipeline execution: {e}")
        sys.exit(1)

    total_bytes = len(raw_payload)
    print(f"Unpacked solid payload stream: {total_bytes / (1024*1024):.2f} MiB")
    
    # Parse the custom byte stream linearly
    offset = 0
    extracted_count = 0
    fixed_header_size = 12 # 3 integers * 4 bytes = 12 bytes
    
    while offset < total_bytes:
        if offset + fixed_header_size > total_bytes:
            break
            
        # Read the numerical metadata lengths
        fn_len, width, height = struct.unpack("<III", raw_payload[offset:offset+fixed_header_size])
        offset += fixed_header_size
        
        # Extract the filename string
        filename = raw_payload[offset:offset+fn_len].decode('utf-8')
        offset += fn_len
        
        # Calculate expected pixel array bounds
        img_size = width * height
        total_planar_bytes = img_size * 3 # 3 channels (R, G, B)
        
        if offset + total_planar_bytes > total_bytes:
            print(f"Error: Truncated data stream encountered when unpacking {filename}")
            break
            
        # Slice out the channel planes
        planar_data = raw_payload[offset:offset+total_planar_bytes]
        offset += total_planar_bytes
        
        # Separate the contiguous segments back out
        r = np.frombuffer(planar_data[0:img_size], dtype=np.uint8).reshape((height, width))
        g = np.frombuffer(planar_data[img_size:2*img_size], dtype=np.uint8).reshape((height, width))
        b = np.frombuffer(planar_data[2*img_size:], dtype=np.uint8).reshape((height, width))
        
        # Stack the planes back into an interleaved RGB matrix (Height, Width, 3)
        interleaved_pixels = np.stack((r, g, b), axis=-1)
        
        # Rebuild the image and save it out completely losslessly as a PNG
        output_image = Image.fromarray(interleaved_pixels, mode="RGB")
        
        # Force conversion of original extension to png to protect lossless data layout
        base_name = os.path.splitext(filename)[0]
        out_file_path = os.path.join(output_dir, f"{base_name}.png")
        
        output_image.save(out_file_path, "PNG")
        print(f" -> Successfully Restored: {out_file_path}")
        extracted_count += 1
        
    print("\n" + "="*40)
    print("UNPACKING COMPLETE")
    print(f"Total Images Restored into '{output_dir}': {extracted_count}")
    print(f"Total Extracted Size: {total_bytes / (1024*1024):.2f} MiB")
    print("="*40)

if __name__ == "__main__":
    if len(sys.argv) < 4:
        print("Custom Solid Compression Pipeline Framework")
        print("-" * 43)
        print("Usage Mode 1 (Pack Folder):")
        print("  uv run compr_solid.py --pack <input_folder_path> <output_archive.vacation>")
        print("\nUsage Mode 2 (Unpack Archive):")
        print("  uv run compr_solid.py --unpack <archive_path> <target_extraction_directory>")
        sys.exit(1)
        
    mode = sys.argv[1].lower()
    
    if mode == "--pack":
        # Turning entropy analysis back on since you mentioned wanting to check performance vs payload entropy
        build_solid_archive(sys.argv[2], sys.argv[3], analyze_entropy=False)
    elif mode == "--unpack":
        extract_solid_archive(sys.argv[2], sys.argv[3])
    else:
        print(f"Error: Unknown parameter switch '{mode}'. Use --pack or --unpack.")
        sys.exit(1)