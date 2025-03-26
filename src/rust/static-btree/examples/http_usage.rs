#[cfg(feature = "http")]
use static_btree::{AsyncBTreeStorage, HttpStorage};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "http")]
    {
        println!("Static B+Tree HTTP Storage Example");
        println!("---------------------------------");

        // URL to a remote B+Tree file
        let url = "https://example.com/btree.data";

        println!("Connecting to B+Tree at {}", url);

        // Create HTTP storage with a cache of 100 nodes
        let storage = HttpStorage::new(url.to_string(), Some(100)).await?;

        println!("Connected to B+Tree with:");
        println!("  - Node size: {} bytes", storage.node_size());
        println!("  - Node count: {}", storage.node_count());

        // Read a node
        println!("\nReading node 0...");
        let mut buffer = vec![0u8; storage.node_size()];
        let bytes_read = storage.read_node(0, &mut buffer).await?;

        println!("Read {} bytes from node 0", bytes_read);
        println!(
            "First 20 bytes: {:?}",
            buffer[0..20.min(bytes_read)].to_vec()
        );

        // Read another node (should be cached if we read it again)
        println!("\nReading node 1...");
        let bytes_read = storage.read_node(1, &mut buffer).await?;

        println!("Read {} bytes from node 1", bytes_read);
        println!(
            "First 20 bytes: {:?}",
            buffer[0..20.min(bytes_read)].to_vec()
        );

        // Read node 1 again (should be cached)
        println!("\nReading node 1 again (should be from cache)...");
        let bytes_read = storage.read_node(1, &mut buffer).await?;

        println!("Read {} bytes from node 1", bytes_read);
        println!(
            "First 20 bytes: {:?}",
            buffer[0..20.min(bytes_read)].to_vec()
        );

        println!("\nHTTP storage example completed successfully!");
    }

    #[cfg(not(feature = "http"))]
    {
        println!("HTTP feature not enabled");
        println!("Build with '--features http' to run this example");
    }

    Ok(())
}
