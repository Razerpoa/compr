import os
import sys
import subprocess
import shutil

from compr._packer import pack_to_stream
from compr._unpacker import unpack_from_stream


def build_solid_archive(folder_path, output_archive):
    """Convenience: pack folder directly to a compressed archive file."""
    print(f"Packing '{folder_path}' -> '{output_archive}'", file=sys.stderr)

    if os.path.exists(output_archive):
        os.remove(output_archive)

    # Use --long=31 always (consistent with extraction's hardcoded --long=31)
    cmd = "zstd -22 --ultra --long=31 -T0 -o " + output_archive
    with subprocess.Popen(cmd, shell=True, stdin=subprocess.PIPE) as proc:
        pack_to_stream(folder_path, proc.stdin)

    compressed_size = os.path.getsize(output_archive)
    print(
        f"\nCreated: {output_archive} ({compressed_size / 1024 ** 2:.2f} MiB)",
        file=sys.stderr
    )


def extract_solid_archive(archive_path, output_dir):
    """Convenience: unpack a compressed archive file to a directory."""
    if not os.path.exists(archive_path):
        print(f"Error: Archive '{archive_path}' not found.", file=sys.stderr)
        sys.exit(1)

    print(f"Unpacking '{archive_path}' -> '{output_dir}'", file=sys.stderr)

    # Check if srep is available (graceful degradation)
    has_srep = shutil.which("srep") is not None

    # Decompress to temp file
    tmp_raw = f"{archive_path}.raw.tmp"
    try:
        subprocess.run(
            ["zstd", "-d", "--long=31", "-o", tmp_raw, archive_path],
            check=True, capture_output=True
        )

        # Check for SREP magic
        with open(tmp_raw, 'rb') as f:
            magic = f.read(4)

        if magic == b'SREP':
            if has_srep:
                print(" -> SREP layer detected. Decompressing...", file=sys.stderr)
                tmp_srep = f"{archive_path}.srep.tmp"
                subprocess.run(
                    ["srep", "-d", tmp_raw, tmp_srep],
                    check=True, capture_output=True
                )
                os.unlink(tmp_raw)
                tmp_raw = tmp_srep
            else:
                print(
                    " -> SREP layer detected but 'srep' not installed. "
                    "Install srep to decompress this archive.",
                    file=sys.stderr
                )
                os.unlink(tmp_raw)
                sys.exit(1)
        else:
            print(" -> Pure ZSTD archive", file=sys.stderr)

        # Feed the decompressed data to the streaming unpacker
        with open(tmp_raw, 'rb') as f:
            unpack_from_stream(f, output_dir)
    except subprocess.CalledProcessError as e:
        print(f"Error during decompression: {e}", file=sys.stderr)
        sys.exit(1)
    finally:
        if os.path.exists(tmp_raw):
            os.unlink(tmp_raw)
