use static_btree::{
    key::{I64KeyEncoder, KeyEncoder},
    BTreeStorage, FileStorage, MemoryStorage,
};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Static B+Tree Storage Example");
    println!("-----------------------------");

    // Create in-memory storage for a small tree
    let node_size = 256; // Each node is 256 bytes
    let node_count = 10; // We'll have 10 nodes total
    let mut memory_storage = MemoryStorage::new(node_size, node_count)?;

    // Create some test data
    let key_encoder = I64KeyEncoder;
    let key1 = key_encoder.encode(&1)?;
    let key2 = key_encoder.encode(&2)?;
    let key3 = key_encoder.encode(&3)?;

    // Create a simple entry (would normally be done by the tree implementation)
    let mut entry1 = format!("{:<8}", key1.len()).into_bytes();
    entry1.extend(&key1);
    entry1.extend(&100u64.to_le_bytes());

    let mut entry2 = format!("{:<8}", key2.len()).into_bytes();
    entry2.extend(&key2);
    entry2.extend(&200u64.to_le_bytes());

    let mut entry3 = format!("{:<8}", key3.len()).into_bytes();
    entry3.extend(&key3);
    entry3.extend(&300u64.to_le_bytes());

    // Write entries to nodes
    memory_storage.write_node(0, &entry1)?;
    memory_storage.write_node(1, &entry2)?;
    memory_storage.write_node(2, &entry3)?;

    // Read back the entries
    let mut buffer = vec![0u8; node_size];

    memory_storage.read_node(0, &mut buffer)?;
    println!("Node 0: {:?}", buffer[0..20].to_vec());

    memory_storage.read_node(1, &mut buffer)?;
    println!("Node 1: {:?}", buffer[0..20].to_vec());

    memory_storage.read_node(2, &mut buffer)?;
    println!("Node 2: {:?}", buffer[0..20].to_vec());

    // Save to a file
    println!("\nSaving to file...");
    let file_path = Path::new("temp/example.btree");

    // Create directory if it doesn't exist
    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Create file storage
    let mut file_storage = FileStorage::create(&file_path, node_size, node_count)?;

    // Transfer data from memory to file
    for i in 0..node_count {
        memory_storage.read_node(i, &mut buffer)?;
        file_storage.write_node(i, &buffer)?;
    }

    // Flush changes
    file_storage.flush()?;
    println!("Data written to {}", file_path.display());

    // Reopen the file
    println!("\nReopening file...");
    let mut reopened_storage = FileStorage::open(&file_path)?;

    // Read back the entries
    reopened_storage.read_node(0, &mut buffer)?;
    println!("Node 0 (from file): {:?}", buffer[0..20].to_vec());

    reopened_storage.read_node(1, &mut buffer)?;
    println!("Node 1 (from file): {:?}", buffer[0..20].to_vec());

    reopened_storage.read_node(2, &mut buffer)?;
    println!("Node 2 (from file): {:?}", buffer[0..20].to_vec());

    println!("\nStorage example completed successfully!");
    Ok(())
}
