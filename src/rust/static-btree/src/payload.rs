use crate::entry::Offset;
use crate::error::Error;
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::Cursor;

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
        Self { count: 0, offsets: Vec::new() }
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
    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), Error> {
        if data.len() < 4 {
            return Err(Error::InvalidFormat("Payload entry too short".to_string()));
        }
        let mut cur = Cursor::new(data);
        let count = cur.read_u32::<LittleEndian>()?;
        let total = 4usize
            .checked_add((count as usize)
                .checked_mul(8)
                .ok_or_else(|| Error::InvalidFormat("Invalid payload size".to_string()))?)
            .ok_or_else(|| Error::InvalidFormat("Invalid payload size".to_string()))?;
        if data.len() < total {
            return Err(Error::InvalidFormat("Payload entry data truncated".to_string()));
        }
        let mut offsets = Vec::with_capacity(count as usize);
        for _ in 0..count {
            offsets.push(cur.read_u64::<LittleEndian>()?);
        }
        Ok((PayloadEntry { count, offsets }, total))
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
        assert_eq!(buf.len(), 4);
        let (decoded, size) = PayloadEntry::deserialize(&buf).unwrap();
        assert_eq!(size, 4);
        assert_eq!(decoded.count, 0);
        assert!(decoded.offsets.is_empty());
    }

    #[test]
    fn test_serialize_deserialize_single() {
        let mut entry = PayloadEntry::new();
        entry.add_offset(0x1122_3344_5566_7788);
        let buf = entry.serialize();
        assert_eq!(buf.len(), 12);
        let (decoded, size) = PayloadEntry::deserialize(&buf).unwrap();
        assert_eq!(size, buf.len());
        assert_eq!(decoded.count, 1);
        assert_eq!(decoded.offsets, vec![0x1122_3344_5566_7788]);
    }

    #[test]
    fn test_serialize_deserialize_multiple() {
        let mut entry = PayloadEntry::new();
        let offs = vec![1u64, 42, 0xdead_beef_cafe, u64::MAX];
        for &o in &offs { entry.add_offset(o); }
        let buf = entry.serialize();
        assert_eq!(buf.len(), 4 + offs.len() * 8);
        let (decoded, size) = PayloadEntry::deserialize(&buf).unwrap();
        assert_eq!(size, buf.len());
        assert_eq!(decoded.count as usize, offs.len());
        assert_eq!(decoded.offsets, offs);
    }

    #[test]
    fn test_deserialize_too_short() {
        let data = [0u8, 1, 2];
        match PayloadEntry::deserialize(&data) {
            Err(Error::InvalidFormat(_)) => (),
            other => panic!("Expected InvalidFormat, got {:?}", other),
        }
    }
}