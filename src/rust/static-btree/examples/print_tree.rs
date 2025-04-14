use static_btree::{Entry, Error, Key, StaticBTreeBuilder};
use std::io::{Cursor, Read, Seek, SeekFrom};

fn main() -> Result<(), Error> {
    // Create a tree with branching factor 2
    let b: u16 = 2;
    let mut cursor = Cursor::new(Vec::new());
    let builder = StaticBTreeBuilder::<i32, _>::new(&mut cursor, b)?;

    // Define some test entries
    let entries = vec![
        Ok(Entry { key: 10, value: 1 }),
        Ok(Entry { key: 20, value: 2 }),
        Ok(Entry { key: 30, value: 3 }),
        Ok(Entry { key: 40, value: 4 }),
        Ok(Entry { key: 50, value: 5 }),
    ];

    // Build the tree
    builder.build_from_sorted(entries)?;

    // Get the buffer for analysis
    let buffer = cursor.into_inner();

    // Print the tree structure
    print_tree_structure(&buffer, b)?;

    // Test some lookups
    test_lookups(&buffer)?;

    Ok(())
}

fn print_tree_structure(buffer: &[u8], b: u16) -> Result<(), Error> {
    // Constants
    let header_size = static_btree::builder::DEFAULT_HEADER_RESERVATION as usize;
    let key_size = std::mem::size_of::<i32>();
    let value_size = std::mem::size_of::<u64>();
    let entry_size = key_size + value_size;
    let leaf_node_size = b as usize * entry_size;
    let internal_node_size = b as usize * key_size;

    // Read header to get metadata
    let mut reader = Cursor::new(buffer);
    reader.seek(SeekFrom::Start(8))?; // Skip magic bytes

    let mut u16_buf = [0u8; 2];
    reader.read_exact(&mut u16_buf)?;
    let version = u16::from_le_bytes(u16_buf);

    reader.read_exact(&mut u16_buf)?;
    let branching_factor = u16::from_le_bytes(u16_buf);

    let mut u64_buf = [0u8; 8];
    reader.read_exact(&mut u64_buf)?;
    let num_entries = u64::from_le_bytes(u64_buf);

    let mut u8_buf = [0u8; 1];
    reader.read_exact(&mut u8_buf)?;
    let height = u8::from_le_bytes(u8_buf);

    println!("===== STATIC B-TREE STRUCTURE =====");
    println!("Version: {}", version);
    println!("Branching Factor: {}", branching_factor);
    println!("Entries: {}", num_entries);
    println!("Height: {}", height);

    // Calculate layout
    let num_leaf_nodes = (num_entries + b as u64 - 1) / b as u64;
    let mut nodes_per_level = vec![num_leaf_nodes];
    let mut num_children = num_leaf_nodes;

    for _ in 1..height {
        let num_nodes = (num_children + b as u64 - 1) / b as u64;
        nodes_per_level.push(num_nodes);
        num_children = num_nodes;
    }

    nodes_per_level.reverse(); // Root first

    // Calculate offsets
    let mut level_offsets = Vec::new();
    let mut offset = header_size as u64;

    for level in 0..height as usize {
        level_offsets.push(offset);
        let node_size = if level == height as usize - 1 {
            leaf_node_size as u64
        } else {
            internal_node_size as u64
        };
        offset += nodes_per_level[level] * node_size;
    }

    // Print each level
    for level in 0..height as usize {
        let is_leaf = level == height as usize - 1;
        let node_size = if is_leaf {
            leaf_node_size
        } else {
            internal_node_size
        };
        let num_nodes = nodes_per_level[level];

        println!("\n--- Level {} ({} nodes) ---", level, num_nodes);

        for node in 0..num_nodes {
            let node_offset = level_offsets[level] + node * node_size as u64;

            if is_leaf {
                // Print leaf node (entries)
                println!("  Leaf Node {} (offset {})", node, node_offset);
                let mut reader = Cursor::new(&buffer[node_offset as usize..]);

                for i in 0..b {
                    let mut key_buf = [0u8; 4]; // i32
                    let mut value_buf = [0u8; 8]; // u64

                    if reader.read_exact(&mut key_buf).is_ok()
                        && reader.read_exact(&mut value_buf).is_ok()
                    {
                        let key = i32::from_le_bytes(key_buf);
                        let value = u64::from_le_bytes(value_buf);

                        if key != 0 || value != 0 || i == 0 {
                            println!("    Entry {}: Key={}, Value={}", i, key, value);
                        } else {
                            println!("    Entry {}: <padding>", i);
                        }
                    } else {
                        println!("    Entry {}: <error reading>", i);
                        break;
                    }
                }
            } else {
                // Print internal node (keys)
                println!("  Internal Node {} (offset {})", node, node_offset);
                let mut reader = Cursor::new(&buffer[node_offset as usize..]);

                for i in 0..b {
                    let mut key_buf = [0u8; 4]; // i32

                    if reader.read_exact(&mut key_buf).is_ok() {
                        let key = i32::from_le_bytes(key_buf);

                        if key != 0 || i == 0 {
                            println!("    Key {}: {}", i, key);
                        } else {
                            println!("    Key {}: <padding>", i);
                        }
                    } else {
                        println!("    Key {}: <error reading>", i);
                        break;
                    }
                }
            }
        }
    }

    println!("\n===== END OF TREE STRUCTURE =====");
    Ok(())
}

fn test_lookups(buffer: &[u8]) -> Result<(), Error> {
    println!("\n===== TESTING LOOKUPS =====");

    let mut reader = Cursor::new(buffer);
    let mut tree = static_btree::StaticBTree::<i32, _>::open(reader)?;

    for key in &[10, 20, 30, 40, 50, 25, 45, 55] {
        match tree.find(key) {
            Ok(Some(value)) => println!("Found key {} -> value {}", key, value),
            Ok(None) => println!("Key {} not found", key),
            Err(e) => println!("Error finding key {}: {:?}", key, e),
        }
    }

    println!("===== END OF LOOKUPS =====");
    Ok(())
}
