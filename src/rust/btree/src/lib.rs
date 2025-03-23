mod entry;
mod errors;
mod http;
mod key;
mod node;
mod query;
mod storage;
mod stream;
mod tree;

// Re-export primary types and functions
pub use entry::Entry;
pub use errors::{BTreeError, KeyError, Result};
#[cfg(feature = "http")]
pub use http::{HttpBTreeBuilder, HttpBTreeReader, HttpBlockStorage, HttpConfig, HttpMetrics};
pub use key::{FloatKeyEncoder, I64KeyEncoder, KeyEncoder, StringKeyEncoder};
pub use node::{Node, NodeType};
pub use query::conditions;
pub use query::{
    AttributeQuery, Condition, LogicalOp, QueryBuilder, QueryExecutor, QueryExpr, QueryResult,
    RTreeIndex, SpatialQuery,
};
pub use storage::{BlockStorage, CachedFileBlockStorage, MemoryBlockStorage};
pub use stream::{BTreeReader, BTreeStreamProcessor};
pub use tree::{BTree, BTreeIndex};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_tree_test() {
        // Simple test for B-tree functionality
        let storage = MemoryBlockStorage::new(4096);
        let key_encoder = Box::new(I64KeyEncoder);

        // Create a new B-tree
        let mut btree = BTree::new(storage, key_encoder).unwrap();

        // Insert some test data
        btree.insert(&1, 100).unwrap();
        btree.insert(&2, 200).unwrap();
        btree.insert(&3, 300).unwrap();

        // Search for a key
        let result = btree.search(&2).unwrap();
        assert_eq!(result, Some(200));

        // Search for a non-existent key
        let result = btree.search(&4).unwrap();
        assert_eq!(result, None);

        // Range query
        let results = btree.range_query(&1, &3).unwrap();
        assert_eq!(results.len(), 3);
        assert!(results.contains(&100));
        assert!(results.contains(&200));
        assert!(results.contains(&300));
    }

    /// This test demonstrates how to use the query system
    /// (no actual test, just showing the API usage)
    #[test]
    fn query_system_example() {
        // This is just an example of the API, not a real test

        // 1. Create B-tree indices for different attributes
        let name_storage = MemoryBlockStorage::new(4096);
        let name_encoder = Box::new(StringKeyEncoder { prefix_length: 16 });
        let name_btree = BTree::open(name_storage, name_encoder, 0); // root at offset 0

        let height_storage = MemoryBlockStorage::new(4096);
        let height_encoder = Box::new(FloatKeyEncoder);
        let height_btree = BTree::open(height_storage, height_encoder, 0);

        // 2. Create a query executor and register indices
        let mut executor = QueryExecutor::new();
        executor
            .register_btree("name".to_string(), &name_btree)
            .register_btree("height".to_string(), &height_btree);
        // Could also register an R-tree with .register_rtree(rtree_index)

        // 3. Build a query using the builder pattern
        let query = QueryBuilder::new()
            // Find all buildings named "Tower"
            .attribute("name", conditions::eq("Tower".to_string()), None)
            // AND with height between 100 and 200 meters
            .attribute(
                "height",
                conditions::between(100.0, 200.0),
                Some(LogicalOp::And),
            )
            // AND within a bounding box
            .spatial(10.0, 20.0, 30.0, 40.0, Some(LogicalOp::And))
            .build()
            .unwrap();

        // 4. Execute the query
        // let result = executor.execute(&query).unwrap();

        // 5. Process results
        // for feature_id in result.feature_ids {
        //     println!("Found feature with ID: {}", feature_id);
        // }
    }
}
