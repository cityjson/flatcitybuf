#[cfg(test)]
mod tests {
    use crate::error::Result;
    use crate::query::memory::{KeyType, TypedQueryCondition};
    use crate::{MemoryIndex, MemoryMultiIndex, Operator, StreamIndex};
    use crate::{SearchIndex, StreamMultiIndex, Stree};
    use chrono::{DateTime, Utc};
    use ordered_float::OrderedFloat;
    use std::collections::HashMap;
    use std::io::Cursor;
    use std::str::FromStr;

    use super::*;
    use crate::entry::Entry;
    use crate::key::FixedStringKey;

    #[test]
    fn test_memory_index_with_complex_data() -> Result<()> {
        // Create a more complex dataset with duplicates and edge cases
        let entries = vec![
            Entry::new(0_i64, 1000_u64),
            Entry::new(1_i64, 1001_u64),
            Entry::new(1_i64, 1101_u64), // Duplicate key
            Entry::new(2_i64, 1002_u64),
            Entry::new(3_i64, 1003_u64),
            Entry::new(4_i64, 1004_u64),
            Entry::new(5_i64, 1005_u64),
            Entry::new(6_i64, 1006_u64),
            Entry::new(7_i64, 1007_u64),
            Entry::new(8_i64, 1008_u64),
            Entry::new(9_i64, 1009_u64),
            Entry::new(9_i64, 1109_u64), // Duplicate key
            Entry::new(10_i64, 1010_u64),
            Entry::new(11_i64, 1011_u64),
            Entry::new(12_i64, 1012_u64),
            Entry::new(13_i64, 1013_u64),
            Entry::new(14_i64, 1014_u64),
            Entry::new(15_i64, 1015_u64),
            Entry::new(16_i64, 1016_u64),
            Entry::new(17_i64, 1017_u64),
            Entry::new(18_i64, 1018_u64),
        ];

        // Use a branching factor of 4
        let index = MemoryIndex::build(&entries, 4)?;

        // Test exact matches with duplicates
        let results = index.find_exact(1_i64)?;
        assert_eq!(results.len(), 2);
        assert!(results.contains(&1001_u64));
        assert!(results.contains(&1101_u64));

        let results = index.find_exact(9_i64)?;
        assert_eq!(results.len(), 2);
        assert!(results.contains(&1009_u64));
        assert!(results.contains(&1109_u64));

        // Test range queries with edge cases
        // Range that includes duplicates
        let results = index.find_range(Some(1_i64), Some(3_i64))?;
        assert_eq!(results.len(), 4); // 1(x2), 2, 3

        // Range that includes the minimum value
        let results = index.find_range(Some(0_i64), Some(2_i64))?;
        assert_eq!(results.len(), 4); // 0, 1(x2), 2

        // Range that includes the maximum value
        let results = index.find_range(Some(17_i64), Some(18_i64))?;
        assert_eq!(results.len(), 2); // 17, 18

        // Test ranges with open bounds
        let results = index.find_range(Some(15_i64), None)?;
        assert_eq!(results.len(), 4); // 15, 16, 17, 18

        let results = index.find_range(None, Some(2_i64))?;
        assert_eq!(results.len(), 4); // 0, 1(x2), 2

        // Test non-existent values
        let results = index.find_exact(42_i64)?;
        assert!(results.is_empty());

        let results = index.find_range(Some(19_i64), Some(25_i64))?;
        assert!(results.is_empty());

        Ok(())
    }

    fn create_id_index(branching_factor: u16) -> Result<MemoryIndex<i64>> {
        let id_entries = vec![
            Entry::new(0_i64, 0),   // id=0
            Entry::new(1_i64, 1),   // id=1
            Entry::new(2_i64, 2),   // id=2
            Entry::new(3_i64, 3),   // id=3
            Entry::new(4_i64, 4),   // id=4
            Entry::new(5_i64, 5),   // id=5
            Entry::new(6_i64, 6),   // id=6
            Entry::new(7_i64, 7),   // id=7
            Entry::new(8_i64, 8),   // id=8
            Entry::new(9_i64, 9),   // id=9
            Entry::new(10_i64, 10), // id=10
            Entry::new(11_i64, 11), // id=11
            Entry::new(12_i64, 12), // id=12
            Entry::new(13_i64, 13), // id=13
            Entry::new(14_i64, 14), // id=14
            Entry::new(15_i64, 15), // id=15
            Entry::new(16_i64, 16), // id=16
            Entry::new(17_i64, 17), // id=17
            Entry::new(18_i64, 18), // id=18
        ];
        let index = MemoryIndex::<i64>::build(&id_entries, branching_factor)?;
        Ok(index)
    }

    fn create_name_index(branching_factor: u16) -> Result<MemoryIndex<FixedStringKey<20>>> {
        let name_entries = vec![
            Entry::new(FixedStringKey::<20>::from_str("alice"), 1),
            Entry::new(FixedStringKey::<20>::from_str("bob"), 2),
            Entry::new(FixedStringKey::<20>::from_str("charlie"), 3),
            Entry::new(FixedStringKey::<20>::from_str("diana"), 4),
            Entry::new(FixedStringKey::<20>::from_str("eve"), 5),
            Entry::new(FixedStringKey::<20>::from_str("frank"), 6),
            Entry::new(FixedStringKey::<20>::from_str("george"), 7),
            Entry::new(FixedStringKey::<20>::from_str("harry"), 8),
            Entry::new(FixedStringKey::<20>::from_str("irene"), 9),
            Entry::new(FixedStringKey::<20>::from_str("john"), 10),
            Entry::new(FixedStringKey::<20>::from_str("kate"), 11),
            Entry::new(FixedStringKey::<20>::from_str("larry"), 12),
            Entry::new(FixedStringKey::<20>::from_str("mary"), 13),
            Entry::new(FixedStringKey::<20>::from_str("nancy"), 14),
            Entry::new(FixedStringKey::<20>::from_str("oliver"), 15),
            Entry::new(FixedStringKey::<20>::from_str("pat"), 16),
            Entry::new(FixedStringKey::<20>::from_str("quentin"), 17),
            Entry::new(FixedStringKey::<20>::from_str("robert"), 18),
            Entry::new(FixedStringKey::<20>::from_str("sally"), 19),
            Entry::new(FixedStringKey::<20>::from_str("tim"), 20),
            Entry::new(FixedStringKey::<20>::from_str("ursula"), 21),
            Entry::new(FixedStringKey::<20>::from_str("victor"), 22),
        ];
        let index = MemoryIndex::build(&name_entries, branching_factor)?;
        Ok(index)
    }

    fn create_score_index(branching_factor: u16) -> Result<MemoryIndex<OrderedFloat<f32>>> {
        let score_entries = vec![
            Entry::new(OrderedFloat(85.5f32), 1),  // score=85.5
            Entry::new(OrderedFloat(85.5f32), 2),  // score=85.5
            Entry::new(OrderedFloat(85.5f32), 3),  // score=85.5
            Entry::new(OrderedFloat(85.5f32), 4),  // score=85.5
            Entry::new(OrderedFloat(92.0f32), 5),  // score=92.0
            Entry::new(OrderedFloat(78.3f32), 6),  // score=78.3
            Entry::new(OrderedFloat(96.7f32), 7),  // score=96.7
            Entry::new(OrderedFloat(88.1f32), 8),  // score=88.1
            Entry::new(OrderedFloat(88.1f32), 9),  // score=88.1
            Entry::new(OrderedFloat(88.1f32), 10), // score=88.1
            Entry::new(OrderedFloat(88.1f32), 11), // score=88.1
            Entry::new(OrderedFloat(88.1f32), 12), // score=88.1
            Entry::new(OrderedFloat(88.1f32), 13), // score=88.1
            Entry::new(OrderedFloat(88.1f32), 14), // score=88.1
            Entry::new(OrderedFloat(88.1f32), 15), // score=88.1
            Entry::new(OrderedFloat(88.1f32), 16), // score=88.1
            Entry::new(OrderedFloat(88.1f32), 17), // score=88.1
            Entry::new(OrderedFloat(88.1f32), 18), // score=88.1
            Entry::new(OrderedFloat(70.1f32), 19), // score=88.1
        ];
        let index = MemoryIndex::build(&score_entries, branching_factor)?;
        Ok(index)
    }

    fn create_datetime_index(branching_factor: u16) -> Result<MemoryIndex<DateTime<Utc>>> {
        let datetime_offsets = [
            (
                DateTime::<Utc>::from_str("2020-01-01T00:00:00Z").unwrap(),
                [0, 1, 2, 3, 4],
            ),
            (
                DateTime::<Utc>::from_str("2021-01-01T00:00:00Z").unwrap(),
                [5, 6, 7, 8, 9],
            ),
            (
                DateTime::<Utc>::from_str("2022-01-01T00:00:00Z").unwrap(),
                [10, 11, 12, 13, 14],
            ),
            (
                DateTime::<Utc>::from_str("2023-01-01T00:00:00Z").unwrap(),
                [15, 16, 17, 18, 19],
            ),
            (
                DateTime::<Utc>::from_str("2024-01-01T00:00:00Z").unwrap(),
                [20, 21, 22, 23, 24],
            ),
        ];
        let mut datetime_entries = vec![];
        for datetime in datetime_offsets {
            for offset in datetime.1 {
                datetime_entries.push(Entry::new(datetime.0, offset as u64));
            }
        }
        let index = MemoryIndex::build(&datetime_entries, branching_factor)?;
        Ok(index)
    }

    fn create_test_multi_index() -> Result<MemoryMultiIndex> {
        // Build indices
        let id_index = create_id_index(4)?;
        let name_index = create_name_index(4)?;
        let score_index = create_score_index(4)?;
        let datetime_index = create_datetime_index(4)?;

        // Create a multi-index with different key types
        let mut multi_index = MemoryMultiIndex::new();
        multi_index.add_i64_index("id".to_string(), id_index);
        multi_index.add_string_index20("name".to_string(), name_index);
        multi_index.add_f32_index("score".to_string(), score_index);
        multi_index.add_datetime_index("datetime".to_string(), datetime_index);
        Ok(multi_index)
    }

    #[test]
    fn test_memory_multi_index_with_mixed_types() -> Result<()> {
        let test_cases = vec![
            (
                vec![
                    TypedQueryCondition {
                        field: "id".to_string(),
                        operator: Operator::Ge,
                        key: KeyType::Int64(3),
                    },
                    TypedQueryCondition {
                        field: "score".to_string(),
                        operator: Operator::Gt,
                        key: KeyType::Float32(OrderedFloat(80.0)),
                    },
                    TypedQueryCondition {
                        field: "datetime".to_string(),
                        operator: Operator::Ge,
                        key: KeyType::DateTime(
                            DateTime::<Utc>::from_str("2023-01-01T00:00:00Z").unwrap(),
                        ),
                    },
                ],
                vec![15, 16, 17, 18],
            ),
            // Test another query: name starts with "a" or "b" AND score < 95.0
            (
                vec![
                    TypedQueryCondition {
                        field: "name".to_string(),
                        operator: Operator::Eq,
                        key: KeyType::StringKey20(FixedStringKey::<20>::from_str("eve")),
                    },
                    TypedQueryCondition {
                        field: "score".to_string(),
                        operator: Operator::Lt,
                        key: KeyType::Float32(OrderedFloat(95.0)),
                    },
                ],
                vec![5],
            ),
            (
                vec![TypedQueryCondition {
                    field: "name".to_string(),
                    operator: Operator::Eq,
                    key: KeyType::StringKey20(FixedStringKey::<20>::from_str("eve")),
                }],
                vec![5],
            ),
        ];

        // Simply test with multi_index
        let id_index = create_id_index(4)?;
        let name_index = create_name_index(4)?;
        let score_index = create_score_index(4)?;
        let datetime_index = create_datetime_index(4)?;

        let id_index2 = id_index.clone();
        let name_index2 = name_index.clone();
        let score_index2 = score_index.clone();
        let datetime_index2 = datetime_index.clone();

        // Create a multi-index with different key types
        let mut multi_index = MemoryMultiIndex::new();
        multi_index.add_i64_index("id".to_string(), id_index);
        multi_index.add_string_index20("name".to_string(), name_index);
        multi_index.add_f32_index("score".to_string(), score_index);
        multi_index.add_datetime_index("datetime".to_string(), datetime_index);

        for (query, expected_results) in &test_cases {
            let results = multi_index.query(query)?;
            assert_eq!(results, *expected_results);
        }

        // round trip serialize and deserialize, and search items
        // serialize---
        let mut index_buffer = Cursor::new(Vec::<u8>::new());
        let mut total_written = 0;
        struct TreeInfo {
            num_items: usize,
            branching_factor: u16,
            index_offset: usize,
            payload_size: usize,
        }
        // serialize and store bytes represented indices into vector
        let mut index_offset = HashMap::new();
        index_offset.insert(
            "id",
            TreeInfo {
                num_items: id_index2.num_items(),
                branching_factor: id_index2.branching_factor(),
                index_offset: 0,
                payload_size: id_index2.payload_size(),
            },
        );
        total_written += id_index2.serialize(&mut index_buffer)?;
        index_offset.insert(
            "name",
            TreeInfo {
                num_items: name_index2.num_items(),
                branching_factor: name_index2.branching_factor(),
                index_offset: 0,
                payload_size: name_index2.payload_size(),
            },
        );
        total_written += name_index2.serialize(&mut index_buffer)?;
        index_offset.insert(
            "score",
            TreeInfo {
                num_items: score_index2.num_items(),
                branching_factor: score_index2.branching_factor(),
                index_offset: 0,
                payload_size: score_index2.payload_size(),
            },
        );
        total_written += score_index2.serialize(&mut index_buffer)?;
        index_offset.insert(
            "datetime",
            TreeInfo {
                num_items: datetime_index2.num_items(),
                branching_factor: datetime_index2.branching_factor(),
                index_offset: 0,
                payload_size: datetime_index2.payload_size(),
            },
        );
        total_written += datetime_index2.serialize(&mut index_buffer)?;

        // This is for stream query test
        let mut index_buffer_for_stream = index_buffer.clone();

        // deserialize---
        // get start offset, num_items, and branching factor for each index
        let TreeInfo {
            num_items: id_num_items,
            branching_factor: id_b,
            index_offset: id_start,
            payload_size: id_payload_size,
        } = index_offset.get("id").unwrap();
        let TreeInfo {
            num_items: name_num_items,
            branching_factor: name_b,
            index_offset: name_start,
            payload_size: name_payload_size,
        } = index_offset.get("name").unwrap();
        let TreeInfo {
            num_items: score_num_items,
            branching_factor: score_b,
            index_offset: score_start,
            payload_size: score_payload_size,
        } = index_offset.get("score").unwrap();
        let TreeInfo {
            num_items: datetime_num_items,
            branching_factor: datetime_b,
            index_offset: datetime_start,
            payload_size: datetime_payload_size,
        } = index_offset.get("datetime").unwrap();

        // create another buffer from 0..id_start, *name_start.., *score_start.., *datetime_start..
        let mut id_index_buffer =
            Cursor::new(index_buffer.get_ref()[*id_start..*name_start].to_vec());
        let mut name_index_buffer =
            Cursor::new(index_buffer.get_ref()[*name_start..*score_start].to_vec());
        let mut score_index_buffer =
            Cursor::new(index_buffer.get_ref()[*score_start..*datetime_start].to_vec());
        let mut datetime_index_buffer =
            Cursor::new(index_buffer.get_ref()[*datetime_start..].to_vec());

        let id_index = MemoryIndex::<i64>::from_buf(&mut id_index_buffer, *id_num_items, *id_b)?;
        let name_index = MemoryIndex::<FixedStringKey<20>>::from_buf(
            &mut name_index_buffer,
            *name_num_items,
            *name_b,
        )?;
        let score_index = MemoryIndex::<OrderedFloat<f32>>::from_buf(
            &mut score_index_buffer,
            *score_num_items,
            *score_b,
        )?;
        let datetime_index = MemoryIndex::<DateTime<Utc>>::from_buf(
            &mut datetime_index_buffer,
            *datetime_num_items,
            *datetime_b,
        )?;

        let mut multi_index = MemoryMultiIndex::new();
        multi_index.add_i64_index("id".to_string(), id_index);
        multi_index.add_string_index20("name".to_string(), name_index);
        multi_index.add_f32_index("score".to_string(), score_index);
        multi_index.add_datetime_index("datetime".to_string(), datetime_index);

        for (query, expected_results) in &test_cases {
            let results = multi_index.query(&query)?;
            assert_eq!(results, *expected_results);
        }

        // stream query test
        let mut stream_index = StreamMultiIndex::new();
        let stream_id_index =
            StreamIndex::<i64>::new(*id_num_items, *id_b, *id_start as u64, *id_payload_size);
        stream_index.add_index("id".to_string(), stream_id_index);
        let stream_name_index = StreamIndex::<FixedStringKey<20>>::new(
            *name_num_items,
            *name_b,
            *name_start as u64,
            *name_payload_size,
        );
        stream_index.add_index("name".to_string(), stream_name_index);
        let stream_score_index = StreamIndex::<OrderedFloat<f32>>::new(
            *score_num_items,
            *score_b,
            *score_start as u64,
            *score_payload_size,
        );
        stream_index.add_index("score".to_string(), stream_score_index);
        let stream_datetime_index = StreamIndex::<DateTime<Utc>>::new(
            *datetime_num_items,
            *datetime_b,
            *datetime_start as u64,
            *datetime_payload_size,
        );
        stream_index.add_index("datetime".to_string(), stream_datetime_index);
        for (query, expected_results) in &test_cases {
            let results = stream_index.query_with_reader(&mut index_buffer_for_stream, &query)?;
            assert_eq!(results, *expected_results);
        }

        Ok(())
    }
}
