# compr — streaming solid archive for images/videos

## Commands

```
cargo build
cargo test                          # unit + integration
cargo test --lib                    # unit tests only
cargo test --test integration       # integration tests only
cargo test <name>                   # single test
cargo run -- pack <src> <dst>       # [--max] [--mem N] [--level N]
cargo run -- unpack <src> <dst>
cargo run -- list <archive>
cargo run -- info <archive>
cargo run -- verify <archive>
cargo run -- entropy <archive>
```

Use `-` for stdin/stdout streaming: `pack dir - | unpack - out/`.

## Architecture

| Layer | File | Role |
|-------|------|------|
| CLI | `src/main.rs` | clap subcommands |
| Format | `src/format.rs` | ArchiveHeader / Entry / ArchiveFooter binary layout |
| Pack | `src/packer.rs` | walkdir → classify → planarize → sort → write |
| Unpack | `src/unpacker.rs` | read header → decompress → extract entries → footer verify |
| Compress | `src/compress.rs` | ZSTD wrapper with auto-decompress, CompressParams (default/max/eco) |
| Classify | `src/classify.rs` | extension-based + image::image_dimensions() fallback |
| Image | `src/image.rs` | load_planar / save_planar (RRR…GGG…BBB…) |
| Entropy | `src/entropy.rs` | Shannon entropy per entry |

## Format quirks

- **Header is always uncompressed** (readers need flags to detect ZSTD).
- **Entries + footer are ZSTD-compressed** as a single solid stream.
- **Images are planarized** (RRR…GGG…BBB…) before compression so ZSTD LDM matches adjacent R-planes.
- **Images always unpack as PNG** via `save_planar()` → `with_extension("png")` regardless of input format.
- **Entries read inline** in unpacker.rs (positional byte reads) — `Entry::read()` is test-only.
- **Sort order**: folder-grouped, images before videos, then filename — for ZSTD LDM efficiency.
- **Path traversal protection**: `is_path_traversal()` in `format.rs` rejects any `..` component (CWE-22).

## Compression params

- `--max`: level 22, window log 30 (1 GiB), auto threads, LDM on
- `--mem N`: eco mode, level 15, window log scaled to budget
- `--level N`: override level (default 19)
- Default: level 19, window log 28 (256 MiB), 2 threads, LDM on
- Decompressor sets `window_log_max(31)` so archives packed with `--max` can be read back.

## Testing

- Integration tests run `cargo run --` as subprocesses (in `tests/integration.rs`).
- Unit tests are inline in each `src/*.rs` module.
- `tempfile` used for temp directories in both unit and integration tests.
- CRC32 check is per-entry (not archive-level). Footer stores only entry-count CRC32.

## Important constraints

- Only image (PNG/JPG/WebP/etc.) and video (MP4/MKV/AVI/etc.) files are packed. Other files are silently skipped.
- Empty input dir or no supported files → `bail!()`.
- Unsupported extension with valid image magic → still classified as Image (dimension probe fallback).
- CRC32 per entry verifies: kind + path_len + path + width + height + data_len + data.
