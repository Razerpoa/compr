# compr — Rust Migration Design

> Status: **Design complete, implementation deferred**
> Date: 2026-05-21

## Overview

Rewrite `compr` from Python to Rust for:
- **Hard memory budget** for ZSTD compression (no more RAM spikes)
- **Cleaner binary format** with magic bytes, versioning, and per-entry CRC32
- **Single binary deployment** — no Python, no pip, no virtualenvs
- **Same streaming pipe architecture** — stdin/stdout Unix philosophy

---

## Architecture

```
compr/
├── Cargo.toml
├── src/
│   ├── main.rs          # CLI (clap)
│   ├── format.rs        # Binary format + ser/de
│   ├── packer.rs        # walkdir + classify + encode
│   ├── unpacker.rs      # decode + validate + reconstruct
│   ├── classify.rs      # Image/video detection
│   ├── image.rs         # image crate loading + planarize
│   └── entropy.rs       # Shannon entropy (carry-over)
└── tests/
    └── integration.rs
```

### Dependencies

| Python | Rust |
|--------|------|
| PIL/Pillow | `image` crate |
| numpy | raw `Vec<u8>` |
| zstd CLI | `zstd` + `zstd-safe` crates |
| os.walk | `walkdir` crate |
| struct.pack | `bytes` crate + manual encoding |
| argparse | `clap` crate |
| pytest | `#[test]` + `tempfile` + `assert_cmd` |
| — | `anyhow` / `thiserror` for errors |
| — | `crc32fast` for per-entry CRC32 |

---

## New Archive Format (Not Backward-Compatible)

```
[MAGIC:     b"CMPR"     4 bytes]
[VERSION:   0x0001      2 bytes]
[FLAGS:     0x0000      2 bytes]
[ENTRY 1]
[ENTRY 2]
...
[ENTRY N]
[ENTRY_COUNT:  u32 LE   4 bytes]
[CRC32:        u32 LE   4 bytes]  CRC32 of ENTRY_COUNT
[MAGIC_END: b"CMPR"     4 bytes]
```

### Entry Format

```
[KIND:      u8       1 byte ]  0x01 = Image, 0x02 = Video
[PATH_LEN:  u16 LE   2 bytes]  max 65535
[PATH:      UTF-8    N bytes]  relative path
[WIDTH:     u32 LE   4 bytes]  0 for video
[HEIGHT:    u32 LE   4 bytes]  0 for video
[DATA_SIZE: u64 LE   8 bytes]  supports >4 GiB
[CRC32:     u32 LE   4 bytes]  covers KIND..PAYLOAD
[PAYLOAD:   bytes    variable] planar RGB or raw video bytes
```

### Key Differences from Python Format

| | Python | Rust |
|---|---|---|
| Magic | None | `CMPR` header + footer |
| Path length | u32 (4 bytes) | u16 (2 bytes) |
| Integrity | None | CRC32 per entry + footer |
| Version | None | 2-byte version |
| Extensibility | Monolithic | Flags field, versioned |

---

## Compression Strategy

### Default Mode (Sane, Low RAM)
- **Level:** 19 (no `--ultra`)
- **Window:** 256 MiB (`window_log=28`)
- **LDM:** Enabled (long-distance matching)
- **Threads:** 2
- **RAM budget:** ~512 MiB (hard-capped via `ZSTD_CCtx_setMemoryLimit`)
- **CRC32:** Per-entry + footer

### Max Mode (Explicit Opt-In)
- **Level:** 22 `--ultra`
- **Window:** 2 GiB (`--long=31`)
- **LDM:** Enabled
- **Threads:** all available
- **RAM budget:** Unbounded
- **Flag:** `--max`

### Eco Mode (Tight Machines)
- **Level:** 15
- **Window:** 128 MiB
- **RAM budget:** 256 MiB
- **Flag:** `--mem 256`

ZSTD's memory limit works via `ZSTD_CCtx_setMemoryLimit()` in libzstd — if the requested level + window exceed the budget, ZSTD gracefully downgrades strategy instead of OOM'ing.

---

## Folder-Grouped Stream Ordering (Compression Optimization)

### Why

ZSTD's LDM (Long Distance Matching) finds repeated byte patterns across the input stream. Identical cameras, lighting, and subjects produce **similar planar RGB bytes** — but only if those bytes are close enough in the stream to be within the LDM window.

Sending images from the same folder contiguously maximizes this: two photos from the same vacation day have more in common than one vacation photo and one wedding photo.

### Per-Folder Ordering Rule

Within each folder, entries are emitted in this order:

```
1. All images   (sorted alphabetically)  ← contiguous planar RGB
2. All videos   (sorted alphabetically)  ← contiguous raw bytes
```

This means the byte stream ZSTD sees looks like:

```
photos/trip/beach.jpg      ← planar RGB
photos/trip/sunset.png     ← planar RGB  (same folder = similar pixels)
photos/trip/beach.mp4      ← raw bytes
photos/trip/timelapse.mp4  ← raw bytes
photos/wedding/cake.jpg    ← planar RGB  (different folder = different scene)
...
```

### Implementation

In `packer.rs` within each `walkdir` directory entry:

```rust
// Collect files per directory, classify once
let mut images: Vec<DirEntry> = Vec::new();
let mut videos: Vec<DirEntry> = Vec::new();

for entry in dir_entries {
    if is_image(&entry) { images.push(entry); }
    else if is_video(&entry) { videos.push(entry); }
}

// Sort each group by filename for deterministic order
images.sort_by_key(|e| e.file_name());
videos.sort_by_key(|e| e.file_name());

// Emit all images first (planar RGB), then videos (raw bytes)
for entry in images { write_image(&mut encoder, entry)?; }
for entry in videos { write_video(&mut encoder, entry)?; }
```

This ensures ZSTD always sees a solid block of planar RGB data from the same folder before switching to video bytes.

### Cross-Folder Considerations

The ordering is **per-directory**, not global. If you have deeply nested trees:

```
photos/trip/day1/   → all images, then all videos
photos/trip/day2/   → all images, then all videos
photos/wedding/     → all images, then all videos
```

The `walkdir` crate traverses depth-first (like `os.walk`), so `photos/trip/day1/*` comes before `photos/trip/day2/*`, and `photos/trip/*` before `photos/wedding/*`. This naturally clusters similar scenes together at every level.

---

## CLI Interface

```bash
compr pack [--max|--mem <MB>] <input_dir> <output.compr>
compr unpack <input.compr> <output_dir>
compr list <input.compr>
compr info <input.compr>
compr verify <input.compr>

# Streaming modes
compr pack ./photos - | zstd -o archive.zst
zstd -d -c archive.zst | compr unpack ./output
```

---

## Implementation Phases

### Phase 1: Rust Scaffold (~1-2 days)
- Cargo.toml with all dependencies
- format.rs — binary format definitions + CRC32
- main.rs — clap CLI skeleton
- Basic pack/unpack with raw file copy (no images)

### Phase 2: Image Pipeline (~1-2 days)
- image.rs — load + planarize via image crate
- classify.rs — image vs video detection
- packer.rs — walkdir + encode + write
- unpacker.rs — decode + de-planarize + recreate dirs

### Phase 3: ZSTD + RAM Control (~2-3 days)
- zstd streaming encoder/decoder integration
- ZSTD_CCtx_setMemoryLimit() via zstd-safe
- --mem, --max, --level CLI flags
- Pipe mode (stdin/stdout)

### Phase 4: Polish + Testing (~1-2 days)
- list, verify, info commands
- 40+ integration tests
- Path traversal protection
- Error handling (anyhow/thiserror)
- entropy.rs utility

**Total: ~1 week for v1.**

---

## Key Design Decisions

1. **Not backward-compatible** — new format with magic, version, CRC32. Old Python archives stay readable via the old Python tool.
2. **Skip SREP** — ZSTD LDM provides sufficient cross-file deduplication. No external dedup tool needed.
3. **Conservative RAM default** — ~512 MB instead of the Python tool's 4-8+ GB. Max mode available via `--max`.
4. **Folder-grouped stream ordering** — images before videos within each directory, so ZSTD sees contiguous planar RGB data from the same folder. Maximizes LDM match probability for similar scenes.
5. **Streaming-first** — no TOC/index in v1. Archives are sequential. If seeking is needed later, add an optional index block.
6. **Test parity** — same test coverage as Python version (flat/nested folders, mixed media, pixel fidelity, edge cases).
