//! Abstract storage layer for static B+Tree nodes.

use crate::errors::{Error, Result, StorageError}; // Import Error, Result, and StorageError
use async_trait::async_trait;
use bytes::Bytes; // Using Bytes for efficient slicing
use std::sync::RwLock; // Using RwLock for interior mutability needed by async trait

/// Trait for abstracting B+tree node storage.
///
/// This trait defines the necessary operations for reading and writing
/// raw node data from various storage backends (memory, file, HTTP).
#[async_trait(?Send)] // ?Send might be needed depending on WASM client needs
pub trait BTreeStorage {
    /// Reads the raw byte data for a node at the given index.
    ///
    /// The index corresponds to the implicit node numbering scheme used
    /// by the static B+Tree.
    ///
    /// # Arguments
    ///
    /// * `node_index` - The index of the node to read.
    ///
    /// # Returns
    ///
    /// A `Result` containing the node data as `Bytes` on success,
    /// or an error if the node cannot be read.
    async fn read_node(&self, node_index: usize) -> Result<Bytes>;

    /// Writes the raw byte data for a node at the given index.
    ///
    /// This method is primarily used during the construction phase of the
    /// static B+Tree. Implementations for read-only storage might
    /// return an error or be a no-op.
    ///
    /// # Arguments
    ///
    /// * `node_index` - The index of the node to write.
    /// * `data` - The raw byte data of the node.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    async fn write_node(&mut self, node_index: usize, data: &[u8]) -> Result<()>;

    /// Returns the fixed size of each node in bytes for this tree instance.
    ///
    /// This size is determined during tree construction based on the
    /// branching factor and the encoded size of the keys/values.
    fn node_size(&self) -> usize;

    /// Returns the total number of nodes managed by this storage backend.
    fn node_count(&self) -> usize;

    /// Flushes any buffered writes to the underlying persistent storage.
    ///
    /// For in-memory storage, this might be a no-op.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure of the flush operation.
    async fn flush(&mut self) -> Result<()>;

    // Optional: Add methods for metadata if needed (e.g., tree height, branch factor)
    // async fn read_metadata(&self) -> Result<TreeMetadata>;
    // async fn write_metadata(&mut self, metadata: &TreeMetadata) -> Result<()>;
}

/// In-memory storage backend for static B+Tree nodes.
///
/// Stores all node data in a single contiguous `Vec<u8>`.
#[derive(Debug)]
pub struct MemoryStorage {
    /// Raw byte data for all nodes, stored contiguously.
    data: RwLock<Vec<u8>>,
    /// The fixed size of each node in bytes.
    node_size: usize,
    /// The total number of nodes.
    node_count: usize,
}

impl MemoryStorage {
    /// Creates a new `MemoryStorage` instance.
    ///
    /// # Arguments
    ///
    /// * `node_size` - The size of each node in bytes.
    /// * `node_count` - The total number of nodes to allocate space for.
    ///
    /// # Returns
    ///
    /// A `Result` containing the new `MemoryStorage` or an error if allocation fails.
    pub fn new(node_size: usize, node_count: usize) -> Result<Self> {
        let total_size = node_count.checked_mul(node_size).ok_or_else(|| {
            Error::Other("Node count or size too large, resulting in overflow".to_string())
        })?;

        // Initialize with zeros for safety.
        let data = vec![0u8; total_size];

        Ok(Self {
            data: RwLock::new(data),
            node_size,
            node_count,
        })
    }
}

#[async_trait(?Send)]
impl BTreeStorage for MemoryStorage {
    async fn read_node(&self, node_index: usize) -> Result<Bytes> {
        if node_index >= self.node_count {
            return Err(Error::Other(format!(
                // Use Error::Other for argument validation
                "Node index {} out of bounds ({})",
                node_index, self.node_count
            )));
        }

        let start = node_index * self.node_size;
        let end = start + self.node_size;

        // Use read lock
        let data_guard = self.data.read().map_err(|e| {
            Error::Storage(StorageError::Read(format!(
                "Failed to acquire read lock: {}",
                e
            )))
        })?;

        // Ensure the slice bounds are correct (belt-and-suspenders)
        if end > data_guard.len() {
            return Err(Error::Storage(StorageError::Read(format!(
                // Use Error::Storage for internal consistency issues
                "Calculated read end ({}) exceeds data length ({}) for node index {}",
                end,
                data_guard.len(),
                node_index
            ))));
        }

        Ok(Bytes::copy_from_slice(&data_guard[start..end]))
    }

    async fn write_node(&mut self, node_index: usize, data: &[u8]) -> Result<()> {
        if node_index >= self.node_count {
            return Err(Error::Other(format!(
                // Use Error::Other
                "Node index {} out of bounds ({})",
                node_index, self.node_count
            )));
        }
        if data.len() != self.node_size {
            return Err(Error::Other(format!(
                // Use Error::Other
                "Incorrect data size for write: expected {}, got {}",
                self.node_size,
                data.len()
            )));
        }

        let start = node_index * self.node_size;
        let end = start + self.node_size;

        // Use write lock
        let mut data_guard = self.data.write().map_err(|e| {
            Error::Storage(StorageError::Write(format!(
                "Failed to acquire write lock: {}",
                e
            )))
        })?;

        // Ensure the slice bounds are correct
        if end > data_guard.len() {
            return Err(Error::Storage(StorageError::Write(format!(
                // Use Error::Storage
                "Calculated write end ({}) exceeds data length ({}) for node index {}",
                end,
                data_guard.len(),
                node_index
            ))));
        }

        data_guard[start..end].copy_from_slice(data);
        Ok(())
    }

    fn node_size(&self) -> usize {
        self.node_size
    }

    fn node_count(&self) -> usize {
        self.node_count
    }

    async fn flush(&mut self) -> Result<()> {
        // No-op for memory storage
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_storage_new() {
        let storage = MemoryStorage::new(64, 100).unwrap();
        assert_eq!(storage.node_size(), 64);
        assert_eq!(storage.node_count(), 100);
        assert_eq!(storage.data.read().unwrap().len(), 64 * 100);
    }

    #[tokio::test]
    async fn test_memory_storage_write_read() {
        let mut storage = MemoryStorage::new(8, 10).unwrap();
        let data1 = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let data2 = vec![9, 10, 11, 12, 13, 14, 15, 16];

        storage.write_node(0, &data1).await.unwrap();
        storage.write_node(5, &data2).await.unwrap();

        let read_data1 = storage.read_node(0).await.unwrap();
        assert_eq!(read_data1.as_ref(), data1.as_slice());

        let read_data2 = storage.read_node(5).await.unwrap();
        assert_eq!(read_data2.as_ref(), data2.as_slice());

        // Check if other nodes are still zero (or initial value)
        let read_data_other = storage.read_node(1).await.unwrap();
        assert_eq!(read_data_other.as_ref(), &[0u8; 8]);
    }

    #[tokio::test]
    async fn test_memory_storage_out_of_bounds_read() {
        let storage = MemoryStorage::new(8, 10).unwrap();
        let result = storage.read_node(10).await;
        assert!(result.is_err());
        match result.err().unwrap() {
            Error::Other(_) => {} // Expected
            e => panic!("Unexpected error type: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_memory_storage_out_of_bounds_write() {
        let mut storage = MemoryStorage::new(8, 10).unwrap();
        let data = vec![1; 8];
        let result = storage.write_node(10, &data).await;
        assert!(result.is_err());
        match result.err().unwrap() {
            Error::Other(_) => {} // Expected
            e => panic!("Unexpected error type: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_memory_storage_incorrect_write_size() {
        let mut storage = MemoryStorage::new(8, 10).unwrap();
        let data_small = vec![1; 7];
        let data_large = vec![1; 9];

        let result_small = storage.write_node(0, &data_small).await;
        assert!(result_small.is_err());
        match result_small.err().unwrap() {
            Error::Other(_) => {} // Expected
            e => panic!("Unexpected error type: {:?}", e),
        }

        let result_large = storage.write_node(0, &data_large).await;
        assert!(result_large.is_err());
        match result_large.err().unwrap() {
            Error::Other(_) => {} // Expected
            e => panic!("Unexpected error type: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_memory_storage_flush() {
        let mut storage = MemoryStorage::new(8, 10).unwrap();
        // Flush should always succeed and do nothing
        assert!(storage.flush().await.is_ok());
    }

    #[test]
    fn test_memory_storage_overflow() {
        let result = MemoryStorage::new(usize::MAX / 2, 3);
        assert!(result.is_err());
        match result.err().unwrap() {
            Error::Other(_) => {} // Expected
            e => panic!("Unexpected error type: {:?}", e),
        }
        let result = MemoryStorage::new(10, usize::MAX / 5);
        assert!(result.is_err());
        match result.err().unwrap() {
            Error::Other(_) => {} // Expected
            e => panic!("Unexpected error type: {:?}", e),
        }
    }
}
