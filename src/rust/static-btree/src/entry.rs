use crate::error::{Error, Result};
use crate::key::Key;
use std::cmp::Ordering;
use std::fmt::Debug;
use std::io::{Read, Write};
use std::mem;

/// The type associated with each key in the tree.
/// Currently fixed to u64, assuming byte offsets as values.
/// For leaf nodes except the last one, the offset is the byte offset of actual data. For the last entry of a leaf node, the offset is the byte offset of the next leaf node as it's B+Tree.
/// For internal nodes, the offset is the byte offset of the first key of the child node.
pub type Offset = u64;

/// Constant for the size of the Value type in bytes.
pub const OFFSET_SIZE: usize = mem::size_of::<Offset>();

/// Represents a Key-Value pair. Stored in leaf nodes and used as input for building.
// Remove the generic V, use the concrete Value type alias directly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry<K: Key> {
    /// The key part of the entry.
    pub key: K,
    /// The value part of the entry (u64 offset).
    pub offset: Offset, // Use the Value type alias directly
}

// Update the impl block to only use the K generic parameter
impl<K: Key> Entry<K> {
    /// The size of the value part in bytes (u64).
    const OFFSET_SIZE: usize = mem::size_of::<Offset>();
    /// The total size of the entry when serialized.
    pub const SERIALIZED_SIZE: usize = K::SERIALIZED_SIZE + Self::OFFSET_SIZE;

    pub fn new(key: K, offset: Offset) -> Self {
        Self { key, offset }
    }

    /// Serializes the entire entry (key followed by value) to a writer.
    /// Assumes little-endian encoding for the `Value`.
    pub fn write_to<W: Write>(&self, writer: &mut W) -> Result<()> {
        self.key.write_to(writer)?;
        writer.write_all(&self.offset.to_le_bytes())?;
        Ok(())
    }

    /// Deserializes an entire entry from a reader.
    /// Assumes little-endian encoding for the `Value`.
    pub fn from_reader<R: Read>(reader: &mut R) -> Result<Self> {
        let key = K::read_from(reader)?;
        let mut offset_bytes = [0u8; Self::OFFSET_SIZE];
        reader.read_exact(&mut offset_bytes)?;
        let offset = Offset::from_le_bytes(offset_bytes);
        Ok(Entry { key, offset })
    }

    pub fn from_bytes(raw: &[u8]) -> Result<Self> {
        let key = K::from_bytes(&raw[0..K::SERIALIZED_SIZE])?;
        let offset = Offset::from_bytes(&raw[K::SERIALIZED_SIZE..])?;
        Ok(Entry { key, offset })
    }

    pub fn key_size() -> usize {
        K::SERIALIZED_SIZE
    }
}

// Update ordering implementations
impl<K: Key> PartialOrd for Entry<K> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.key.partial_cmp(&other.key)
    }
}

impl<K: Key> Ord for Entry<K> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key.cmp(&other.key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::Key;
    use std::io::Cursor;

    #[test]
    fn test_entry_serialization_deserialization() {
        let entry = Entry {
            // No V generic needed here
            key: 12345,
            offset: 9876543210,
        };

        let mut buffer = Vec::new();
        entry.write_to(&mut buffer).expect("write should succeed");

        assert_eq!(
            buffer.len(),
            i32::SERIALIZED_SIZE + mem::size_of::<Offset>()
        );
        assert_eq!(buffer.len(), Entry::<i32>::SERIALIZED_SIZE); // Update const access

        let mut cursor = Cursor::new(buffer);
        let deserialized_entry =
            Entry::<i32>::from_reader(&mut cursor).expect("read should succeed"); // Update type

        assert_eq!(entry, deserialized_entry);
    }

    #[test]
    fn test_entry_ordering() {
        let entry1 = Entry {
            // No V generic
            key: 10,
            offset: 100,
        };
        let entry2 = Entry {
            // No V generic
            key: 20,
            offset: 50,
        };
        let entry3 = Entry {
            // No V generic
            key: 10,
            offset: 200,
        };

        assert!(entry1 < entry2);
        assert!(entry2 > entry1);
        assert_eq!(entry1.cmp(&entry3), Ordering::Equal);
        assert_eq!(entry1.partial_cmp(&entry3), Some(Ordering::Equal));
    }

    #[test]
    fn test_entry_read_error_short_read() {
        let mut short_buffer = vec![0u8; Entry::<i32>::SERIALIZED_SIZE - 1]; // Update const access
        let mut cursor = Cursor::new(&mut short_buffer);
        let result = Entry::<i32>::from_reader(&mut cursor); // Update type
        assert!(result.is_err());
        match result.err().unwrap() {
            Error::IoError(e) => assert_eq!(e.kind(), std::io::ErrorKind::UnexpectedEof),
            _ => panic!("expected io error"),
        }
    }
}
