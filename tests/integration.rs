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

    write_file(src.path(), "a.txt", b"hello");
    write_file(src.path(), "b.txt", b"world");

    run_pack(src.path(), &archive);
    run_unpack(&archive, dst.path());

    assert_eq!(fs::read(dst.path().join("a.txt")).unwrap(), b"hello");
    assert_eq!(fs::read(dst.path().join("b.txt")).unwrap(), b"world");
}

#[test]
fn test_nested_round_trip() {
    let src = TempDir::new().unwrap();
    let dst = TempDir::new().unwrap();
    let archive_dir = TempDir::new().unwrap();
    let archive = archive_dir.path().join("test.compr");

    write_file(src.path(), "root.txt", b"root");
    write_file(src.path(), "sub1/nested.txt", b"nested");
    write_file(src.path(), "sub1/sub2/deep.txt", b"deep");

    run_pack(src.path(), &archive);
    run_unpack(&archive, dst.path());

    assert_eq!(fs::read(dst.path().join("root.txt")).unwrap(), b"root");
    assert_eq!(fs::read(dst.path().join("sub1/nested.txt")).unwrap(), b"nested");
    assert_eq!(fs::read(dst.path().join("sub1/sub2/deep.txt")).unwrap(), b"deep");
}

#[test]
fn test_large_file_round_trip() {
    let src = TempDir::new().unwrap();
    let dst = TempDir::new().unwrap();
    let archive_dir = TempDir::new().unwrap();
    let archive = archive_dir.path().join("test.compr");

    let large_data = (0..65536).map(|i| (i % 256) as u8).collect::<Vec<_>>();
    write_file(src.path(), "large.bin", &large_data);

    run_pack(src.path(), &archive);
    run_unpack(&archive, dst.path());

    assert_eq!(fs::read(dst.path().join("large.bin")).unwrap(), large_data);
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

    write_file(src.path(), "a.txt", b"hello");
    write_file(src.path(), "sub/b.txt", b"world");

    run_pack(src.path(), &archive);

    let output = Command::new("cargo")
        .args(["run", "--", "list", archive.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("a.txt"));
    assert!(stdout.contains("sub/b.txt"));
}

#[test]
fn test_verify_valid() {
    let src = TempDir::new().unwrap();
    let archive_dir = TempDir::new().unwrap();
    let archive = archive_dir.path().join("test.compr");

    write_file(src.path(), "f.txt", b"data");
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

    write_file(src.path(), "f.txt", b"data");
    run_pack(src.path(), &archive);

    // Corrupt a byte in the archive's first payload
    let mut data = fs::read(&archive).unwrap();
    // Packed format: header(8) + entry: kind(1) + path_len(2) + path("f.txt"=5) + w(4) + h(4) + ds(8) + crc(4)
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

    write_file(src.path(), "a.txt", b"hello pipe");
    write_file(src.path(), "sub/b.txt", b"nested pipe");

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

    assert_eq!(fs::read(dst.path().join("a.txt")).unwrap(), b"hello pipe");
    assert_eq!(fs::read(dst.path().join("sub/b.txt")).unwrap(), b"nested pipe");
}
