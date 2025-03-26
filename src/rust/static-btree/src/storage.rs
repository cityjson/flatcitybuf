// Storage interface for static B+tree
//
// This module provides a flexible storage abstraction for static B+trees, enabling
// different backends (memory, file, HTTP) while maintaining the same interface.
// The design uses standard Read/Write/Seek traits for maximum compatibility.

use crate::errors::{HttpError, Result, StorageError};
use std::io::{Cursor, Read, Seek, SeekFrom, Write};

/// Trait for accessing the storage of a static B+Tree.
///
/// This trait is implemented by any type that can provide random access
/// read and write operations to a byte stream, using the standard
/// Read, Write, and Seek traits.
pub trait BTreeStorage {
    /// Read a node from storage at the specified index.
    ///
    /// # Arguments
    /// * `node_index` - The index of the node to read
    /// * `buf` - The buffer to read the node data into
    ///
    /// # Returns
    /// The number of bytes read or an error
    fn read_node(&mut self, node_index: usize, buf: &mut [u8]) -> Result<usize>;

    /// Write a node to storage at the specified index.
    ///
    /// # Arguments
    /// * `node_index` - The index of the node to write
    /// * `data` - The data to write to the node
    ///
    /// # Returns
    /// The number of bytes written or an error
    fn write_node(&mut self, node_index: usize, data: &[u8]) -> Result<usize>;

    /// Get the fixed size of nodes in this storage
    fn node_size(&self) -> usize;

    /// Get the total number of nodes in the storage
    fn node_count(&self) -> usize;

    /// Flush any pending writes to the underlying storage
    fn flush(&mut self) -> Result<()>;
}

/// Generic implementation of BTreeStorage for any type that implements Read, Write, and Seek.
///
/// This allows the B+Tree to work with a variety of storage backends, including
/// files, memory buffers, network streams, etc.
pub struct GenericStorage<T> {
    /// The underlying I/O object
    inner: T,
    /// The fixed size of each node in bytes
    node_size: usize,
    /// The total number of nodes in the storage
    node_count: usize,
    /// Offset in bytes where the B+Tree data begins in the underlying storage
    base_offset: u64,
}

impl<T> GenericStorage<T>
where
    T: Read + Write + Seek,
{
    /// Create a new GenericStorage with the specified parameters
    pub fn new(inner: T, node_size: usize, node_count: usize, base_offset: u64) -> Self {
        Self {
            inner,
            node_size,
            node_count,
            base_offset,
        }
    }
}

impl<T> BTreeStorage for GenericStorage<T>
where
    T: Read + Write + Seek,
{
    fn read_node(&mut self, node_index: usize, buf: &mut [u8]) -> Result<usize> {
        if node_index >= self.node_count {
            return Err(StorageError::InvalidOffset(node_index as u64).into());
        }

        if buf.len() < self.node_size {
            return Err(StorageError::Read(format!(
                "buffer too small: {} < {}",
                buf.len(),
                self.node_size
            ))
            .into());
        }

        let offset = self.base_offset + (node_index * self.node_size) as u64;
        self.inner.seek(SeekFrom::Start(offset))?;

        // Read exactly node_size bytes
        let bytes_read = self.inner.read(&mut buf[0..self.node_size])?;

        if bytes_read < self.node_size {
            return Err(StorageError::Read(format!(
                "incomplete read: read {} of {} bytes",
                bytes_read, self.node_size
            ))
            .into());
        }

        Ok(bytes_read)
    }

    fn write_node(&mut self, node_index: usize, data: &[u8]) -> Result<usize> {
        if node_index >= self.node_count {
            return Err(StorageError::InvalidOffset(node_index as u64).into());
        }

        if data.len() > self.node_size {
            return Err(StorageError::Write(format!(
                "data too large: {} > {}",
                data.len(),
                self.node_size
            ))
            .into());
        }

        let offset = self.base_offset + (node_index * self.node_size) as u64;
        self.inner.seek(SeekFrom::Start(offset))?;

        // Write the data
        let bytes_written = self.inner.write(data)?;

        // If we wrote less than node_size, pad with zeros
        if bytes_written < self.node_size {
            let padding = vec![0u8; self.node_size - bytes_written];
            self.inner.write_all(&padding)?;
        }

        Ok(bytes_written)
    }

    fn node_size(&self) -> usize {
        self.node_size
    }

    fn node_count(&self) -> usize {
        self.node_count
    }

    fn flush(&mut self) -> Result<()> {
        self.inner.flush()?;
        Ok(())
    }
}

/// In-memory storage for a static B+Tree using a `Cursor<Vec<u8>>`.
pub struct MemoryStorage {
    /// Storage implementation using a cursor over a byte vector
    storage: GenericStorage<Cursor<Vec<u8>>>,
}

impl MemoryStorage {
    /// Create a new in-memory storage for a B+Tree
    pub fn new(node_size: usize, node_count: usize) -> Result<Self> {
        if node_size == 0 {
            return Err(StorageError::InvalidBlockSize(node_size).into());
        }

        // Allocate a buffer large enough for all nodes plus a header
        let header_size = 16; // Simple header with metadata
        let total_size = header_size + (node_size * node_count);
        let buffer = vec![0u8; total_size];

        // Create the cursor and storage
        let cursor = Cursor::new(buffer);
        let storage = GenericStorage::new(cursor, node_size, node_count, header_size as u64);

        let mut result = Self { storage };

        // Write metadata in the header (node_size, node_count)
        // This is a simplified version for illustration
        let mut header = [0u8; 16];
        (&mut header[0..4]).copy_from_slice(&(node_size as u32).to_le_bytes());
        (&mut header[4..8]).copy_from_slice(&(node_count as u32).to_le_bytes());

        // Get a mutable reference to the inner cursor to write the header
        let inner = &mut result.storage.inner;
        inner.seek(SeekFrom::Start(0))?;
        inner.write_all(&header)?;

        Ok(result)
    }

    /// Get the raw bytes of the storage
    pub fn into_inner(self) -> Vec<u8> {
        self.storage.inner.into_inner()
    }
}

impl BTreeStorage for MemoryStorage {
    fn read_node(&mut self, node_index: usize, buf: &mut [u8]) -> Result<usize> {
        self.storage.read_node(node_index, buf)
    }

    fn write_node(&mut self, node_index: usize, data: &[u8]) -> Result<usize> {
        self.storage.write_node(node_index, data)
    }

    fn node_size(&self) -> usize {
        self.storage.node_size()
    }

    fn node_count(&self) -> usize {
        self.storage.node_count()
    }

    fn flush(&mut self) -> Result<()> {
        self.storage.flush()
    }
}

/// File-based storage for a static B+Tree
pub struct FileStorage {
    /// Storage implementation using a file
    storage: GenericStorage<std::fs::File>,
}

impl FileStorage {
    /// Create a new file storage for a B+Tree
    pub fn create<P: AsRef<std::path::Path>>(
        path: P,
        node_size: usize,
        node_count: usize,
    ) -> Result<Self> {
        if node_size == 0 {
            return Err(StorageError::InvalidBlockSize(node_size).into());
        }

        let header_size = 16; // Simple header with metadata
        let total_size = header_size + (node_size * node_count);

        // Create and truncate the file
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;

        // Set the file size
        file.set_len(total_size as u64)?;

        // Create the storage
        let mut storage = GenericStorage::new(file, node_size, node_count, header_size as u64);

        // Create the result
        let mut result = Self { storage };

        // Write metadata in the header
        let mut header = [0u8; 16];
        (&mut header[0..4]).copy_from_slice(&(node_size as u32).to_le_bytes());
        (&mut header[4..8]).copy_from_slice(&(node_count as u32).to_le_bytes());

        // Write the header
        result.storage.inner.seek(SeekFrom::Start(0))?;
        result.storage.inner.write_all(&header)?;
        result.storage.inner.flush()?;

        Ok(result)
    }

    /// Open an existing file storage
    pub fn open<P: AsRef<std::path::Path>>(path: P) -> Result<Self> {
        // Open the existing file
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)?;

        // Read the header to get node_size and node_count
        let mut header = [0u8; 16];
        file.seek(SeekFrom::Start(0))?;
        file.read_exact(&mut header)?;

        let node_size = u32::from_le_bytes([header[0], header[1], header[2], header[3]]) as usize;
        let node_count = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;

        if node_size == 0 {
            return Err(StorageError::InvalidBlockSize(node_size).into());
        }

        // Create the storage
        let storage = GenericStorage::new(file, node_size, node_count, 16);

        Ok(Self { storage })
    }
}

impl BTreeStorage for FileStorage {
    fn read_node(&mut self, node_index: usize, buf: &mut [u8]) -> Result<usize> {
        self.storage.read_node(node_index, buf)
    }

    fn write_node(&mut self, node_index: usize, data: &[u8]) -> Result<usize> {
        self.storage.write_node(node_index, data)
    }

    fn node_size(&self) -> usize {
        self.storage.node_size()
    }

    fn node_count(&self) -> usize {
        self.storage.node_count()
    }

    fn flush(&mut self) -> Result<()> {
        self.storage.flush()
    }
}

/// Asynchronous storage interface for HTTP and other non-blocking I/O
#[cfg(feature = "http")]
pub mod async_storage {
    use super::*;
    use async_trait::async_trait;
    use std::{num::NonZeroUsize, sync::Arc};

    /// Asynchronous B-tree storage trait
    ///
    /// This trait defines the interface for asynchronous access to
    /// static B+tree storage, primarily for HTTP-based access.
    #[async_trait]
    pub trait AsyncBTreeStorage {
        /// Read a node from storage at the specified index
        async fn read_node(&self, node_index: usize, buf: &mut [u8]) -> Result<usize>;

        /// Get the fixed size of nodes in this storage
        fn node_size(&self) -> usize;

        /// Get the total number of nodes in the storage
        fn node_count(&self) -> usize;
    }

    /// Structure representing an HTTP range request
    #[derive(Debug, Clone)]
    pub enum HttpRange {
        /// A specific byte range (start to end)
        Range {
            /// Start byte (inclusive)
            start: usize,
            /// End byte (exclusive)
            end: usize,
        },

        /// An open-ended range starting at a specific byte
        RangeFrom {
            /// Start byte (inclusive)
            start: usize,
        },
    }

    impl HttpRange {
        /// Get the start byte of the range
        pub fn start(&self) -> usize {
            match self {
                HttpRange::Range { start, .. } => *start,
                HttpRange::RangeFrom { start } => *start,
            }
        }

        /// Get the end byte of the range, if defined
        pub fn end(&self) -> Option<usize> {
            match self {
                HttpRange::Range { end, .. } => Some(*end),
                HttpRange::RangeFrom { .. } => None,
            }
        }

        /// Get the length of the range, if defined
        pub fn length(&self) -> Option<usize> {
            match self {
                HttpRange::Range { start, end } => Some(end - start),
                HttpRange::RangeFrom { .. } => None,
            }
        }

        /// Convert to an HTTP range header value
        pub fn to_header_value(&self) -> String {
            match self {
                HttpRange::Range { start, end } => format!("bytes={}-{}", start, end - 1),
                HttpRange::RangeFrom { start } => format!("bytes={}-", start),
            }
        }
    }

    /// HTTP-based storage for a static B+Tree
    ///
    /// This implementation uses HTTP range requests to fetch nodes from
    /// a remote source, with optional caching to improve performance.
    pub struct HttpStorage {
        /// Base URL for the B+Tree data
        base_url: String,
        /// HTTP client for making requests
        client: reqwest::Client,
        /// Size of each node in bytes
        node_size: usize,
        /// Total number of nodes available
        node_count: usize,
        /// Header size in bytes
        header_size: usize,
        /// Optional cache for frequently accessed nodes
        cache: Option<Arc<tokio::sync::RwLock<lru::LruCache<usize, Vec<u8>>>>>,
    }

    impl HttpStorage {
        /// Create a new HTTP storage for a B+Tree
        ///
        /// This method fetches the header from the remote URL to determine
        /// node size and count information.
        pub async fn new(base_url: String, cache_size: Option<usize>) -> Result<Self> {
            let client = reqwest::Client::new();

            // Fetch the header to get node_size and node_count
            let response = client
                .get(&base_url)
                .header(reqwest::header::RANGE, "bytes=0-15")
                .send()
                .await
                .map_err(|e| HttpError::Network(e.to_string()))?;

            if !response.status().is_success() {
                return Err(HttpError::Status(format!(
                    "HTTP request failed: {}",
                    response.status()
                ))
                .into());
            }

            let header = response
                .bytes()
                .await
                .map_err(|e| HttpError::Response(e.to_string()))?;

            if header.len() < 16 {
                return Err(HttpError::Response(format!(
                    "header too small: {} < 16",
                    header.len()
                ))
                .into());
            }

            let node_size =
                u32::from_le_bytes([header[0], header[1], header[2], header[3]]) as usize;
            let node_count =
                u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;

            if node_size == 0 {
                return Err(StorageError::InvalidBlockSize(node_size).into());
            }

            let cache = cache_size.map(|size| {
                Arc::new(tokio::sync::RwLock::new(lru::LruCache::new(
                    NonZeroUsize::new(size).unwrap(),
                )))
            });

            Ok(Self {
                base_url,
                client,
                node_size,
                node_count,
                header_size: 16,
                cache,
            })
        }
    }

    #[async_trait]
    impl AsyncBTreeStorage for HttpStorage {
        async fn read_node(&self, node_index: usize, buf: &mut [u8]) -> Result<usize> {
            if node_index >= self.node_count {
                return Err(StorageError::InvalidOffset(node_index as u64).into());
            }

            if buf.len() < self.node_size {
                return Err(StorageError::Read(format!(
                    "buffer too small: {} < {}",
                    buf.len(),
                    self.node_size
                ))
                .into());
            }

            // Check cache first if enabled
            if let Some(cache) = &self.cache {
                let mut cache_write = cache.write().await;
                if let Some(data) = cache_write.get(&node_index) {
                    buf[..data.len()].copy_from_slice(data);
                    return Ok(data.len());
                }
                drop(cache_write);
            }

            // Calculate byte range for the node
            let start = self.header_size + (node_index * self.node_size);
            let end = start + self.node_size;

            let range = HttpRange::Range { start, end };

            // Make HTTP range request
            let response = self
                .client
                .get(&self.base_url)
                .header(reqwest::header::RANGE, range.to_header_value())
                .send()
                .await
                .map_err(|e| HttpError::Network(e.to_string()))?;

            if !response.status().is_success() {
                return Err(HttpError::Status(format!(
                    "HTTP request failed: {}",
                    response.status()
                ))
                .into());
            }

            let data = response
                .bytes()
                .await
                .map_err(|e| HttpError::Response(e.to_string()))?;

            let bytes_read = data.len();

            if bytes_read < self.node_size {
                return Err(StorageError::Read(format!(
                    "incomplete read: read {} of {} bytes",
                    bytes_read, self.node_size
                ))
                .into());
            }

            // Copy data to buffer
            buf[..bytes_read].copy_from_slice(&data);

            // Update cache if enabled
            if let Some(cache) = &self.cache {
                let mut cache_write = cache.write().await;
                cache_write.put(node_index, data.to_vec());
            }

            Ok(bytes_read)
        }

        fn node_size(&self) -> usize {
            self.node_size
        }

        fn node_count(&self) -> usize {
            self.node_count
        }
    }
}

// Re-export HTTP storage when the feature is enabled
#[cfg(feature = "http")]
pub use async_storage::{AsyncBTreeStorage, HttpRange, HttpStorage};

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_memory_storage() -> Result<()> {
        // Create a new memory storage
        let mut storage = MemoryStorage::new(256, 10)?;

        // Write test data to node 0
        let test_data = vec![1, 2, 3, 4, 5];
        storage.write_node(0, &test_data)?;

        // Read it back
        let mut buf = vec![0; 256];
        storage.read_node(0, &mut buf)?;

        // Verify the data
        assert_eq!(&buf[0..5], &test_data);
        assert_eq!(buf[5], 0); // Rest should be zeros

        // Test out of bounds access
        let result = storage.read_node(10, &mut buf);
        assert!(result.is_err());

        Ok(())
    }

    #[cfg(feature = "testing")]
    #[test]
    fn test_file_storage() -> Result<()> {
        // Create a temporary directory
        let dir = tempfile::tempdir()?;
        let file_path = dir.path().join("test.btree");

        // Create a new file storage
        let mut storage = FileStorage::create(&file_path, 256, 10)?;

        // Write test data to node 0
        let test_data = vec![1, 2, 3, 4, 5];
        storage.write_node(0, &test_data)?;

        // Write to another node
        let test_data2 = vec![6, 7, 8, 9, 10];
        storage.write_node(5, &test_data2)?;

        // Flush to ensure data is written
        storage.flush()?;

        // Close and reopen
        drop(storage);
        let mut storage = FileStorage::open(&file_path)?;

        // Read data back
        let mut buf = vec![0; 256];
        storage.read_node(0, &mut buf)?;

        // Verify the data
        assert_eq!(&buf[0..5], &test_data);

        // Read second node
        let mut buf2 = vec![0; 256];
        storage.read_node(5, &mut buf2)?;

        // Verify the data
        assert_eq!(&buf2[0..5], &test_data2);

        Ok(())
    }
}
