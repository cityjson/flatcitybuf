// #[cfg(test)]
// mod tests {
//     use std::io::{Cursor, Write};

//     use crate::entry::Entry;
//     use crate::error::Result;
//     use crate::query::{
//         MemoryIndex, MemoryMultiIndex, Operator, Query, StreamIndex, StreamMultiIndex,
//     };
//     use crate::stree::Stree;

//     // Test building an index and performing a basic query
//     #[test]
//     fn test_memory_and_stream_indices() -> Result<()> {
//         // Create entries for "name" field
//         let name_entries = vec![
//             Entry::new("alice".to_string(), 1),
//             Entry::new("bob".to_string(), 2),
//             Entry::new("charlie".to_string(), 3),
//             Entry::new("diana".to_string(), 4),
//         ];

//         // Create entries for "age" field
//         let age_entries = vec![
//             Entry::new(25i32, 1), // alice, age 25
//             Entry::new(32i32, 2), // bob, age 32
//             Entry::new(28i32, 3), // charlie, age 28
//             Entry::new(41i32, 4), // diana, age 41
//         ];

//         // Build memory indices
//         let name_memory_index = MemoryIndex::build(&name_entries, 4)?;
//         let age_memory_index = MemoryIndex::build(&age_entries, 4)?;

//         // Create memory multi-index
//         let mut memory_multi_index = MemoryMultiIndex::new();
//         memory_multi_index.add_index("name".to_string(), name_memory_index);
//         memory_multi_index.add_index("age".to_string(), age_memory_index);

//         // Query: name starts with "b" AND age > 30
//         let mut query = Query::new();
//         query.add_condition("name".to_string(), Operator::Ge, "b".to_string());
//         query.add_condition("name".to_string(), Operator::Lt, "c".to_string());
//         query.add_condition("age".to_string(), Operator::Gt, 30i32);

//         let results = memory_multi_index.query(&query)?;
//         assert_eq!(results, vec![2]); // Should match bob (id=2)

//         // Now test stream indices
//         // Serialize the trees to buffers
//         let mut name_buffer = Vec::new();
//         let mut age_buffer = Vec::new();

//         let name_stree = Stree::build(&name_entries, 4)?;
//         let age_stree = Stree::build(&age_entries, 4)?;

//         name_stree.stream_write(&mut name_buffer)?;
//         age_stree.stream_write(&mut age_buffer)?;

//         // Create combined buffer
//         let mut combined_buffer = Vec::new();
//         combined_buffer.write_all(&name_buffer)?;
//         combined_buffer.write_all(&age_buffer)?;

//         let mut cursor = Cursor::new(combined_buffer);

//         // Create stream indices
//         let name_stream_index = StreamIndex::new(
//             name_entries.len(),
//             4,
//             0, // Offset at the start
//             0, // No payload
//         );

//         let age_stream_index = StreamIndex::new(
//             age_entries.len(),
//             4,
//             name_buffer.len() as u64, // Offset after name_buffer
//             0,                        // No payload
//         );

//         // Create stream multi-index
//         let mut stream_multi_index = StreamMultiIndex::new();
//         stream_multi_index.add_index("name".to_string(), name_stream_index);
//         stream_multi_index.add_index("age".to_string(), age_stream_index);

//         // Run the same query
//         let results = stream_multi_index.query_with_reader(&mut cursor, &query)?;
//         assert_eq!(results, vec![2]); // Should match bob (id=2)

//         Ok(())
//     }
// }
