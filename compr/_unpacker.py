import os
import sys
import struct

import numpy as np
from PIL import Image


def unpack_from_stream(input_stream, output_dir):
    """Read header+payload stream from input_stream, reconstruct files in output_dir.

    Validates path traversal: rejects rel_path with ../ that escapes output_dir.
    Images -> saved as .png. Videos -> saved with original filename/extension.
    """
    os.makedirs(output_dir, exist_ok=True)
    extracted_images = 0
    extracted_videos = 0
    output_dir_norm = os.path.normpath(output_dir)

    while True:
        # Read fixed header (21 bytes: 1 marker + 3 I's + 1 Q)
        header_bytes = input_stream.read(21)
        if not header_bytes or len(header_bytes) < 21:
            break

        marker, path_len, width, height, data_size = struct.unpack(
            "<cIIIQ", header_bytes
        )

        # Read path
        path_bytes = input_stream.read(path_len)
        if len(path_bytes) < path_len:
            print("Error: Truncated path in stream", file=sys.stderr)
            break
        rel_path = path_bytes.decode('utf-8')

        # Read payload
        payload = input_stream.read(data_size)
        if len(payload) < data_size:
            print(f"Error: Truncated payload for {rel_path}", file=sys.stderr)
            break

        # Path traversal protection
        dest = os.path.normpath(os.path.join(output_dir_norm, rel_path))
        if not dest.startswith(output_dir_norm + os.sep):
            print(f"Error: Path traversal blocked: {rel_path}", file=sys.stderr)
            continue

        os.makedirs(os.path.dirname(dest), exist_ok=True)

        if marker == b'I' and width > 0 and height > 0:
            # Validate data_size matches expected planar dimensions
            expected_pixels = width * height
            if data_size != expected_pixels * 3:
                print(
                    f"Error: data_size mismatch for image {rel_path}: "
                    f"got {data_size}, expected {expected_pixels * 3}",
                    file=sys.stderr
                )
                continue

            # Image: de-planarize and save as PNG
            r = np.frombuffer(payload[0:expected_pixels], dtype=np.uint8).reshape((height, width))
            g = np.frombuffer(
                payload[expected_pixels:2 * expected_pixels], dtype=np.uint8
            ).reshape((height, width))
            b = np.frombuffer(
                payload[2 * expected_pixels:], dtype=np.uint8
            ).reshape((height, width))
            img = Image.fromarray(np.stack((r, g, b), axis=-1), mode="RGB")

            # Replace original extension with .png
            out_path = os.path.splitext(dest)[0] + ".png"
            img.save(out_path, "PNG")
            print(f" -> Restored Image: {out_path}", file=sys.stderr)
            extracted_images += 1
        else:
            # Video (or raw file): write bytes directly
            with open(dest, 'wb') as f:
                f.write(payload)
            print(f" -> Restored File: {dest}", file=sys.stderr)
            extracted_videos += 1

    print(
        f"\nUnpacked: {extracted_images} images, {extracted_videos} files",
        file=sys.stderr
    )
