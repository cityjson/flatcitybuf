//! Example demonstrating how to embed a B-tree within a custom file format
//!
//! This example creates a simple file format with:
//! - A header section
//! - An embedded B-tree index
//! - A data section
//!
//! The file format could represent something like a spatial data file where
//! the B-tree provides an index for fast lookups into the data section.

use anyhow::Result;
use btree::{AnyKeyEncoder, BTree, GenericBlockStorage, KeyType};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use tempfile::tempfile;

// File format constants
const HEADER_SIZE: u64 = 512; // 512 bytes for header
const BTREE_SIZE: u64 = 8192; // 8KB for B-tree
const BLOCK_SIZE: usize = 512; // 512-byte blocks

/// Header type for our custom file format
#[derive(Debug)]
struct FileHeader {
    magic: [u8; 4],
    version: u32,
    record_count: u32,
    btree_root_offset: u64,
}

impl FileHeader {
    fn new() -> Self {
        Self {
            magic: *b"FLAT",      // Magic identifier
            version: 1,           // Version 1
            record_count: 0,      // No records yet
            btree_root_offset: 0, // B-tree root not set yet
        }
    }

    fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(HEADER_SIZE as usize);

        // Write magic identifier
        buf.extend_from_slice(&self.magic);

        // Write version as little-endian u32
        buf.extend_from_slice(&self.version.to_le_bytes());

        // Write record count
        buf.extend_from_slice(&self.record_count.to_le_bytes());

        // Write B-tree root offset
        buf.extend_from_slice(&self.btree_root_offset.to_le_bytes());

        // Pad to header size
        buf.resize(HEADER_SIZE as usize, 0);
        buf
    }

    fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 20 {
            anyhow::bail!("header too small");
        }

        let mut magic = [0u8; 4];
        magic.copy_from_slice(&bytes[0..4]);

        if &magic != b"FLAT" {
            anyhow::bail!("invalid file format");
        }

        let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        let record_count = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        let btree_root_offset = u64::from_le_bytes([
            bytes[12], bytes[13], bytes[14], bytes[15], bytes[16], bytes[17], bytes[18], bytes[19],
        ]);

        Ok(Self {
            magic,
            version,
            record_count,
            btree_root_offset,
        })
    }
}

/// A simple record type to store in our data section
#[derive(Debug, Clone)]
struct Record {
    id: i64,
    name: String,
    value: f64,
}

impl Record {
    fn new(id: i64, name: &str, value: f64) -> Self {
        Self {
            id,
            name: name.to_string(),
            value,
        }
    }

    fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        // Write id
        buf.extend_from_slice(&self.id.to_le_bytes());

        // Write name length and data
        let name_bytes = self.name.as_bytes();
        let name_len = name_bytes.len() as u16;
        buf.extend_from_slice(&name_len.to_le_bytes());
        buf.extend_from_slice(name_bytes);

        // Write value
        buf.extend_from_slice(&self.value.to_le_bytes());

        buf
    }

    fn decode(bytes: &[u8]) -> Result<(Self, usize)> {
        if bytes.len() < 10 {
            anyhow::bail!("record data too small");
        }

        let id = i64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]);

        let name_len = u16::from_le_bytes([bytes[8], bytes[9]]) as usize;

        if bytes.len() < 10 + name_len + 8 {
            anyhow::bail!("record data too small for name");
        }

        let name = String::from_utf8(bytes[10..10 + name_len].to_vec())?;

        let value_start = 10 + name_len;
        let value = f64::from_le_bytes([
            bytes[value_start],
            bytes[value_start + 1],
            bytes[value_start + 2],
            bytes[value_start + 3],
            bytes[value_start + 4],
            bytes[value_start + 5],
            bytes[value_start + 6],
            bytes[value_start + 7],
        ]);

        let record = Self { id, name, value };
        let bytes_read = value_start + 8;

        Ok((record, bytes_read))
    }
}

/// Write our custom file format with embedded B-tree
fn create_file_with_btree() -> Result<File> {
    println!("creating file with embedded b-tree...");

    // Create a temporary file
    let file = tempfile()?;
    let mut file_clone = file.try_clone()?;

    // Create and write header (initially with zero values)
    let header = FileHeader::new();
    file_clone.write_all(&header.encode())?;

    // Create B-tree storage starting after header
    let storage = GenericBlockStorage::with_bounds(
        file_clone,
        HEADER_SIZE,                    // Start after header
        Some(HEADER_SIZE + BTREE_SIZE), // Limit to allocated section
        BLOCK_SIZE,                     // Block size
        10,                             // Cache size
    );

    // Create B-tree with integer keys
    let key_encoder = Box::new(AnyKeyEncoder::i64());
    let mut btree = BTree::new(storage, key_encoder)?;

    // Get the root offset to save in header
    let root_offset = btree.root_offset();

    // Sample records to store
    let records = vec![
        Record::new(1, "Alpha", 10.5),
        Record::new(2, "Beta", 20.7),
        Record::new(3, "Gamma", 30.2),
        Record::new(4, "Delta", 40.9),
        Record::new(5, "Epsilon", 50.1),
    ];

    // Write records and index them in the B-tree
    // First, seek to the data section
    let mut current_file = file.try_clone()?;
    current_file.seek(SeekFrom::Start(HEADER_SIZE + BTREE_SIZE))?;

    // Keep track of record positions
    let mut record_positions = Vec::new();

    for record in &records {
        // Get current position as record offset
        let record_pos = current_file.stream_position()?;
        record_positions.push((record.id, record_pos));

        // Write record
        let encoded = record.encode();
        println!(
            "Writing record id={} at position {} (size {} bytes)",
            record.id,
            record_pos,
            encoded.len()
        );
        current_file.write_all(&encoded)?;
    }

    // Insert all records into B-tree by ID
    for (id, pos) in record_positions {
        btree.insert(&KeyType::I64(id), pos)?;
    }

    // Update header with record count and B-tree root
    let updated_header = FileHeader {
        magic: *b"FLAT",
        version: 1,
        record_count: records.len() as u32,
        btree_root_offset: root_offset,
    };

    // Write updated header
    let mut header_file = file.try_clone()?;
    header_file.seek(SeekFrom::Start(0))?;
    header_file.write_all(&updated_header.encode())?;

    // Ensure everything is flushed
    btree.flush()?;

    println!(
        "file created with {} records and b-tree index",
        records.len()
    );

    // Return the original file, now containing our data
    Ok(file)
}

/// Read from our custom file format using the embedded B-tree
fn read_from_file(mut file: File) -> Result<()> {
    println!("reading from file with embedded b-tree...");

    // Read the header
    let mut header_bytes = vec![0u8; HEADER_SIZE as usize];
    file.seek(SeekFrom::Start(0))?;
    file.read_exact(&mut header_bytes)?;

    let header = FileHeader::decode(&header_bytes)?;
    println!("header: {:?}", header);

    // Create a new file reference for the B-tree
    let file_for_btree = file.try_clone()?;

    // Create B-tree storage at the same location as when writing
    let storage = GenericBlockStorage::with_bounds(
        file_for_btree,
        HEADER_SIZE,                    // Start after header
        Some(HEADER_SIZE + BTREE_SIZE), // Limit to allocated section
        BLOCK_SIZE,                     // Block size
        10,                             // Cache size
    );

    // Open existing B-tree using the root from header
    let key_encoder = Box::new(AnyKeyEncoder::i64());
    let btree = BTree::open(storage, key_encoder, header.btree_root_offset);

    // Look up records by ID
    println!("looking up records by id...");

    for id in 1..=5 {
        // Find record position using B-tree
        match btree.search(&KeyType::I64(id))? {
            Some(record_pos) => {
                println!("Record id={} found at position {}", id, record_pos);

                // Seek to record position
                file.seek(SeekFrom::Start(record_pos))?;

                // Read enough bytes for most records
                let mut buffer = vec![0u8; 100];
                let bytes_read = file.read(&mut buffer)?;
                buffer.truncate(bytes_read);

                // Decode record
                match Record::decode(&buffer) {
                    Ok((record, _)) => {
                        println!("Successfully decoded record id={}: {:?}", id, record);
                    }
                    Err(e) => {
                        println!("Error decoding record at position {}: {:?}", record_pos, e);
                    }
                }
            }
            None => {
                println!("record with id={} not found", id);
            }
        }
    }

    // Also demonstrate a range query
    println!("\nrange query for ids 2-4:");
    let results = btree.range_query(&KeyType::I64(2), &KeyType::I64(4))?;

    println!("found {} records in range", results.len());

    for (idx, record_pos) in results.iter().enumerate() {
        println!("Range result {}: position {}", idx + 1, record_pos);

        // Seek to record position
        file.seek(SeekFrom::Start(*record_pos))?;

        // Read and decode
        let mut buffer = vec![0u8; 100];
        let bytes_read = file.read(&mut buffer)?;
        buffer.truncate(bytes_read);

        match Record::decode(&buffer) {
            Ok((record, _)) => {
                println!("Range record {}: {:?}", idx + 1, record);
            }
            Err(e) => {
                println!(
                    "Error decoding range record at position {}: {:?}",
                    record_pos, e
                );
            }
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    // Create a file with our format and an embedded B-tree
    println!("==========================================");
    println!("Creating composite file format with B-tree");
    println!("==========================================");
    let file = create_file_with_btree()?;

    println!("\n");
    println!("==========================================");
    println!("Reading from file using embedded B-tree");
    println!("==========================================");
    // Read from the file using the B-tree for lookups
    read_from_file(file)?;

    println!("\nExample completed successfully!");

    Ok(())
}
