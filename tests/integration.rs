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
    data.extend_from_slice(&1u16.to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes());
    // Entry: kind=VIDEO(0x02), path="../../etc/pwned"
    data.push(0x02);
    let evil_path = "../../etc/pwned";
    data.extend_from_slice(&(evil_path.len() as u16).to_le_bytes());
    data.extend_from_slice(evil_path.as_bytes());
    data.extend_from_slice(&0u32.to_le_bytes()); // width
    data.extend_from_slice(&0u32.to_le_bytes()); // height
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

    assert!(pos_png < pos_mp4, "image1.png should appear before video.mp4");
    assert!(pos_jpg < pos_mp4, "image2.jpg should appear before video.mp4");
    assert!(pos_png < pos_avi, "image1.png should appear before video2.avi");
    assert!(pos_jpg < pos_avi, "image2.jpg should appear before video2.avi");
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
