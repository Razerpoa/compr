import sys

from compr._packer import pack_to_stream
from compr._unpacker import unpack_from_stream
from compr._archive import build_solid_archive, extract_solid_archive


def main():
    if len(sys.argv) < 3:
        print("Solid Compression Pipeline -- Streaming Pipe Architecture")
        print("-" * 55)
        print("Streaming Mode (pipe-friendly):")
        print("  python main.py --pack <input_folder> | zstd -o archive.compr")
        print("  zstd -d -c archive.compr | python main.py --unpack <output_dir>")
        print()
        print("Convenience Mode (file-to-file):")
        print("  python main.py --pack <input_folder> <output_archive.compr>")
        print("  python main.py --unpack <archive.compr> <output_dir>")
        sys.exit(1)

    mode = sys.argv[1].lower()

    if mode == "--pack":
        if len(sys.argv) == 3:
            pack_to_stream(sys.argv[2], sys.stdout.buffer)
        elif len(sys.argv) == 4:
            build_solid_archive(sys.argv[2], sys.argv[3])
        else:
            print("Usage: python main.py --pack <folder> [archive.compr]", file=sys.stderr)
            sys.exit(1)
    elif mode == "--unpack":
        if len(sys.argv) == 3:
            unpack_from_stream(sys.stdin.buffer, sys.argv[2])
        elif len(sys.argv) == 4:
            extract_solid_archive(sys.argv[2], sys.argv[3])
        else:
            print("Usage: python main.py --unpack [archive.compr] <output_dir>", file=sys.stderr)
            sys.exit(1)
    else:
        print(f"Error: Unknown mode '{mode}'. Use --pack or --unpack.", file=sys.stderr)
        sys.exit(1)



