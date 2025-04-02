use crate::error::Error;
use crate::key::Key;
use crate::Value; // Import the type alias from lib.rs
use std::cmp::Ordering;
use std::fmt::Debug;
use std::io::{Read, Write};
use std::mem;

/// Represents a Key-Value pair. Stored in leaf nodes and used as input for building.
// Remove the generic V, use the concrete Value type alias directly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry<K: Key> {
    /// The key part of the entry.
    pub key: K,
    /// The value part of the entry (u64 offset).
    pub value: Value, // Use the Value type alias directly
}

// Update the impl block to only use the K generic parameter
impl<K: Key> Entry<K> {
    /// The size of the value part in bytes (u64).
    const VALUE_SIZE: usize = mem::size_of::<Value>();
    /// The total size of the entry when serialized.
    pub const SERIALIZED_SIZE: usize = K::SERIALIZED_SIZE + Self::VALUE_SIZE;

    /// Serializes the entire entry (key followed by value) to a writer.
    /// Assumes little-endian encoding for the `Value`.
    pub fn write_to<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        self.key.write_to(writer)?;
        writer.write_all(&self.value.to_le_bytes())?;
        Ok(())
    }

    /// Deserializes an entire entry from a reader.
    /// Assumes little-endian encoding for the `Value`.
    pub fn read_from<R: Read>(reader: &mut R) -> Result<Self, Error> {
        let key = K::read_from(reader)?;
        let mut value_bytes = [0u8; Self::VALUE_SIZE];
        reader.read_exact(&mut value_bytes)?;
        let value = Value::from_le_bytes(value_bytes);
        Ok(Entry { key, value })
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
            // No V generic needed here
            key: TestKey(12345),
            value: 9876543210,
        };

        let mut buffer = Vec::new();
        entry.write_to(&mut buffer).expect("write should succeed");

        assert_eq!(
            buffer.len(),
            TestKey::SERIALIZED_SIZE + mem::size_of::<Value>()
        );
        assert_eq!(buffer.len(), Entry::<TestKey>::SERIALIZED_SIZE); // Update const access

        let mut cursor = Cursor::new(buffer);
        let deserialized_entry =
            Entry::<TestKey>::read_from(&mut cursor).expect("read should succeed"); // Update type

        assert_eq!(entry, deserialized_entry);
    }

    #[test]
    fn test_entry_ordering() {
        let entry1 = Entry {
            // No V generic
            key: TestKey(10),
            value: 100,
        };
        let entry2 = Entry {
            // No V generic
            key: TestKey(20),
            value: 50,
        };
        let entry3 = Entry {
            // No V generic
            key: TestKey(10),
            value: 200,
        };

        assert!(entry1 < entry2);
        assert!(entry2 > entry1);
        assert_eq!(entry1.cmp(&entry3), Ordering::Equal);
        assert_eq!(entry1.partial_cmp(&entry3), Some(Ordering::Equal));
    }

    #[test]
    fn test_entry_read_error_short_read() {
        let mut short_buffer = vec![0u8; Entry::<TestKey>::SERIALIZED_SIZE - 1]; // Update const access
        let mut cursor = Cursor::new(&mut short_buffer);
        let result = Entry::<TestKey>::read_from(&mut cursor); // Update type
        assert!(result.is_err());
        match result.err().unwrap() {
            Error::IoError(e) => assert_eq!(e.kind(), std::io::ErrorKind::UnexpectedEof),
            _ => panic!("expected io error"),
        }
    }
}
