use crate::entry::Offset;
use crate::error::Error;
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::{Cursor, Read, Seek};

#[derive(Debug)]
/// A collection of offsets for duplicate keys.
pub struct PayloadEntry {
    /// Number of duplicates (including the original key)
    pub count: u32,
    /// Offsets for each duplicate
    pub offsets: Vec<Offset>,
}

impl PayloadEntry {
    /// Create an empty payload entry.
    pub fn new() -> Self {
        Self {
            count: 0,
            offsets: Vec::new(),
        }
    }

    /// Add an offset to the entry.
    pub fn add_offset(&mut self, offset: Offset) {
        self.offsets.push(offset);
        self.count += 1;
    }

    /// Serialized size: 4 bytes for count + 8 bytes per offset.
    pub fn serialized_size(&self) -> usize {
        4 + (self.count as usize * 8)
    }

    /// Serialize into a byte buffer (little-endian).
    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(self.serialized_size());
        buf.write_u32::<LittleEndian>(self.count).unwrap();
        for &off in &self.offsets {
            buf.write_u64::<LittleEndian>(off).unwrap();
        }
        buf
    }

    /// Deserialize from a byte slice, returning (entry, bytes_consumed).
    pub fn deserialize<R: Read + Seek>(data: &mut R) -> Result<(Self, usize), Error> {
        // [count, offset1, offset2, ...]
        let count = data.read_u32::<LittleEndian>()?;
        let mut offsets = Vec::with_capacity(count as usize);
        for _ in 0..count {
            offsets.push(data.read_u64::<LittleEndian>()?);
        }

        Ok((PayloadEntry { count, offsets }, count as usize * 8))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error;

    #[test]
    fn test_serialize_deserialize_empty() {
        let entry = PayloadEntry::new();
        let buf = entry.serialize();
        let buf_len = buf.len();
        assert_eq!(buf_len, 4);
        let (decoded, size) = PayloadEntry::deserialize(&mut Cursor::new(buf)).unwrap();
        assert_eq!(size, buf_len);
        assert_eq!(decoded.count, 0);
        assert!(decoded.offsets.is_empty());
    }

    #[test]
    fn test_serialize_deserialize_single() {
        let mut entry = PayloadEntry::new();
        entry.add_offset(0x1122_3344_5566_7788);
        let buf = entry.serialize();
        let buf_len = buf.len();
        assert_eq!(buf_len, 12);
        let (decoded, size) = PayloadEntry::deserialize(&mut Cursor::new(buf)).unwrap();
        assert_eq!(size, buf_len);
        assert_eq!(decoded.count, 1);
        assert_eq!(decoded.offsets, vec![0x1122_3344_5566_7788]);
    }

    #[test]
    fn test_serialize_deserialize_multiple() {
        let mut entry = PayloadEntry::new();
        let offs = vec![1u64, 42, 0xdead_beef_cafe, u64::MAX];
        for &o in &offs {
            entry.add_offset(o);
        }
        let buf = entry.serialize();
        let buf_len = buf.len();
        assert_eq!(buf_len, 4 + offs.len() * 8);
        let (decoded, size) = PayloadEntry::deserialize(&mut Cursor::new(buf)).unwrap();
        assert_eq!(size, buf_len);
        assert_eq!(decoded.count as usize, offs.len());
        assert_eq!(decoded.offsets, offs);
    }

    #[test]
    fn test_deserialize_too_short() {
        let data = [0u8, 1, 2];
        match PayloadEntry::deserialize(&mut Cursor::new(data)) {
            Err(Error::InvalidFormat(_)) => (),
            other => panic!("Expected InvalidFormat, got {:?}", other),
        }
    }
}
