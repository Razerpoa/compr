use std::io::{Read, Write};
use thiserror::Error;

pub const MAGIC: &[u8; 4] = b"CMPR";
pub const VERSION: u16 = 0x0003;
pub const MARKER_IMAGE: u8 = 0x01;
pub const MARKER_VIDEO: u8 = 0x02;
pub const MARKER_SOLID_BLOCK: u8 = 0x03;
pub const FOOTER_MARKER: u8 = 0xFF; // unambiguous vs 0x01/0x02
pub const FLAG_ZSTD: u16 = 0x0001; // archive payload is ZSTD-compressed
pub const FLAG_SREP: u16 = 0x0002; // archive payload is SREP-compressed

#[derive(Debug, Clone, PartialEq)]
pub struct ArchiveHeader {
    pub magic: [u8; 4],
    pub version: u16,
    pub flags: u16,
}

impl ArchiveHeader {
    pub fn write<W: Write>(&self, w: &mut W) -> Result<(), FormatError> {
        w.write_all(&self.magic)?;
        w.write_all(&self.version.to_le_bytes())?;
        w.write_all(&self.flags.to_le_bytes())?;
        Ok(())
    }

    pub fn read<R: Read>(r: &mut R) -> Result<Self, FormatError> {
        let mut magic = [0u8; 4];
        r.read_exact(&mut magic)?;
        if magic != *MAGIC { return Err(FormatError::InvalidMagic(magic)); }
        let mut buf = [0u8; 2];
        r.read_exact(&mut buf)?;
        let version = u16::from_le_bytes(buf);
        r.read_exact(&mut buf)?;
        let flags = u16::from_le_bytes(buf);
        Ok(ArchiveHeader { magic, version, flags })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub kind: u8,
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub filter_type: u8,
    pub data: Vec<u8>,
}

impl Entry {
    fn path_len16(&self) -> Result<u16, FormatError> {
        let p = self.path.as_bytes();
        u16::try_from(p.len()).map_err(|_| FormatError::PathTooLong(p.len()))
    }

    pub fn calculate_crc32(&self) -> Result<u32, FormatError> {
        let p = self.path.as_bytes();
        let path_len = self.path_len16()?;
        let data_len = u64::try_from(self.data.len()).map_err(|_| FormatError::DataTooLarge(self.data.len()))?;
        let mut h = crc32fast::Hasher::new();
        h.update(&[self.kind]);
        h.update(&path_len.to_le_bytes());
        h.update(p);
        h.update(&self.width.to_le_bytes());
        h.update(&self.height.to_le_bytes());
        h.update(&[self.filter_type]);
        h.update(&data_len.to_le_bytes());
        h.update(&self.data);
        Ok(h.finalize())
    }

    pub fn write<W: Write>(&self, w: &mut W) -> Result<u64, FormatError> {
        let p = self.path.as_bytes();
        let path_len = self.path_len16()?;
        let crc = self.calculate_crc32()?;
        let mut t: u64 = 0;
        w.write_all(&[self.kind])?; t += 1;
        w.write_all(&path_len.to_le_bytes())?; t += 2;
        w.write_all(p)?; t += p.len() as u64;
        w.write_all(&self.width.to_le_bytes())?; t += 4;
        w.write_all(&self.height.to_le_bytes())?; t += 4;
        w.write_all(&[self.filter_type])?; t += 1;
        w.write_all(&(self.data.len() as u64).to_le_bytes())?; t += 8;
        w.write_all(&crc.to_le_bytes())?; t += 4;
        w.write_all(&self.data)?; t += self.data.len() as u64;
        Ok(t)
    }

    /// Read an entry from a stream. Used in unit tests for round-trip verification.
    #[allow(dead_code)]
    pub fn read<R: Read>(r: &mut R) -> Result<Self, FormatError> {
        let mut buf = [0u8; 1];
        r.read_exact(&mut buf)?;
        let kind = buf[0];
        if kind != MARKER_IMAGE && kind != MARKER_VIDEO {
            return Err(FormatError::UnknownKind(kind));
        }
        let mut pl = [0u8; 2];
        r.read_exact(&mut pl)?;
        let plen = u16::from_le_bytes(pl) as usize;
        let mut pb = vec![0u8; plen];
        r.read_exact(&mut pb)?;
        let path = String::from_utf8(pb).map_err(|_| FormatError::InvalidPath)?;
        let mut b4 = [0u8; 4];
        r.read_exact(&mut b4)?; let w = u32::from_le_bytes(b4);
        r.read_exact(&mut b4)?; let h = u32::from_le_bytes(b4);
        let mut b1 = [0u8; 1];
        r.read_exact(&mut b1)?; let filter_type = b1[0];
        let mut b8 = [0u8; 8];
        r.read_exact(&mut b8)?; let ds = u64::from_le_bytes(b8) as usize;
        r.read_exact(&mut b4)?; let sc = u32::from_le_bytes(b4);
        let mut data = vec![0u8; ds];
        r.read_exact(&mut data)?;
        let entry = Entry { kind, path, width: w, height: h, filter_type, data };
        let cc = entry.calculate_crc32()?;
        if sc != cc { return Err(FormatError::CrcMismatch { expected: sc, computed: cc }); }
        Ok(entry)
    }
}

/// Reject entry paths containing `..` components to prevent
/// CWE-22 path traversal attacks. Called before any file I/O.
pub fn is_path_traversal(path: &str) -> bool {
    use std::path::Component;
    std::path::Path::new(path).components().any(|c| c == Component::ParentDir)
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArchiveFooter {
    pub entry_count: u32,
    pub crc32: u32,
}

impl ArchiveFooter {
    pub fn write<W: Write>(&self, w: &mut W) -> Result<(), FormatError> {
        w.write_all(&[FOOTER_MARKER])?;
        w.write_all(&self.entry_count.to_le_bytes())?;
        w.write_all(&self.crc32.to_le_bytes())?;
        w.write_all(MAGIC)?;
        Ok(())
    }

    /// Read footer from a stream. Used in unit tests for round-trip verification.
    #[allow(dead_code)]
    pub fn read<R: Read>(r: &mut R) -> Result<Self, FormatError> {
        let mut b = [0u8; 1];
        r.read_exact(&mut b)?;
        if b[0] != FOOTER_MARKER { return Err(FormatError::ExpectedFooter { got: b[0] }); }
        let mut eb = [0u8; 4];
        r.read_exact(&mut eb)?;
        let ec = u32::from_le_bytes(eb);
        let cc = { let mut h = crc32fast::Hasher::new(); h.update(&eb); h.finalize() };
        r.read_exact(&mut eb)?;
        let fc = u32::from_le_bytes(eb);
        if fc != cc { return Err(FormatError::CrcMismatch { expected: fc, computed: cc }); }
        r.read_exact(&mut eb)?;
        if eb != *MAGIC { return Err(FormatError::InvalidMagic(eb)); }
        Ok(ArchiveFooter { entry_count: ec, crc32: fc })
    }

    pub fn compute_crc32(entry_count: u32) -> u32 {
        let mut h = crc32fast::Hasher::new();
        h.update(&entry_count.to_le_bytes());
        h.finalize()
    }
}

#[derive(Error, Debug)]
pub enum FormatError {
    #[error("Invalid magic: {0:?}")] InvalidMagic([u8; 4]),
    #[allow(dead_code)] #[error("Unknown entry kind: {0}")] UnknownKind(u8),
    #[allow(dead_code)] #[error("Expected footer marker 0xFF, got {got}")] ExpectedFooter { got: u8 },
    #[allow(dead_code)] #[error("CRC32 mismatch: expected {expected:#x}, computed {computed:#x}")] CrcMismatch { expected: u32, computed: u32 },
    #[allow(dead_code)] #[error("Invalid UTF-8 path")] InvalidPath,
    #[allow(dead_code)] #[error("Path too long: {0} bytes (max 65535)")] PathTooLong(usize),
    #[allow(dead_code)] #[error("Data too large: {0} bytes (max u64)")] DataTooLarge(usize),
    #[error("IO: {0}")] Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn hdr_rt() {
        let h = ArchiveHeader { magic: *MAGIC, version: VERSION, flags: 0 };
        let mut b = Vec::new(); h.write(&mut b).unwrap();
        assert_eq!(ArchiveHeader::read(&mut b.as_slice()).unwrap(), h);
    }

    #[test] fn hdr_bad_magic() {
        assert!(ArchiveHeader::read(&mut b"XXXX\x01\x00\x00\x00".as_slice()).is_err());
    }

    #[test] fn entry_rt() {
        let e = Entry { kind: MARKER_VIDEO, path: "a/b.mp4".into(), width: 0, height: 0, filter_type: 0, data: vec![1,2,3] };
        let mut b = Vec::new(); e.write(&mut b).unwrap();
        assert_eq!(Entry::read(&mut b.as_slice()).unwrap(), e);
    }

    #[test] fn entry_crc() {
        let e = Entry { kind: MARKER_VIDEO, path: "x.bin".into(), width: 0, height: 0, filter_type: 0, data: vec![0xAB; 100] };
        let mut b = Vec::new(); e.write(&mut b).unwrap();
        // Corrupt the last payload byte
        let payload_end = b.len();
        let data_start = payload_end - 100;
        b[data_start + 50] ^= 0xFF;
        assert!(matches!(Entry::read(&mut b.as_slice()), Err(FormatError::CrcMismatch{..})));
    }

    #[test] fn bad_kind() {
        assert!(matches!(Entry::read(&mut b"\xFF\x00\x00".as_slice()), Err(FormatError::UnknownKind(0xFF))));
    }

    #[test] fn footer_rt() {
        let f = ArchiveFooter { entry_count: 42, crc32: ArchiveFooter::compute_crc32(42) };
        let mut b = Vec::new(); f.write(&mut b).unwrap();
        assert_eq!(b.len(), 13);
        assert_eq!(ArchiveFooter::read(&mut b.as_slice()).unwrap(), f);
    }

    #[test] fn footer_marker_no_collision() {
        assert_ne!(FOOTER_MARKER, MARKER_IMAGE);
        assert_ne!(FOOTER_MARKER, MARKER_VIDEO);
    }

    #[test] fn path_traversal_detected() {
        assert!(is_path_traversal("../../etc/shadow"));
        assert!(is_path_traversal("a/../../../b"));
        assert!(!is_path_traversal("normal/path/file.txt"));
        assert!(!is_path_traversal("just_a_file.txt"));
    }
}
