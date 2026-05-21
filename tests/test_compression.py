import os
import sys
import io
import struct
import subprocess
from pathlib import Path

import pytest
import numpy as np
from PIL import Image

# Ensure project root is in sys.path for the compr package
_project_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _project_root not in sys.path:
    sys.path.insert(0, _project_root)

from compr import (
    pack_to_stream, unpack_from_stream,
    build_solid_archive, extract_solid_archive,
    is_image_file, is_video_file,
)


# ── Helpers ──────────────────────────────────────────────────────────────────


def create_test_image(path, width=10, height=10, color=(255, 0, 0)):
    path.parent.mkdir(parents=True, exist_ok=True)
    pixels = np.zeros((height, width, 3), dtype=np.uint8)
    pixels[:, :] = color
    pixels[0, 0] = [(color[0] + 50) % 256, (color[1] + 50) % 256, (color[2] + 50) % 256]
    Image.fromarray(pixels).save(path)


def create_test_video(path, content=None):
    path.parent.mkdir(parents=True, exist_ok=True)
    if content is None:
        content = b'\x00\x00\x00\x18ftypmp42\x00\x00\x00\x00mp42\x00\x00\x00\x00'
    path.write_bytes(content)


def pack_unpack_stream(src_dir, dst_dir):
    """Helper: pack bytes to BytesIO, then unpack from BytesIO."""
    buf = io.BytesIO()
    pack_to_stream(src_dir, buf)
    buf.seek(0)
    unpack_from_stream(buf, dst_dir)
    return buf


# ── File Classification ──────────────────────────────────────────────────────


class TestFileClassification:
    def test_png_is_image(self, tmp_path):
        p = tmp_path / "t.png"
        create_test_image(p)
        assert is_image_file(str(p)) is True

    def test_jpg_is_image(self, tmp_path):
        p = tmp_path / "t.jpg"
        create_test_image(p)
        assert is_image_file(str(p)) is True

    def test_webp_is_image(self, tmp_path):
        p = tmp_path / "t.webp"
        create_test_image(p)
        assert is_image_file(str(p)) is True

    def test_bmp_is_image(self, tmp_path):
        p = tmp_path / "t.bmp"
        create_test_image(p)
        assert is_image_file(str(p)) is True

    def test_tiff_is_image(self, tmp_path):
        p = tmp_path / "t.tiff"
        create_test_image(p)
        assert is_image_file(str(p)) is True

    def test_gif_is_image(self, tmp_path):
        p = tmp_path / "t.gif"
        create_test_image(p)
        assert is_image_file(str(p)) is True

    def test_mp4_is_not_image(self, tmp_path):
        p = tmp_path / "t.mp4"
        create_test_video(p)
        assert is_image_file(str(p)) is False

    def test_text_is_not_image(self, tmp_path):
        p = tmp_path / "t.txt"
        p.write_text("hello")
        assert is_image_file(str(p)) is False

    def test_mp4_is_video(self, tmp_path):
        p = tmp_path / "t.mp4"
        p.write_bytes(b'')
        assert is_video_file(str(p)) is True

    def test_avi_is_video(self, tmp_path):
        p = tmp_path / "t.avi"
        p.write_bytes(b'')
        assert is_video_file(str(p)) is True

    def test_mkv_is_video(self, tmp_path):
        p = tmp_path / "t.mkv"
        p.write_bytes(b'')
        assert is_video_file(str(p)) is True

    def test_mov_is_video(self, tmp_path):
        p = tmp_path / "t.mov"
        p.write_bytes(b'')
        assert is_video_file(str(p)) is True

    def test_wmv_is_video(self, tmp_path):
        p = tmp_path / "t.wmv"
        p.write_bytes(b'')
        assert is_video_file(str(p)) is True

    def test_flv_is_video(self, tmp_path):
        p = tmp_path / "t.flv"
        p.write_bytes(b'')
        assert is_video_file(str(p)) is True

    def test_webm_is_video(self, tmp_path):
        p = tmp_path / "t.webm"
        p.write_bytes(b'')
        assert is_video_file(str(p)) is True

    def test_m4v_is_video(self, tmp_path):
        p = tmp_path / "t.m4v"
        p.write_bytes(b'')
        assert is_video_file(str(p)) is True

    def test_mpg_is_video(self, tmp_path):
        p = tmp_path / "t.mpg"
        p.write_bytes(b'')
        assert is_video_file(str(p)) is True

    def test_mpeg_is_video(self, tmp_path):
        p = tmp_path / "t.mpeg"
        p.write_bytes(b'')
        assert is_video_file(str(p)) is True

    def test_png_is_not_video(self, tmp_path):
        p = tmp_path / "t.png"
        create_test_image(p)
        assert is_video_file(str(p)) is False

    def test_uppercase_extension(self, tmp_path):
        p = tmp_path / "clip.MP4"
        p.write_bytes(b'')
        assert is_video_file(str(p)) is True

    def test_3gp_is_video(self, tmp_path):
        p = tmp_path / "clip.3gp"
        p.write_bytes(b'')
        assert is_video_file(str(p)) is True

    def test_ogv_is_video(self, tmp_path):
        p = tmp_path / "clip.ogv"
        p.write_bytes(b'')
        assert is_video_file(str(p)) is True


# ── Streaming Round-Trip ─────────────────────────────────────────────────────


class TestStreamRoundTrip:
    def test_flat_images(self, tmp_path):
        src, dst = tmp_path / "src", tmp_path / "dst"
        src.mkdir()
        create_test_image(src / "red.png", 10, 10, (255, 0, 0))
        create_test_image(src / "green.jpg", 20, 20, (0, 255, 0))
        create_test_image(src / "blue.webp", 15, 15, (0, 0, 255))

        pack_unpack_stream(src, dst)

        for orig, w, h in [("red.png", 10, 10), ("green.jpg", 20, 20), ("blue.webp", 15, 15)]:
            base = os.path.splitext(orig)[0]
            p = dst / f"{base}.png"
            assert p.exists(), f"Missing {p}"
            with Image.open(p) as img:
                assert img.size == (w, h), f"Size mismatch {orig}"

    def test_nested_structure(self, tmp_path):
        src, dst = tmp_path / "src", tmp_path / "dst"
        create_test_image(src / "root.png")
        create_test_image(src / "sub1" / "nested.jpg")
        create_test_image(src / "sub1" / "sub2" / "deep.png")

        pack_unpack_stream(src, dst)

        for orig in ["root.png", "sub1/nested.jpg", "sub1/sub2/deep.png"]:
            base = os.path.splitext(orig)[0]
            assert (dst / f"{base}.png").exists(), f"Missing {base}.png"

    def test_deeply_nested(self, tmp_path):
        src, dst = tmp_path / "src", tmp_path / "dst"
        create_test_image(src / "a" / "b" / "c" / "d" / "deep.webp")
        pack_unpack_stream(src, dst)
        assert (dst / "a" / "b" / "c" / "d" / "deep.png").exists()

    def test_mixed_media(self, tmp_path):
        src, dst = tmp_path / "src", tmp_path / "dst"
        src.mkdir()
        create_test_image(src / "photo.png", 5, 5, (100, 150, 200))
        create_test_video(src / "clip.mp4", b"fake mp4 content")
        create_test_video(src / "nested" / "movie.avi", b"fake avi content")
        create_test_image(src / "nested" / "deep" / "pic.webp")

        pack_unpack_stream(src, dst)

        assert (dst / "photo.png").exists()
        assert (dst / "nested" / "deep" / "pic.png").exists()
        assert (dst / "clip.mp4").exists()
        assert (dst / "nested" / "movie.avi").exists()

    def test_video_byte_exact_copy(self, tmp_path):
        src, dst = tmp_path / "src", tmp_path / "dst"
        src.mkdir()
        video_data = bytes(range(256)) * 10
        create_test_video(src / "video.mp4", video_data)

        pack_unpack_stream(src, dst)

        assert (dst / "video.mp4").read_bytes() == video_data

    def test_large_video_streaming(self, tmp_path):
        """Test that videos are streamed properly with a moderately large payload."""
        src, dst = tmp_path / "src", tmp_path / "dst"
        src.mkdir()
        # ~1 MB of video data to test chunked streaming
        video_data = bytes([i % 256 for i in range(1048576)])
        create_test_video(src / "big.bin", video_data)

        # Trick: use .mp4 extension to be recognized as video
        (src / "big.mp4").write_bytes(video_data)
        (src / "big.bin").unlink()

        pack_unpack_stream(src, dst)

        assert (dst / "big.mp4").read_bytes() == video_data

    def test_pixel_fidelity(self, tmp_path):
        src, dst = tmp_path / "src", tmp_path / "dst"
        src.mkdir()
        w, h = 32, 32
        pixels = np.zeros((h, w, 3), dtype=np.uint8)
        for y in range(h):
            for x in range(w):
                pixels[y, x] = [x * 8 % 256, y * 8 % 256, (x + y) * 4 % 256]
        Image.fromarray(pixels).save(src / "gradient.png")

        pack_unpack_stream(src, dst)

        restored = np.array(Image.open(dst / "gradient.png"))
        assert np.array_equal(pixels, restored), "Pixel mismatch"

    def test_grayscale_image(self, tmp_path):
        """Grayscale images handled by PIL's .convert('RGB')."""
        src, dst = tmp_path / "src", tmp_path / "dst"
        src.mkdir()
        img = Image.fromarray(np.ones((8, 8), dtype=np.uint8) * 128, mode='L')
        img.save(src / "gray.png")

        pack_unpack_stream(src, dst)

        assert (dst / "gray.png").exists()
        with Image.open(dst / "gray.png") as restored:
            assert restored.mode == "RGB"
            assert restored.size == (8, 8)

    def test_rgba_image(self, tmp_path):
        """RGBA images handled by PIL's .convert('RGB')."""
        src, dst = tmp_path / "src", tmp_path / "dst"
        src.mkdir()
        rgba = np.zeros((8, 8, 4), dtype=np.uint8)
        rgba[:, :] = [100, 150, 200, 128]
        Image.fromarray(rgba, mode='RGBA').save(src / "with_alpha.png")

        pack_unpack_stream(src, dst)

        assert (dst / "with_alpha.png").exists()
        with Image.open(dst / "with_alpha.png") as restored:
            assert restored.mode == "RGB"

    def test_video_all_formats(self, tmp_path):
        src, dst = tmp_path / "src", tmp_path / "dst"
        src.mkdir()
        exts = [".mp4", ".mkv", ".avi", ".mov", ".wmv", ".flv", ".webm", ".m4v", ".mpg"]
        for ext in exts:
            create_test_video(src / f"v{ext}", f"data for {ext}".encode())

        pack_unpack_stream(src, dst)

        for ext in exts:
            fname = f"v{ext}"
            assert (dst / fname).exists(), f"Missing {fname}"
            assert (dst / fname).read_bytes() == f"data for {ext}".encode()


# ── File-to-File Convenience Mode ────────────────────────────────────────────


class TestFileModeRoundTrip:
    def test_file_mode_round_trip(self, tmp_path):
        src, dst = tmp_path / "src", tmp_path / "dst"
        archive = tmp_path / "archive.compr"
        create_test_image(src / "photo.png", 5, 5, (10, 20, 30))
        create_test_video(src / "nested" / "clip.mp4", b"video bytes")

        build_solid_archive(str(src), str(archive))
        assert archive.exists()

        extract_solid_archive(str(archive), str(dst))
        assert (dst / "photo.png").exists()
        assert (dst / "nested" / "clip.mp4").exists()
        assert (dst / "nested" / "clip.mp4").read_bytes() == b"video bytes"


# ── Edge Cases ───────────────────────────────────────────────────────────────


class TestEdgeCases:
    def test_empty_folder_exits(self, tmp_path):
        src = tmp_path / "empty"
        src.mkdir()
        buf = io.BytesIO()
        with pytest.raises(SystemExit):
            pack_to_stream(src, buf)

    def test_unsupported_only_exits(self, tmp_path):
        src = tmp_path / "src"
        src.mkdir()
        (src / "notes.txt").write_text("hello")
        buf = io.BytesIO()
        with pytest.raises(SystemExit):
            pack_to_stream(src, buf)

    def test_mixed_supported_and_unsupported(self, tmp_path):
        src, dst = tmp_path / "src", tmp_path / "dst"
        src.mkdir()
        create_test_image(src / "photo.png", 5, 5, (0, 0, 0))
        (src / "notes.txt").write_text("hello")

        pack_unpack_stream(src, dst)

        assert (dst / "photo.png").exists()
        assert not (dst / "notes.txt").exists()

    def test_corrupted_image_skipped(self, tmp_path):
        src, dst = tmp_path / "src", tmp_path / "dst"
        src.mkdir()
        create_test_image(src / "valid.png", 5, 5, (0, 0, 0))
        (src / "bad.jpg").write_bytes(b"not an image")

        pack_unpack_stream(src, dst)

        assert (dst / "valid.png").exists()
        assert not (dst / "bad.jpg").exists()
        assert not (dst / "bad.png").exists()

    def test_path_traversal_rejected(self, tmp_path):
        """Malicious rel_path like '../../etc/passwd' must be rejected.
        The implementation uses 'continue' (not raise) to skip bad entries,
        so we assert the malicious file was NOT created on disk."""
        src, dst = tmp_path / "src", tmp_path / "dst"
        src.mkdir()
        create_test_image(src / "safe.png", 5, 5, (0, 0, 0))

        # Pack normally, then manually craft a malicious header
        buf = io.BytesIO()
        pack_to_stream(src, buf)

        # Append a malicious entry with path traversal
        bad_path = "../../etc/passwd"
        bp_bytes = bad_path.encode('utf-8')
        header = struct.pack(
            f"<cIIIQ{len(bp_bytes)}s", b'V', len(bp_bytes), 0, 0, 5, bp_bytes
        )
        buf.write(header + b"hello")
        buf.seek(0)

        # Run unpacker — should skip the bad entry silently
        unpack_from_stream(buf, dst)

        # The safe file MUST be extracted
        assert (dst / "safe.png").exists()
        # The malicious file MUST NOT have been created
        assert not (dst / "etc" / "passwd").exists()
        assert not Path("/tmp/passwd").exists()

    def test_large_image(self, tmp_path):
        src, dst = tmp_path / "src", tmp_path / "dst"
        src.mkdir()
        create_test_image(src / "large.png", 200, 200, (50, 100, 150))

        pack_unpack_stream(src, dst)

        assert (dst / "large.png").exists()
        with Image.open(dst / "large.png") as img:
            assert img.size == (200, 200)

    def test_multiple_image_formats(self, tmp_path):
        src, dst = tmp_path / "src", tmp_path / "dst"
        src.mkdir()
        for ext in [".png", ".jpg", ".webp", ".bmp", ".tiff", ".tif"]:
            create_test_image(src / f"img{ext}", 8, 8, (10, 20, 30))
        Image.fromarray(np.zeros((6, 6, 3), dtype=np.uint8)).save(src / "img.gif")

        pack_unpack_stream(src, dst)

        for orig in [
            "img.png", "img.jpg", "img.webp", "img.bmp",
            "img.tiff", "img.tif", "img.gif"
        ]:
            base = os.path.splitext(orig)[0]
            assert (dst / f"{base}.png").exists(), f"Missing {base}.png"

    def test_invalid_archive_path_exits(self, tmp_path):
        with pytest.raises(SystemExit):
            extract_solid_archive("/nonexistent/path.compr", str(tmp_path))

    def test_data_size_mismatch_skipped(self, tmp_path):
        """A corrupted image header with mismatched data_size should be skipped."""
        src, dst = tmp_path / "src", tmp_path / "dst"
        src.mkdir()
        create_test_image(src / "good.png", 5, 5, (0, 0, 0))

        # Pack, then corrupt one header
        buf = io.BytesIO()
        pack_to_stream(src, buf)

        # Read and modify: find the image header and corrupt data_size
        buf.seek(0)
        data = buf.read()
        # First entry: marker 'I', path_len, width=5, height=5, data_size=75
        # We'll inject a bad header after the valid one
        bad_path = "bad.png"
        bp_bytes = bad_path.encode('utf-8')
        # width=5, height=5, but data_size=10 (wrong, should be 75)
        bad_header = struct.pack(
            f"<cIIIQ{len(bp_bytes)}s",
            b'I', len(bp_bytes), 5, 5, 10, bp_bytes
        )
        data += bad_header + b"\x00" * 10

        buf2 = io.BytesIO(data)
        unpack_from_stream(buf2, dst)

        assert (dst / "good.png").exists()
        # bad.png should NOT have been extracted (data_size mismatch)
        assert not (dst / "bad.png").exists()


# ── CLI Tests ────────────────────────────────────────────────────────────────


class TestCLI:
    def test_no_args_exits(self):
        result = subprocess.run(
            [sys.executable, "-m", "main"],
            capture_output=True, text=True,
            cwd=os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        )
        assert result.returncode != 0
        output = result.stdout + result.stderr
        assert "Usage" in output or "Streaming" in output

    def test_pack_nonexistent_folder_exits(self):
        result = subprocess.run(
            [sys.executable, "-m", "main", "--pack", "/nonexistent_path_12345"],
            capture_output=True, text=True,
            cwd=os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        )
        assert result.returncode != 0

    def test_pack_streaming_roundtrip(self, tmp_path):
        """Test the full CLI pipe: pack -> zstd -> zstd -d -> unpack."""
        src, dst = tmp_path / "src", tmp_path / "dst"
        archive = tmp_path / "archive.zst"
        src.mkdir()
        create_test_image(src / "test.png", 5, 5, (100, 0, 0))
        create_test_video(src / "clip.mp4", b"video data")

        base = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

        # Pack streaming to zstd
        pack_proc = subprocess.Popen(
            [sys.executable, "-m", "main", "--pack", str(src)],
            stdout=subprocess.PIPE, cwd=base
        )
        zstd_proc = subprocess.Popen(
            ["zstd", "-o", str(archive)],
            stdin=pack_proc.stdout, stdout=subprocess.PIPE, stderr=subprocess.PIPE
        )
        pack_proc.stdout.close()
        zstd_proc.communicate()
        assert archive.exists(), "Archive was not created"

        # Unpack through zstd pipe
        zstd_cat = subprocess.Popen(
            ["zstd", "-d", "-c", str(archive)],
            stdout=subprocess.PIPE, cwd=base
        )
        unpack_proc = subprocess.run(
            [sys.executable, "-m", "main", "--unpack", str(dst)],
            stdin=zstd_cat.stdout, capture_output=True, text=True, cwd=base
        )
        zstd_cat.stdout.close()
        zstd_cat.wait()

        assert (dst / "test.png").exists()
        assert (dst / "clip.mp4").exists()
        assert (dst / "clip.mp4").read_bytes() == b"video data"
