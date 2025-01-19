// Current version of FlatCityBuf
pub(crate) const VERSION: u8 = 1;

// Magic bytes for FlatCityBuf
pub(crate) const MAGIC_BYTES: [u8; 8] = [b'f', b'c', b'b', VERSION, b'f', b'c', b'b', 0];

// Maximum buffer size for header
pub(crate) const HEADER_MAX_BUFFER_SIZE: usize = 1024 * 1024 * 512; // 512MB

// Size of magic bytes
pub(crate) const MAGIC_BYTES_SIZE: usize = 8;

// Size of header size
pub(crate) const HEADER_SIZE_SIZE: usize = 4;

// // Offset of header size
// pub(crate) const HEADER_SIZE_OFFSET: usize = MAGIC_BYTES_SIZE + HEADER_SIZE_SIZE;
