// Current version of FlatCityBuf
pub const VERSION: u8 = 1;

// Magic bytes for FlatCityBuf
pub const MAGIC_BYTES: [u8; 8] = [b'f', b'c', b'b', VERSION, b'f', b'c', b'b', 0];

// Maximum buffer size for header
pub const HEADER_MAX_BUFFER_SIZE: usize = 1024 * 1024 * 512; // 512MB

// Size of magic bytes
pub const MAGIC_BYTES_SIZE: usize = 8;

// Size of header size
pub const HEADER_SIZE_SIZE: usize = 4;

/// The name of the header column that stands for a City Object's id. A file
/// written with an id index declares it with `primary_key = true`; no
/// attribute blob carries a value for it, because the value is
/// `CityObject.id`.
pub const ID_COLUMN_NAME: &str = "id";
