use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use tempfile::TempDir;

/// Helper: create test file with content.
fn write_file(dir: &Path, rel: &str, content: &[u8]) {
    let p = dir.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, content).unwrap();
}

/// Helper: run `cargo run -- pack ...` and assert success.
fn run_pack(src: &Path, dst: &Path) {
    let status = Command::new("cargo")
        .args(["run", "--", "pack", src.to_str().unwrap(), dst.to_str().unwrap()])
        .status()
        .unwrap();
    assert!(status.success(), "pack failed");
}

/// Helper: run `cargo run -- pack ...` with extra flags and assert success.
fn run_pack_with_args(src: &Path, dst: &Path, extra_args: &[&str]) {
    let mut args = vec!["run", "--", "pack"];
    args.extend_from_slice(extra_args);
    args.push(src.to_str().unwrap());
    args.push(dst.to_str().unwrap());
    let status = Command::new("cargo")
        .args(&args)
        .status()
        .unwrap();
    assert!(status.success(), "pack with args {:?} failed", extra_args);
}

/// Helper: run `cargo run -- unpack ...` and assert success.
fn run_unpack(src: &Path, dst: &Path) {
    let status = Command::new("cargo")
        .args(["run", "--", "unpack", src.to_str().unwrap(), dst.to_str().unwrap()])
        .status()
        .unwrap();
    assert!(status.success(), "unpack failed");
}

#[test]
fn test_flat_round_trip() {
    let src = TempDir::new().unwrap();
    let dst = TempDir::new().unwrap();
    let archive_dir = TempDir::new().unwrap();
    let archive = archive_dir.path().join("test.compr");

    write_file(src.path(), "a.mp4", b"hello");
    write_file(src.path(), "b.mp4", b"world");

    run_pack(src.path(), &archive);
    run_unpack(&archive, dst.path());

    assert_eq!(fs::read(dst.path().join("a.mp4")).unwrap(), b"hello");
    assert_eq!(fs::read(dst.path().join("b.mp4")).unwrap(), b"world");
}

#[test]
fn test_nested_round_trip() {
    let src = TempDir::new().unwrap();
    let dst = TempDir::new().unwrap();
    let archive_dir = TempDir::new().unwrap();
    let archive = archive_dir.path().join("test.compr");

    write_file(src.path(), "root.mp4", b"root");
    write_file(src.path(), "sub1/nested.mp4", b"nested");
    write_file(src.path(), "sub1/sub2/deep.mp4", b"deep");

    run_pack(src.path(), &archive);
    run_unpack(&archive, dst.path());

    assert_eq!(fs::read(dst.path().join("root.mp4")).unwrap(), b"root");
    assert_eq!(fs::read(dst.path().join("sub1/nested.mp4")).unwrap(), b"nested");
    assert_eq!(fs::read(dst.path().join("sub1/sub2/deep.mp4")).unwrap(), b"deep");
}

#[test]
fn test_large_file_round_trip() {
    let src = TempDir::new().unwrap();
    let dst = TempDir::new().unwrap();
    let archive_dir = TempDir::new().unwrap();
    let archive = archive_dir.path().join("test.compr");

    let large_data = (0..65536).map(|i| (i % 256) as u8).collect::<Vec<_>>();
    write_file(src.path(), "large.mp4", &large_data);

    run_pack(src.path(), &archive);
    run_unpack(&archive, dst.path());

    assert_eq!(fs::read(dst.path().join("large.mp4")).unwrap(), large_data);
}

#[test]
fn test_empty_folder_pack_fails() {
    let src = TempDir::new().unwrap();
    let archive_dir = TempDir::new().unwrap();
    let archive = archive_dir.path().join("test.compr");

    let status = Command::new("cargo")
        .args(["run", "--", "pack", src.path().to_str().unwrap(), archive.to_str().unwrap()])
        .status()
        .unwrap();
    assert!(!status.success(), "pack should fail on empty dir");
}

#[test]
fn test_nonexistent_input_fails() {
    let archive_dir = TempDir::new().unwrap();
    let archive = archive_dir.path().join("test.compr");
    let status = Command::new("cargo")
        .args(["run", "--", "pack", "/nonexistent_path_xyz", archive.to_str().unwrap()])
        .status()
        .unwrap();
    assert!(!status.success());
}

#[test]
fn test_nonexistent_archive_fails() {
    let dst = TempDir::new().unwrap();
    let status = Command::new("cargo")
        .args(["run", "--", "unpack", "/nonexistent.compr", dst.path().to_str().unwrap()])
        .status()
        .unwrap();
    assert!(!status.success());
}

#[test]
fn test_list_output() {
    let src = TempDir::new().unwrap();
    let archive_dir = TempDir::new().unwrap();
    let archive = archive_dir.path().join("test.compr");

    write_file(src.path(), "a.mp4", b"hello");
    write_file(src.path(), "sub/b.mp4", b"world");

    run_pack(src.path(), &archive);

    let output = Command::new("cargo")
        .args(["run", "--", "list", archive.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("a.mp4"));
    assert!(stdout.contains("sub/b.mp4"));
}

#[test]
fn test_verify_valid() {
    let src = TempDir::new().unwrap();
    let archive_dir = TempDir::new().unwrap();
    let archive = archive_dir.path().join("test.compr");

    write_file(src.path(), "f.mp4", b"data");
    run_pack(src.path(), &archive);

    let output = Command::new("cargo")
        .args(["run", "--", "verify", archive.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
}

#[test]
fn test_verify_corrupted() {
    let src = TempDir::new().unwrap();
    let archive_dir = TempDir::new().unwrap();
    let archive = archive_dir.path().join("test.compr");

    write_file(src.path(), "f.mp4", b"data");
    run_pack(src.path(), &archive);

    // Corrupt a byte in the archive's first payload
    let mut data = fs::read(&archive).unwrap();
    // Packed format: header(8) + entry: kind(1) + path_len(2) + path("f.mp4"=5) + w(4) + h(4) + ds(8) + crc(4)
    // Payload starts at byte 8 + 1 + 2 + 5 + 4 + 4 + 8 + 4 = 36. Corrupt byte 40 (5th byte of payload).
    let payload_offset = 8 + 1 + 2 + 5 + 4 + 4 + 8 + 4;
    data[payload_offset + 4] ^= 0xFF;
    fs::write(&archive, &data).unwrap();

    let output = Command::new("cargo")
        .args(["run", "--", "verify", archive.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!output.status.success(), "verify should fail on corrupted data");
}

#[test]
fn test_path_traversal_rejected() {
    // Manually craft an archive with a malicious entry path containing `..`
    let archive_dir = TempDir::new().unwrap();
    let archive = archive_dir.path().join("evil.compr");

    // Build archive bytes manually: header + one entry with path "../../etc/pwned"
    let mut data = Vec::new();
    // Header: magic(4) + version(2) + flags(2)
    data.extend_from_slice(b"CMPR");
    data.extend_from_slice(&2u16.to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes());
    // Entry: kind=VIDEO(0x02), path="../../etc/pwned"
    data.push(0x02);
    let evil_path = "../../etc/pwned";
    data.extend_from_slice(&(evil_path.len() as u16).to_le_bytes());
    data.extend_from_slice(evil_path.as_bytes());
    data.extend_from_slice(&0u32.to_le_bytes()); // width
    data.extend_from_slice(&0u32.to_le_bytes()); // height
    data.push(0);                                // filter_type
    data.extend_from_slice(&5u64.to_le_bytes()); // data_size
    data.extend_from_slice(&0u32.to_le_bytes()); // CRC32 (rejected before CRC check)
    data.extend_from_slice(b"hello");           // payload

    fs::write(&archive, &data).unwrap();

    let dst = TempDir::new().unwrap();
    let status = Command::new("cargo")
        .args(["run", "--", "unpack", archive.to_str().unwrap(), dst.path().to_str().unwrap()])
        .status()
        .unwrap();
    assert!(!status.success(), "unpack should reject path traversal");

    // Verify the malicious file was NOT created
    assert!(!dst.path().join("etc").join("pwned").exists(), "malicious file was created!");
    assert!(!Path::new("/tmp/pwned").exists(), "file escaped to /tmp!");
}

#[test]
fn test_streaming_pipe_round_trip() {
    let src = TempDir::new().unwrap();
    let dst = TempDir::new().unwrap();

    write_file(src.path(), "a.mp4", b"hello pipe");
    write_file(src.path(), "sub/b.mp4", b"nested pipe");

    // Pack to stdout
    let mut pack = Command::new("cargo")
        .args(["run", "--", "pack", src.path().to_str().unwrap(), "-"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    // Unpack from stdin
    let unpack = Command::new("cargo")
        .args(["run", "--", "unpack", "-", dst.path().to_str().unwrap()])
        .stdin(pack.stdout.take().unwrap())
        .status()
        .unwrap();
    assert!(unpack.success(), "streaming unpack failed");

    assert_eq!(fs::read(dst.path().join("a.mp4")).unwrap(), b"hello pipe");
    assert_eq!(fs::read(dst.path().join("sub/b.mp4")).unwrap(), b"nested pipe");
}

#[test]
fn test_image_round_trip() {
    let src = TempDir::new().unwrap();
    let dst = TempDir::new().unwrap();
    let archive_dir = TempDir::new().unwrap();
    let archive = archive_dir.path().join("test.compr");

    let mut img = image::RgbImage::new(4, 4);
    for y in 0..4 {
        for x in 0..4 {
            img.put_pixel(x, y, image::Rgb([(x * 64) as u8, (y * 64) as u8, 128]));
        }
    }
    img.save(src.path().join("pattern.png")).unwrap();

    run_pack(src.path(), &archive);
    run_unpack(&archive, dst.path());

    let unpacked = dst.path().join("pattern.png");
    assert!(unpacked.exists(), "unpacked PNG should exist");

    let loaded = image::open(&unpacked).unwrap().to_rgb8();
    assert_eq!(loaded.width(), 4);
    assert_eq!(loaded.height(), 4);
    for y in 0..4 {
        for x in 0..4 {
            assert_eq!(
                loaded.get_pixel(x, y),
                &image::Rgb([(x * 64) as u8, (y * 64) as u8, 128]),
                "Pixel mismatch at ({x},{y})"
            );
        }
    }
}

#[test]
fn test_mixed_content_ordering() {
    let src = TempDir::new().unwrap();
    let dst = TempDir::new().unwrap();
    let archive_dir = TempDir::new().unwrap();
    let archive = archive_dir.path().join("test.compr");

    let img1 = image::RgbImage::new(1, 1);
    img1.save(src.path().join("image1.png")).unwrap();
    let img2 = image::RgbImage::new(2, 2);
    img2.save(src.path().join("image2.jpg")).unwrap();

    fs::write(src.path().join("video.mp4"), b"video data").unwrap();
    fs::write(src.path().join("video2.avi"), b"more video").unwrap();
    fs::write(src.path().join("notes.txt"), b"skip me").unwrap();

    run_pack(src.path(), &archive);

    let output = Command::new("cargo")
        .args(["run", "--", "list", archive.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    let pos_png = stdout.find("image1.png").unwrap();
    let pos_jpg = stdout.find("image2.jpg").unwrap();
    let pos_mp4 = stdout.find("video.mp4").unwrap();
    let pos_avi = stdout.find("video2.avi").unwrap();

    assert!(pos_mp4 < pos_png, "video.mp4 should appear before image1.png");
    assert!(pos_mp4 < pos_jpg, "video.mp4 should appear before image2.jpg");
    assert!(pos_avi < pos_png, "video2.avi should appear before image1.png");
    assert!(pos_avi < pos_jpg, "video2.avi should appear before image2.jpg");
    assert!(!stdout.contains("notes.txt"), "notes.txt should be skipped");

    run_unpack(&archive, dst.path());

    assert!(dst.path().join("image1.png").exists());
    assert!(dst.path().join("image2.png").exists(), "image2.jpg unpacks as image2.png");
    assert!(dst.path().join("video.mp4").exists());
    assert!(dst.path().join("video2.avi").exists());
    assert!(!dst.path().join("notes.txt").exists(), "notes.txt should not be unpacked");
}

#[test]
fn test_image_multiple_formats() {
    let src = TempDir::new().unwrap();
    let dst = TempDir::new().unwrap();
    let archive_dir = TempDir::new().unwrap();
    let archive = archive_dir.path().join("test.compr");

    let mut a = image::RgbImage::new(4, 4);
    a.put_pixel(0, 0, image::Rgb([255, 0, 0]));
    a.put_pixel(3, 3, image::Rgb([0, 255, 0]));
    a.save(src.path().join("a.png")).unwrap();

    let mut b = image::RgbImage::new(8, 8);
    b.put_pixel(0, 0, image::Rgb([0, 0, 255]));
    b.put_pixel(7, 7, image::Rgb([255, 255, 0]));
    b.save(src.path().join("b.jpg")).unwrap();

    let mut c = image::RgbImage::new(16, 16);
    c.put_pixel(0, 0, image::Rgb([128, 128, 0]));
    c.put_pixel(15, 15, image::Rgb([0, 128, 128]));
    c.save(src.path().join("c.webp")).unwrap();

    fs::write(src.path().join("video.mp4"), b"raw video bytes").unwrap();

    run_pack(src.path(), &archive);
    run_unpack(&archive, dst.path());

    let png = dst.path().join("a.png");
    assert!(png.exists());
    assert_eq!(image::open(&png).unwrap().to_rgb8().dimensions(), (4, 4));

    let jpg_png = dst.path().join("b.png");
    assert!(jpg_png.exists());
    assert_eq!(image::open(&jpg_png).unwrap().to_rgb8().dimensions(), (8, 8));

    let webp_png = dst.path().join("c.png");
    assert!(webp_png.exists());
    assert_eq!(image::open(&webp_png).unwrap().to_rgb8().dimensions(), (16, 16));

    assert!(dst.path().join("video.mp4").exists());
}

#[test]
fn test_compressed_round_trip() {
    let src = TempDir::new().unwrap();
    let dst = TempDir::new().unwrap();
    let archive_dir = TempDir::new().unwrap();
    let archive = archive_dir.path().join("test.compr");

    write_file(src.path(), "a.mp4", b"hello compressed");
    write_file(src.path(), "sub/b.mp4", b"nested compressed");

    run_pack(src.path(), &archive);
    run_unpack(&archive, dst.path());

    assert_eq!(fs::read(dst.path().join("a.mp4")).unwrap(), b"hello compressed");
    assert_eq!(fs::read(dst.path().join("sub/b.mp4")).unwrap(), b"nested compressed");
}

#[test]
fn test_max_flag_round_trip() {
    let src = TempDir::new().unwrap();
    let dst = TempDir::new().unwrap();
    let archive_dir = TempDir::new().unwrap();
    let archive = archive_dir.path().join("max.compr");

    write_file(src.path(), "f.mp4", b"data for --max test");
    run_pack_with_args(src.path(), &archive, &["--max"]);
    run_unpack(&archive, dst.path());
    assert_eq!(fs::read(dst.path().join("f.mp4")).unwrap(), b"data for --max test");
}

#[test]
fn test_mem_flag_round_trip() {
    let src = TempDir::new().unwrap();
    let dst = TempDir::new().unwrap();
    let archive_dir = TempDir::new().unwrap();
    let archive = archive_dir.path().join("mem.compr");

    write_file(src.path(), "f.mp4", b"data for --mem test");
    run_pack_with_args(src.path(), &archive, &["--mem", "128"]);
    run_unpack(&archive, dst.path());
    assert_eq!(fs::read(dst.path().join("f.mp4")).unwrap(), b"data for --mem test");
}

#[test]
fn test_level_flag_round_trip() {
    let src = TempDir::new().unwrap();
    let dst = TempDir::new().unwrap();
    let archive_dir = TempDir::new().unwrap();
    let archive = archive_dir.path().join("level.compr");

    write_file(src.path(), "f.mp4", b"data for --level test");
    run_pack_with_args(src.path(), &archive, &["--level", "1"]);
    run_unpack(&archive, dst.path());
    assert_eq!(fs::read(dst.path().join("f.mp4")).unwrap(), b"data for --level test");
}

#[test]
fn test_verify_compressed_archive() {
    let src = TempDir::new().unwrap();
    let archive_dir = TempDir::new().unwrap();
    let archive = archive_dir.path().join("test.compr");

    write_file(src.path(), "f.mp4", b"verify compressed data");
    // Pack with --max to ensure ZSTD is active
    run_pack_with_args(src.path(), &archive, &["--max"]);

    let output = Command::new("cargo")
        .args(["run", "--", "verify", archive.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success(), "verify should pass on valid compressed archive");
}

#[test]
fn test_streaming_pipe_compressed() {
    let src = TempDir::new().unwrap();
    let dst = TempDir::new().unwrap();

    let mut img = image::RgbImage::new(4, 4);
    img.put_pixel(0, 0, image::Rgb([128, 64, 32]));
    img.save(src.path().join("pipe.png")).unwrap();
    write_file(src.path(), "pipe.mp4", b"pipe data");

    // Pack to stdout (compressed by default)
    let mut pack = Command::new("cargo")
        .args(["run", "--", "pack", src.path().to_str().unwrap(), "-"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    // Unpack from stdin (auto-detect compression)
    let unpack = Command::new("cargo")
        .args(["run", "--", "unpack", "-", dst.path().to_str().unwrap()])
        .stdin(pack.stdout.take().unwrap())
        .status()
        .unwrap();
    assert!(unpack.success(), "streaming compressed unpack failed");

    let restored = image::open(dst.path().join("pipe.png")).unwrap().to_rgb8();
    assert_eq!(restored.get_pixel(0, 0), &image::Rgb([128, 64, 32]));
    assert_eq!(fs::read(dst.path().join("pipe.mp4")).unwrap(), b"pipe data");
}

#[test]
fn test_entropy_command() {
    let src = TempDir::new().unwrap();
    let archive_dir = TempDir::new().unwrap();
    let archive = archive_dir.path().join("test.compr");

    write_file(src.path(), "f.mp4", b"AAAAAAAAAAAAAAAA"); // 0 entropy
    run_pack(src.path(), &archive);

    let output = Command::new("cargo")
        .args(["run", "--", "entropy", archive.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("f.mp4"));
    assert!(stdout.contains("bits/byte"));
}

#[test]
fn test_empty_file() {
    let src = TempDir::new().unwrap();
    let dst = TempDir::new().unwrap();
    let archive_dir = TempDir::new().unwrap();
    let archive = archive_dir.path().join("test.compr");

    write_file(src.path(), "empty.mp4", b"");
    run_pack(src.path(), &archive);
    run_unpack(&archive, dst.path());
    assert_eq!(fs::read(dst.path().join("empty.mp4")).unwrap(), b"");
}

#[test]
fn test_single_file() {
    let src = TempDir::new().unwrap();
    let dst = TempDir::new().unwrap();
    let archive_dir = TempDir::new().unwrap();
    let archive = archive_dir.path().join("test.compr");

    write_file(src.path(), "solo.mp4", b"just me");
    run_pack(src.path(), &archive);
    run_unpack(&archive, dst.path());
    assert_eq!(fs::read(dst.path().join("solo.mp4")).unwrap(), b"just me");
}

#[test]
fn test_deeply_nested_paths() {
    let src = TempDir::new().unwrap();
    let dst = TempDir::new().unwrap();
    let archive_dir = TempDir::new().unwrap();
    let archive = archive_dir.path().join("test.compr");

    let deep = (0..10).map(|_| "a").collect::<Vec<_>>().join("/");
    write_file(src.path(), &format!("{deep}/deep.mp4"), b"deep file");
    run_pack(src.path(), &archive);
    run_unpack(&archive, dst.path());
    assert_eq!(
        fs::read(dst.path().join(&format!("{deep}/deep.mp4"))).unwrap(),
        b"deep file"
    );
}

#[test]
fn test_unicode_path() {
    let src = TempDir::new().unwrap();
    let dst = TempDir::new().unwrap();
    let archive_dir = TempDir::new().unwrap();
    let archive = archive_dir.path().join("test.compr");

    write_file(src.path(), "文件夹/文件.mp4", b"unicode");
    run_pack(src.path(), &archive);
    run_unpack(&archive, dst.path());
    assert_eq!(fs::read(dst.path().join("文件夹/文件.mp4")).unwrap(), b"unicode");
}

#[test]
fn test_image_round_trip_compressed_max() {
    let src = TempDir::new().unwrap();
    let dst = TempDir::new().unwrap();
    let archive_dir = TempDir::new().unwrap();
    let archive = archive_dir.path().join("test.compr");

    let mut img = image::RgbImage::new(8, 8);
    for y in 0..8 {
        for x in 0..8 {
            img.put_pixel(x, y, image::Rgb([(x * 32) as u8, (y * 32) as u8, 100]));
        }
    }
    img.save(src.path().join("img.png")).unwrap();

    run_pack_with_args(src.path(), &archive, &["--max"]);
    run_unpack(&archive, dst.path());

    let loaded = image::open(dst.path().join("img.png")).unwrap().to_rgb8();
    assert_eq!(loaded.dimensions(), (8, 8));
    for y in 0..8 {
        for x in 0..8 {
            assert_eq!(loaded.get_pixel(x, y), &image::Rgb([(x * 32) as u8, (y * 32) as u8, 100]));
        }
    }
}

#[test]
fn test_backward_compatibility_fails() {
    let archive_dir = TempDir::new().unwrap();
    let archive = archive_dir.path().join("old.compr");

    // Build version 2 archive bytes manually
    let mut data = Vec::new();
    data.extend_from_slice(b"CMPR");
    data.extend_from_slice(&2u16.to_le_bytes()); // Version 2
    data.extend_from_slice(&0u16.to_le_bytes());

    fs::write(&archive, &data).unwrap();

    let dst = TempDir::new().unwrap();
    let output = Command::new("cargo")
        .args(["run", "--", "unpack", archive.to_str().unwrap(), dst.path().to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("version") || stderr.contains("Version"), "Error should mention 'version', got: {}", stderr);
}

#[test]
fn test_corrupted_filter_type_crc_failure() {
    let src = TempDir::new().unwrap();
    let archive_dir = TempDir::new().unwrap();
    let archive = archive_dir.path().join("test.compr");

    let mut img = image::RgbImage::new(4, 4);
    img.save(src.path().join("img.png")).unwrap();

    // Pack it (version 2, with filter_type: 2 for image)
    run_pack(src.path(), &archive);

    let mut data = fs::read(&archive).unwrap();
    
    // Craft an uncompressed version 3 archive manually:
    let mut manual_data = Vec::new();
    manual_data.extend_from_slice(b"CMPR");
    manual_data.extend_from_slice(&3u16.to_le_bytes()); // Version 3
    manual_data.extend_from_slice(&0u16.to_le_bytes()); // No flags (ZSTD off)
    
    // Entry: kind=MARKER_IMAGE(0x01), path="img.png"
    manual_data.push(0x01);
    let path = "img.png";
    manual_data.extend_from_slice(&(path.len() as u16).to_le_bytes());
    manual_data.extend_from_slice(path.as_bytes());
    
    // width=2, height=2, filter_type=2, data_size=12, CRC32
    manual_data.extend_from_slice(&2u32.to_le_bytes()); // w
    manual_data.extend_from_slice(&2u32.to_le_bytes()); // h
    manual_data.push(2);                                // filter_type = 2 (Paeth)
    
    let payload = vec![0u8; 12];
    let mut h = crc32fast::Hasher::new();
    h.update(&[0x01]);
    let path_len = path.len() as u16;
    h.update(&path_len.to_le_bytes());
    h.update(path.as_bytes());
    h.update(&2u32.to_le_bytes());
    h.update(&2u32.to_le_bytes());
    h.update(&[2]);
    h.update(&12u64.to_le_bytes());
    h.update(&payload);
    let crc = h.finalize();

    manual_data.extend_from_slice(&12u64.to_le_bytes()); // data_size
    manual_data.extend_from_slice(&crc.to_le_bytes());  // CRC32
    manual_data.extend_from_slice(&payload);             // payload
    
    // Footer: FOOTER_MARKER(0xFF) + entry_count(1) + footer_crc(crc32(entry_count)) + MAGIC(CMPR)
    manual_data.push(0xFF);
    manual_data.extend_from_slice(&1u32.to_le_bytes());
    let mut fh = crc32fast::Hasher::new();
    fh.update(&1u32.to_le_bytes());
    let footer_crc = fh.finalize();
    manual_data.extend_from_slice(&footer_crc.to_le_bytes());
    manual_data.extend_from_slice(b"CMPR");

    fs::write(&archive, &manual_data).unwrap();

    // Verify it passes when valid
    let output = Command::new("cargo")
        .args(["run", "--", "verify", archive.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success(), "Verification of manually crafted valid archive failed");

    // Corrupt the filter_type byte!
    // filter_type is at: header(8) + kind(1) + plen(2) + path("img.png"=7) + w(4) + h(4) = 26th byte (index 26)
    manual_data[26] = 0; // change from 2 to 0
    fs::write(&archive, &manual_data).unwrap();

    // Verify it fails due to CRC mismatch!
    let output = Command::new("cargo")
        .args(["run", "--", "verify", archive.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("CRC32") || stderr.contains("CRC"), "Expected CRC failure, got: {}", stderr);
}
