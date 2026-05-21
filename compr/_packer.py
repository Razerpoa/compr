import os
import sys
import struct

import numpy as np
from PIL import Image

from compr._classify import is_image_file, is_video_file


def pack_to_stream(folder_path, output_stream):
    """Walk folder, classify each file, write header+payload to output_stream.

    Header format (21 bytes fixed + variable path):
      [Marker:1s][path_len:I][width:I][height:I][data_size:Q][path_bytes][payload]

    Marker b'I' = image (planar RGB payload)
    Marker b'V' = video (raw byte payload)
    """
    if not os.path.isdir(folder_path):
        print(f"Error: '{folder_path}' is not a valid directory.", file=sys.stderr)
        sys.exit(1)

    image_count = 0
    video_count = 0
    skipped = 0

    for root, dirs, files in os.walk(folder_path):
        dirs.sort()  # Deterministic directory traversal
        for f in sorted(files):
            full_path = os.path.join(root, f)
            rel_path = os.path.relpath(full_path, folder_path)
            path_bytes = rel_path.encode('utf-8')
            path_len = len(path_bytes)

            try:
                if is_image_file(full_path):
                    with Image.open(full_path) as img:
                        img_rgb = img.convert("RGB")
                        width, height = img_rgb.size
                        pixels = np.array(img_rgb)

                        # Planarize: RRR...GGG...BBB...
                        r_plane = pixels[:, :, 0].flatten().tobytes()
                        g_plane = pixels[:, :, 1].flatten().tobytes()
                        b_plane = pixels[:, :, 2].flatten().tobytes()
                        planar_data = r_plane + g_plane + b_plane
                        data_size = len(planar_data)

                        header = struct.pack(
                            f"<cIIIQ{path_len}s",
                            b'I', path_len, width, height, data_size, path_bytes
                        )
                        output_stream.write(header + planar_data)
                        print(f" -> Image: {rel_path} ({width}x{height})", file=sys.stderr)
                        image_count += 1

                elif is_video_file(full_path):
                    data_size = os.path.getsize(full_path)
                    header = struct.pack(
                        f"<cIIIQ{path_len}s",
                        b'V', path_len, 0, 0, data_size, path_bytes
                    )
                    output_stream.write(header)
                    # Stream video in 64 KiB chunks to avoid loading >4 GiB files into RAM
                    with open(full_path, 'rb') as vf:
                        while chunk := vf.read(65536):
                            output_stream.write(chunk)
                    print(f" -> Video: {rel_path} ({data_size / 1024:.1f} KiB)", file=sys.stderr)
                    video_count += 1
                else:
                    print(f" -> Skipping (unsupported): {rel_path}", file=sys.stderr)
                    skipped += 1

            except Exception as e:
                print(f" -> Skipping {rel_path}: {e}", file=sys.stderr)
                skipped += 1

    if image_count == 0 and video_count == 0:
        print("Error: No supported images or videos found.", file=sys.stderr)
        sys.exit(1)

    print(
        f"\nSummary: {image_count} images, {video_count} videos, {skipped} skipped",
        file=sys.stderr
    )
