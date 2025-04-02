use crate::error::Error;
use crate::key::Key;
use crate::Value; // Import the type alias from lib.rs
use std::cmp::Ordering;
use std::fmt::Debug;
use std::io::{Read, Write};
use std::mem;

/// Represents a Key-Value pair. Stored in leaf nodes and used as input for building.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry<K: Key, V: Value> {
    /// The key part of the entry.
    pub key: K,
    /// The value part of the entry (typically a u64 offset).
    pub value: V,
}

impl<K: Key, V: Value> Entry<K, V> {
    /// The size of the value part in bytes (assuming `Value = u64`).
    const VALUE_SIZE: usize = mem::size_of::<Value>(); // Use the constant from lib.rs? No, keep it local for clarity.
    /// The total size of the entry when serialized.
    pub const SERIALIZED_SIZE: usize = K::SERIALIZED_SIZE + Self::VALUE_SIZE;

    /// Serializes the entire entry (key followed by value) to a writer.
    /// Assumes little-endian encoding for the `Value`.
    pub fn write_to<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        // Write the key first using its trait implementation.
        self.key.write_to(writer)?;
        // Write the value as little-endian bytes.
        // Note: The spec uses V: Value, but Value is u64. We assume V is always u64 here.
        // If V could be something else implementing a hypothetical 'Value' trait, this would need adjustment.
        writer.write_all(&self.value.to_le_bytes())?;
        Ok(())
    }

    /// Deserializes an entire entry from a reader.
    /// Assumes little-endian encoding for the `Value`.
    pub fn read_from<R: Read>(reader: &mut R) -> Result<Self, Error> {
        // Read the key using its trait implementation.
        let key = K::read_from(reader)?;
        // Read the exact number of bytes for the value.
        let mut value_bytes = [0u8; Self::VALUE_SIZE];
        reader.read_exact(&mut value_bytes)?;
        // Convert bytes to the Value type (u64).
        let value = Value::from_le_bytes(value_bytes);
        Ok(Entry { key, value })
    }
}

// Implement ordering based *only* on the key. This is essential for sorting
// input entries before building and for searching within leaf nodes.
impl<K: Key, V: Value> PartialOrd for Entry<K, V> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.key.partial_cmp(&other.key)
    }
}

impl<K: Key, V: Value> Ord for Entry<K, V> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key.cmp(&other.key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::Key; // Need a concrete Key implementation for testing
    use std::io::Cursor;

    // Define a simple Key implementation for testing purposes
    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
    struct TestKey(i32);

    impl Key for TestKey {
        const SERIALIZED_SIZE: usize = 4;

        fn write_to<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
            writer.write_all(&self.0.to_le_bytes()).map_err(Error::from)
        }

        fn read_from<R: Read>(reader: &mut R) -> Result<Self, Error> {
            let mut bytes = [0u8; Self::SERIALIZED_SIZE];
            reader.read_exact(&mut bytes)?;
            Ok(TestKey(i32::from_le_bytes(bytes)))
        }
    }

    #[test]
    fn test_entry_serialization_deserialization() {
        let entry = Entry {
            key: TestKey(12345),
            value: 9876543210,
        };

        let mut buffer = Vec::new();
        entry.write_to(&mut buffer).expect("write should succeed");

        assert_eq!(
            buffer.len(),
            TestKey::SERIALIZED_SIZE + mem::size_of::<Value>()
        );
        assert_eq!(buffer.len(), Entry::<TestKey, Value>::SERIALIZED_SIZE);

        let mut cursor = Cursor::new(buffer);
        let deserialized_entry =
            Entry::<TestKey, Value>::read_from(&mut cursor).expect("read should succeed");

        assert_eq!(entry, deserialized_entry);
    }

    #[test]
    fn test_entry_ordering() {
        let entry1 = Entry {
            key: TestKey(10),
            value: 100,
        };
        let entry2 = Entry {
            key: TestKey(20),
            value: 50, // Value should not affect comparison
        };
        let entry3 = Entry {
            key: TestKey(10),
            value: 200, // Value should not affect comparison
        };

        assert!(entry1 < entry2);
        assert!(entry2 > entry1);
        assert_eq!(entry1.cmp(&entry3), Ordering::Equal);
        assert_eq!(entry1.partial_cmp(&entry3), Some(Ordering::Equal));
    }

    #[test]
    fn test_entry_read_error_short_read() {
        let mut short_buffer = vec![0u8; Entry::<TestKey, Value>::SERIALIZED_SIZE - 1]; // One byte too short
        let mut cursor = Cursor::new(&mut short_buffer);
        let result = Entry::<TestKey, Value>::read_from(&mut cursor);
        assert!(result.is_err());
        match result.err().unwrap() {
            Error::IoError(e) => assert_eq!(e.kind(), std::io::ErrorKind::UnexpectedEof),
            _ => panic!("expected io error"),
        }
    }
}
