use crate::error::Error;
use std::cmp::Ordering;
use std::fmt::Debug;
use std::io::{Read, Write};

/// Trait defining requirements for keys used in the StaticBTree.
///
/// Keys must support ordering (`Ord`), cloning (`Clone`), debugging (`Debug`),
/// and have a fixed serialized size (`SERIALIZED_SIZE`). Variable-length types
/// like `String` must be adapted (e.g., using fixed-size prefixes) to conform.
pub trait Key: Sized + Ord + Clone + Debug {
    /// The exact size of the key in bytes when serialized.
    /// This is crucial for calculating node sizes and offsets.
    const SERIALIZED_SIZE: usize;

    /// Serializes the key into the provided writer.
    ///
    /// # Arguments
    /// * `writer`: The `Write` target.
    ///
    /// # Returns
    /// `Ok(())` on success.
    /// `Err(Error)` if writing fails or the implementation cannot guarantee writing exactly `SERIALIZED_SIZE` bytes.
    fn write_to<W: Write>(&self, writer: &mut W) -> Result<(), Error>;

    /// Deserializes a key from the provided reader.
    ///
    /// # Arguments
    /// * `reader`: The `Read` source.
    ///
    /// # Returns
    /// `Ok(Self)` containing the deserialized key on success.
    /// `Err(Error)` if reading fails or the implementation cannot read exactly `SERIALIZED_SIZE` bytes.
    fn read_from<R: Read>(reader: &mut R) -> Result<Self, Error>;
}

// Implementations for standard types will go here in Step 2.
