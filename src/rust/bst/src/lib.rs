mod byte_serializable;
mod query;
mod sorted_index;

#[cfg(test)]
mod tests {
    use crate::byte_serializable::ByteSerializable;
    use crate::query::{MultiIndex, Operator, Query, QueryCondition};
    use crate::sorted_index::{KeyValue, SortedIndex, ValueOffset};
    use chrono::NaiveDate;
    use ordered_float::OrderedFloat;
    use std::error::Error;

    #[test]
    fn test_queries() -> Result<(), Box<dyn Error>> {
        // Sample records (id, city, height of the building, year of construction)
        let records = vec![
            (
                0,
                "Delft".to_string(),
                20.0,
                NaiveDate::from_ymd_opt(1920, 1, 1).unwrap(),
            ),
            (
                1,
                "Amsterdam".to_string(),
                60.5,
                NaiveDate::from_ymd_opt(1950, 1, 1).unwrap(),
            ),
            (
                2,
                "Rotterdam".to_string(),
                25.5,
                NaiveDate::from_ymd_opt(1970, 1, 1).unwrap(),
            ),
            (
                3,
                "Utrecht".to_string(),
                30.6,
                NaiveDate::from_ymd_opt(1980, 1, 1).unwrap(),
            ),
            (
                4,
                "Tokyo".to_string(),
                100.2,
                NaiveDate::from_ymd_opt(1990, 1, 1).unwrap(),
            ),
            (
                5,
                "Osaka".to_string(),
                65.3,
                NaiveDate::from_ymd_opt(2000, 1, 1).unwrap(),
            ),
            (
                6,
                "Kyoto".to_string(),
                28.4,
                NaiveDate::from_ymd_opt(2005, 1, 1).unwrap(),
            ),
            (
                7,
                "Fukuoka".to_string(),
                35.5,
                NaiveDate::from_ymd_opt(2010, 1, 1).unwrap(),
            ),
        ];

        // Build index entries for each field.
        let mut id_entries: Vec<KeyValue<u64>> = Vec::new();
        let mut city_entries: Vec<KeyValue<String>> = Vec::new();
        let mut height_entries: Vec<KeyValue<OrderedFloat<f64>>> = Vec::new();
        let mut year_entries: Vec<KeyValue<NaiveDate>> = Vec::new();

        for (offset, record) in records.iter().enumerate() {
            let (id, city, height, year) = record;
            let voffset = offset as ValueOffset;

            // Build or update the id index.
            if let Some(kv) = id_entries.iter_mut().find(|kv| kv.key == *id) {
                kv.offsets.push(voffset);
            } else {
                id_entries.push(KeyValue {
                    key: *id,
                    offsets: vec![voffset],
                });
            }

            // Build or update the city index.
            if let Some(kv) = city_entries.iter_mut().find(|kv| kv.key == *city) {
                kv.offsets.push(voffset);
            } else {
                city_entries.push(KeyValue {
                    key: city.clone(),
                    offsets: vec![voffset],
                });
            }

            let height_f64 = *height;
            if let Some(kv) = height_entries.iter_mut().find(|kv| kv.key == height_f64) {
                kv.offsets.push(voffset);
            } else {
                height_entries.push(KeyValue {
                    key: OrderedFloat(height_f64),
                    offsets: vec![voffset],
                });
            }

            // Build or update the year index.
            if let Some(kv) = year_entries.iter_mut().find(|kv| kv.key == *year) {
                kv.offsets.push(voffset);
            } else {
                year_entries.push(KeyValue {
                    key: *year,
                    offsets: vec![voffset],
                });
            }
        }

        // Create SortedIndices and build each index.
        let mut id_index = SortedIndex::new();
        id_index.build_index(id_entries);
        let mut city_index = SortedIndex::new();
        city_index.build_index(city_entries);
        let mut height_index = SortedIndex::new();
        height_index.build_index(height_entries);
        let mut year_index = SortedIndex::new();
        year_index.build_index(year_entries);

        // Create a MultiIndex and register each index by field name.
        let mut multi_index = MultiIndex::new();
        multi_index.add_index("id".to_string(), Box::new(id_index));
        multi_index.add_index("city".to_string(), Box::new(city_index));
        multi_index.add_index("height".to_string(), Box::new(height_index));
        multi_index.add_index("year".to_string(), Box::new(year_index));

        // Query 1: id == 1
        let query1 = Query {
            conditions: vec![QueryCondition {
                field: "id".to_string(),
                operator: Operator::Eq,
                key: 1u64.to_bytes(),
            }],
        };
        let result1 = multi_index.query(query1);
        assert_eq!(result1, vec![1]);

        // Query 2: city == "Amsterdam"
        let query2 = Query {
            conditions: vec![QueryCondition {
                field: "city".to_string(),
                operator: Operator::Eq,
                key: "Amsterdam".to_string().to_bytes(),
            }],
        };
        let result2 = multi_index.query(query2);
        assert_eq!(result2, vec![1]);

        // Query 3: height > 20
        let query3 = Query {
            conditions: vec![QueryCondition {
                field: "height".to_string(),
                operator: Operator::Gt,
                key: (20.0f64).to_bytes(),
            }],
        };
        let result3 = multi_index.query(query3);
        assert_eq!(result3, vec![1, 2, 3, 4, 5, 6, 7]);

        // Query 4: year <= 2000-01-01
        let query4 = Query {
            conditions: vec![QueryCondition {
                field: "year".to_string(),
                operator: Operator::Le,
                key: NaiveDate::from_ymd_opt(2000, 1, 1).unwrap().to_bytes(),
            }],
        };
        let result4 = multi_index.query(query4);
        assert_eq!(result4, vec![0, 1, 2, 3, 4, 5]);

        // Query 5: city != "Tokyo" AND height > 30
        let query5 = Query {
            conditions: vec![
                QueryCondition {
                    field: "city".to_string(),
                    operator: Operator::Ne,
                    key: "Tokyo".to_string().to_bytes(),
                },
                QueryCondition {
                    field: "height".to_string(),
                    operator: Operator::Gt,
                    key: (30.0f64).to_bytes(),
                },
            ],
        };
        let result5 = multi_index.query(query5);
        assert_eq!(result5, vec![1, 3, 5, 7]);

        // Query 6: year between 1950 and 2010 (inclusive) AND city != "Delft"
        let query6 = Query {
            conditions: vec![
                QueryCondition {
                    field: "year".to_string(),
                    operator: Operator::Gt,
                    key: NaiveDate::from_ymd_opt(1950, 1, 1).unwrap().to_bytes(),
                },
                QueryCondition {
                    field: "year".to_string(),
                    operator: Operator::Lt,
                    key: NaiveDate::from_ymd_opt(2010, 1, 1).unwrap().to_bytes(),
                },
                QueryCondition {
                    field: "city".to_string(),
                    operator: Operator::Ne,
                    key: "Delft".to_string().to_bytes(),
                },
            ],
        };
        let result6 = multi_index.query(query6);
        assert_eq!(result6, vec![2, 3, 4, 5, 6]);

        // Query 7: height >= 30 AND height <= 65
        let query7 = Query {
            conditions: vec![
                QueryCondition {
                    field: "height".to_string(),
                    operator: Operator::Ge,
                    key: (30.0f64).to_bytes(),
                },
                QueryCondition {
                    field: "height".to_string(),
                    operator: Operator::Le,
                    key: (65.0f64).to_bytes(),
                },
            ],
        };
        let result7 = multi_index.query(query7);
        assert_eq!(result7, vec![1, 3, 7]);

        // Query 8: year > 1970 AND city == "Rotterdam"
        let query8 = Query {
            conditions: vec![
                QueryCondition {
                    field: "year".to_string(),
                    operator: Operator::Gt,
                    key: NaiveDate::from_ymd_opt(1970, 1, 1).unwrap().to_bytes(),
                },
                QueryCondition {
                    field: "city".to_string(),
                    operator: Operator::Eq,
                    key: "Rotterdam".to_string().to_bytes(),
                },
            ],
        };
        let result8 = multi_index.query(query8);
        assert_eq!(result8, vec![]);

        // Query 9: city != "Utrecht" AND city != "Osaka" AND height > 25
        let query9 = Query {
            conditions: vec![
                QueryCondition {
                    field: "city".to_string(),
                    operator: Operator::Ne,
                    key: "Utrecht".to_string().to_bytes(),
                },
                QueryCondition {
                    field: "city".to_string(),
                    operator: Operator::Ne,
                    key: "Osaka".to_string().to_bytes(),
                },
                QueryCondition {
                    field: "height".to_string(),
                    operator: Operator::Gt,
                    key: (25.0f64).to_bytes(),
                },
            ],
        };
        let result9 = multi_index.query(query9);
        assert_eq!(result9, vec![1, 2, 4, 6, 7]);

        Ok(())
    }
}
