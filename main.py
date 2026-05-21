"""Entry point for the compr compression tool.

Usage:
  python main.py --pack <input_folder> | zstd -o archive.compr
  zstd -d -c archive.compr | python main.py --unpack <output_dir>
  python main.py --pack <input_folder> <archive.compr>
  python main.py --unpack <archive.compr> <output_dir>
"""

from compr._cli import main

if __name__ == "__main__":
    main()
