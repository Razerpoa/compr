use std::io::{self, Write, Read};
use std::collections::HashMap;

const MIN_MATCH: usize = 32;
const TAG_LITERAL: u8 = 0;
const TAG_MATCH: u8 = 1;

/// SREP-like preprocessor writer.
/// Finds long matches in the input stream and replaces them with (offset, length) pairs.
/// This implementation uses an in-memory dictionary that grows with the input.
pub struct SrepWriter<W: Write> {
    inner: W,
    history: Vec<u8>,
    hash_table: HashMap<u64, u64>,
    pending_literals: usize,
    pos: usize,
}

impl<W: Write> SrepWriter<W> {
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            history: Vec::with_capacity(1024 * 1024),
            hash_table: HashMap::with_capacity(1024 * 128),
            pending_literals: 0,
            pos: 0,
        }
    }

    fn write_vint(&mut self, mut n: u64) -> io::Result<()> {
        let mut buf = [0u8; 10];
        let mut i = 0;
        while n >= 0x80 {
            buf[i] = (n as u8) | 0x80;
            n >>= 7;
            i += 1;
        }
        buf[i] = n as u8;
        self.inner.write_all(&buf[..=i])
    }

    fn emit_literal(&mut self, length: usize) -> io::Result<()> {
        if length == 0 { return Ok(()); }
        let mut start = self.pos - length;
        while start < self.pos {
            let chunk_len = (self.pos - start).min(65536);
            self.inner.write_all(&[TAG_LITERAL])?;
            self.write_vint(chunk_len as u64)?;
            self.inner.write_all(&self.history[start..start + chunk_len])?;
            start += chunk_len;
        }
        Ok(())
    }

    fn emit_match(&mut self, offset: usize, length: usize) -> io::Result<()> {
        self.inner.write_all(&[TAG_MATCH])?;
        self.write_vint(offset as u64)?;
        self.write_vint(length as u64)?;
        Ok(())
    }

    pub fn finish(&mut self) -> io::Result<()> {
        self.process_buffer(true)?;
        self.inner.flush()
    }

    fn process_buffer(&mut self, final_flush: bool) -> io::Result<()> {
        while self.pos + MIN_MATCH <= self.history.len() {
            // Use an 8-byte sliding window for hashing
            let chunk = &self.history[self.pos..self.pos + 8];
            let hash = u64::from_le_bytes(chunk.try_into().unwrap());

            let mut found_match = false;
            if let Some(&match_pos) = self.hash_table.get(&hash) {
                let match_pos = match_pos as usize;
                if match_pos < self.pos {
                // Verify and extend match
                let mut m_len = 0;
                while self.pos + m_len < self.history.len() &&
                      self.history[match_pos + m_len] == self.history[self.pos + m_len] {
                    m_len += 1;
                }

                if m_len >= MIN_MATCH {
                    // 1. Emit literals since last match
                    self.emit_literal(self.pending_literals)?;
                    self.pending_literals = 0;

                    // 2. Emit match reference
                    let offset = self.pos - match_pos;
                    self.emit_match(offset, m_len)?;

                    // 3. Update dictionary for the matched content (at least some of it)
                    // We only insert at the start and end of match to save time/memory,
                    // or skip entirely. Here we skip.
                    self.pos += m_len;
                    found_match = true;
                }
                }
            }

            if !found_match {
                self.hash_table.insert(hash, self.pos as u64);
                self.pos += 1;
                self.pending_literals += 1;
            }
        }

        if final_flush && self.pos < self.history.len() {
            self.pending_literals += self.history.len() - self.pos;
            self.pos = self.history.len();
            self.emit_literal(self.pending_literals)?;
            self.pending_literals = 0;
        }

        Ok(())
    }
}

impl<W: Write> Write for SrepWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.history.extend_from_slice(buf);
        self.process_buffer(false)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// SREP-like preprocessor reader.
pub struct SrepReader<R: Read> {
    inner: R,
    history: Vec<u8>,
    output_buffer: Vec<u8>,
    output_pos: usize,
}

impl<R: Read> SrepReader<R> {
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            history: Vec::with_capacity(1024 * 1024),
            output_buffer: Vec::new(),
            output_pos: 0,
        }
    }

    fn read_vint(&mut self) -> io::Result<u64> {
        let mut res = 0u64;
        let mut shift = 0;
        loop {
            let mut b = [0u8; 1];
            self.inner.read_exact(&mut b)?;
            res |= ((b[0] & 0x7F) as u64) << shift;
            if b[0] & 0x80 == 0 { break; }
            shift += 7;
        }
        Ok(res)
    }
}

impl<R: Read> Read for SrepReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut total_read = 0;

        while total_read < buf.len() {
            if self.output_pos < self.output_buffer.len() {
                let len = (self.output_buffer.len() - self.output_pos).min(buf.len() - total_read);
                buf[total_read..total_read + len].copy_from_slice(&self.output_buffer[self.output_pos..self.output_pos + len]);
                self.output_pos += len;
                total_read += len;
                continue;
            }

            let mut tag = [0u8; 1];
            match self.inner.read_exact(&mut tag) {
                Ok(_) => {},
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e),
            }

            match tag[0] {
                TAG_LITERAL => {
                    let len = self.read_vint()? as usize;
                    if len > 1024 * 1024 * 64 {
                        return Err(io::Error::new(io::ErrorKind::InvalidData, "SREP literal too large"));
                    }
                    let mut data = vec![0u8; len];
                    self.inner.read_exact(&mut data)?;
                    self.history.extend_from_slice(&data);

                    let to_copy = len.min(buf.len() - total_read);
                    buf[total_read..total_read + to_copy].copy_from_slice(&data[..to_copy]);
                    if to_copy < len {
                        self.output_buffer = data;
                        self.output_pos = to_copy;
                    } else {
                        self.output_buffer.clear();
                        self.output_pos = 0;
                    }
                    total_read += to_copy;
                }
                TAG_MATCH => {
                    let offset = self.read_vint()? as usize;
                    let length = self.read_vint()? as usize;

                    if length > 1024 * 1024 * 64 {
                        return Err(io::Error::new(io::ErrorKind::InvalidData, "SREP match too large"));
                    }
                    if offset > self.history.len() || offset == 0 {
                        return Err(io::Error::new(io::ErrorKind::InvalidData, "SREP offset out of bounds"));
                    }

                    let mut data = Vec::with_capacity(length);
                    for _ in 0..length {
                        let b = self.history[self.history.len() - offset];
                        self.history.push(b);
                        data.push(b);
                    }

                    let to_copy = length.min(buf.len() - total_read);
                    buf[total_read..total_read + to_copy].copy_from_slice(&data[..to_copy]);
                    if to_copy < length {
                        self.output_buffer = data;
                        self.output_pos = to_copy;
                    } else {
                        self.output_buffer.clear();
                        self.output_pos = 0;
                    }
                    total_read += to_copy;
                }
                _ => return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid SREP tag")),
            }
        }

        Ok(total_read)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_srep_roundtrip() {
        let mut data = Vec::new();
        data.extend_from_slice(b"Hello world! This is a test of SREP. ");
        data.extend_from_slice(b"Hello world! This is a test of SREP. "); // Match
        data.extend_from_slice(b"Some other data here. ");
        data.extend_from_slice(b"Hello world! This is a test of SREP. "); // Match again

        let mut compressed = Vec::new();
        {
            let mut writer = SrepWriter::new(&mut compressed);
            writer.write_all(&data).unwrap();
            writer.finish().unwrap();
        }

        assert!(compressed.len() < data.len());

        let mut decompressed = Vec::new();
        {
            let mut reader = SrepReader::new(&compressed[..]);
            reader.read_to_end(&mut decompressed).unwrap();
        }

        assert_eq!(data, decompressed);
    }
}
