use static_btree::{KeyType, StaticBTree};
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("static b+tree basic usage example");
    println!("--------------------------------");

    // Create a tree with i32 keys and branching factor 16
    let mut builder = StaticBTree::<i32>::builder(16, KeyType::I32);

    // Number of entries to insert
    let n = 100_000;
    println!("building tree with {} entries...", n);

    // Add entries to the tree (in this case, key = value)
    let start = Instant::now();
    for i in 0..n {
        let key = (i as i32).to_le_bytes();
        builder.add_entry(&key, i as u64);
    }

    // Build the tree
    let tree = builder.build()?;
    let build_time = start.elapsed();

    println!("tree built in {:?}", build_time);
    println!("tree height: {}", tree.height());
    println!("tree size: {} entries", tree.len());
    println!("tree branching factor: {}", tree.branching_factor());
    println!("tree memory usage: ~{} bytes", tree.data().len());

    // Perform some lookups
    println!("\nperforming lookups...");
    let start = Instant::now();

    // Try to find some existing keys
    for i in (0..n).step_by(n / 10) {
        let key = (i as i32).to_le_bytes();
        let values = tree.find(&key)?;
        match values.first() {
            Some(&value) => println!("found key {}: {} ({} values)", i, value, values.len()),
            None => println!("key {} not found (this should not happen)", i),
        }
    }

    // Try to find a non-existent key
    let non_existent = ((n + 1000) as i32).to_le_bytes();
    let values = tree.find(&non_existent)?;
    match values.is_empty() {
        false => println!("found non-existent key: {:?} (unexpected)", values),
        true => println!("non-existent key not found (expected)"),
    }

    let lookup_time = start.elapsed();
    println!("lookups completed in {:?}", lookup_time);

    // Perform a range query
    println!("\nperforming range query...");
    let start = Instant::now();

    let range_start = ((n / 2) as i32).to_le_bytes();
    let range_end = ((n / 2 + 10) as i32).to_le_bytes();
    let results = tree.range(&range_start, &range_end)?;

    println!("range query returned {} results", results.len());
    for (i, (key, value)) in results.iter().enumerate().take(5) {
        let key_val = i32::from_le_bytes([key[0], key[1], key[2], key[3]]);
        println!("result {}: key {} -> value {}", i, key_val, value);
    }

    let range_time = start.elapsed();
    println!("range query completed in {:?}", range_time);

    Ok(())
}
