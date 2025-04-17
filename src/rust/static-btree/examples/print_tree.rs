use static_btree::{Entry, Error, Key, StaticBTreeBuilder};
use std::io::{Cursor, Read, Seek, SeekFrom};

fn main() -> Result<(), Error> {
    // Create a tree with branching factor 2
    let b: u16 = 2;
    let cursor = Cursor::new(Vec::new());
    let builder = StaticBTreeBuilder::<i32, _>::new(cursor, b)?;

    // Define some test entries
    let entries = vec![
        Ok(Entry { key: 10, offset: 1 }),
        Ok(Entry { key: 20, offset: 2 }),
        Ok(Entry { key: 30, offset: 3 }),
        Ok(Entry { key: 40, offset: 4 }),
        Ok(Entry { key: 50, offset: 5 }),
    ];

    // Build the tree
    let tree = builder.build_from_sorted(entries)?;

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
    let offset_size = std::mem::size_of::<u64>();
    let entry_size = key_size + offset_size;
    let leaf_node_size = b as usize * entry_size;
    let internal_node_size = b as usize * key_size;

    // Read header to get metadata
    let mut reader = Cursor::new(buffer);
    reader.seek(SeekFrom::Start(6))?; // Skip magic bytes

    let mut u16_buf = [0u8; 2];
    reader.read_exact(&mut u16_buf)?;
    let version = u16::from_le_bytes(u16_buf);

    let mut u64_buf = [0u8; 8];
    reader.read_exact(&mut u64_buf)?;
    let root_offset = u64::from_le_bytes(u64_buf);

    let mut u8_buf = [0u8; 1];
    reader.read_exact(&mut u8_buf)?;
    let height = u8::from_le_bytes(u8_buf);

    reader.read_exact(&mut u16_buf)?;
    let node_size = u16::from_le_bytes(u16_buf);

    reader.read_exact(&mut u64_buf)?;
    let num_entries = u64::from_le_bytes(u64_buf);

    println!("===== STATIC B-TREE STRUCTURE =====");
    println!("Version: {}", version);
    println!("Root Offset: {}", root_offset);
    println!("Height: {}", height);
    println!("Node Size: {}", node_size);
    println!("Entries: {}", num_entries);

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

                // Skip node type
                let mut type_buf = [0u8; 1];
                if reader.read_exact(&mut type_buf).is_err() {
                    println!("    Error reading node type");
                    continue;
                }

                // Read count
                let mut count_buf = [0u8; 2];
                if reader.read_exact(&mut count_buf).is_err() {
                    println!("    Error reading count");
                    continue;
                }
                let count = u16::from_le_bytes(count_buf);

                // Read next leaf pointer
                let mut next_leaf_buf = [0u8; 8];
                if reader.read_exact(&mut next_leaf_buf).is_err() {
                    println!("    Error reading next leaf pointer");
                    continue;
                }
                let next_leaf = u64::from_le_bytes(next_leaf_buf);

                println!("    Count: {}, Next Leaf: {}", count, next_leaf);

                // Read entries
                for i in 0..count {
                    let mut key_buf = [0u8; 4]; // i32
                    let mut offset_buf = [0u8; 8]; // u64

                    if reader.read_exact(&mut key_buf).is_ok()
                        && reader.read_exact(&mut offset_buf).is_ok()
                    {
                        let key = i32::from_le_bytes(key_buf);
                        let offset = u64::from_le_bytes(offset_buf);

                        println!("    Entry {}: Key={}, Offset={}", i, key, offset);
                    } else {
                        println!("    Entry {}: <error reading>", i);
                        break;
                    }
                }
            } else {
                // Print internal node (keys)
                println!("  Internal Node {} (offset {})", node, node_offset);
                let mut reader = Cursor::new(&buffer[node_offset as usize..]);

                // Skip node type
                let mut type_buf = [0u8; 1];
                if reader.read_exact(&mut type_buf).is_err() {
                    println!("    Error reading node type");
                    continue;
                }

                // Read count
                let mut count_buf = [0u8; 2];
                if reader.read_exact(&mut count_buf).is_err() {
                    println!("    Error reading count");
                    continue;
                }
                let count = u16::from_le_bytes(count_buf);

                println!("    Count: {}", count);

                // Read keys and child pointers
                for i in 0..count {
                    let mut key_buf = [0u8; 4]; // i32
                    let mut child_buf = [0u8; 8]; // u64

                    if reader.read_exact(&mut key_buf).is_ok()
                        && reader.read_exact(&mut child_buf).is_ok()
                    {
                        let key = i32::from_le_bytes(key_buf);
                        let child = u64::from_le_bytes(child_buf);

                        println!("    Key {}: {}, Child: {}", i, key, child);
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
    let tree = static_btree::StaticBTree::deserialize(&mut reader)?;

    for key in &[10, 20, 30, 40, 50, 25, 45, 55] {
        match tree.find(key, &mut Cursor::new(buffer)) {
            Ok(offsets) => {
                if offsets.is_empty() {
                    println!("Key {} not found", key);
                } else {
                    for offset in offsets {
                        println!("Found key {} -> offset {}", key, offset);
                    }
                }
            }
            Err(e) => println!("Error finding key {}: {:?}", key, e),
        }
    }

    println!("===== END OF LOOKUPS =====");
    Ok(())
}
