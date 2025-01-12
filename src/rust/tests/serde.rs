use anyhow::Result;
use flatbuffers::FlatBufferBuilder;
use flatcitybuf::{
    attribute::{AttributeSchema, AttributeSchemaMethods},
    deserializer::decode_attributes,
    root_as_city_feature, root_as_header,
    serializer::{to_columns, to_fcb_attribute},
    CityFeature, CityFeatureArgs, CityObject, CityObjectArgs, Header, HeaderArgs,
};
use serde_json::json;

#[test]
fn test_attribute_serialization() -> Result<()> {
    let test_cases = vec![
        // Case 1: Same schema
        (
            json!({
                "attributes": {
                    "int": -10,
                    "uint": 5,
                    "bool": true,
                    "float": 1.0,
                    "string": "hoge",
                    "array": [1, 2, 3],
                    "json": {
                        "hoge": "fuga"
                    },
                }
            }),
            json!({
                "attributes": {
                    "int": -10,
                    "uint": 5,
                    "bool": true,
                    "float": 1.0,
                    "string": "hoge",
                    "array": [1, 2, 3],
                    "json": {
                        "hoge": "fuga"
                    },
                }
            }),
            "same schema",
        ),
        // Case 2: JSON with null value
        (
            json!({
                "attributes": {
                    "int": -10,
                    "uint": 5,
                    "bool": true,
                    "float": 1.0,
                    "string": "hoge",
                    "array": [1, 2, 3],
                    "json": {
                        "hoge": "fuga"
                    },
                    "exception": null
                }
            }),
            json!({
                "attributes": {
                    "int": -10,
                    "uint": 5,
                    "bool": true,
                    "float": 1.0,
                    "string": "hoge",
                    "array": [1, 2, 3],
                    "json": {
                        "hoge": "fuga"
                    },
                    "exception": 1000
                }
            }),
            "JSON with null value",
        ),
        // Case 3: JSON is empty
        (
            json!({
                "attributes": {}
            }),
            json!({
                "attributes": {
                    "int": -10,
                    "uint": 5,
                    "bool": true,
                    "float": 1.0,
                    "string": "hoge",
                    "array": [1, 2, 3],
                    "json": {
                        "hoge": "fuga"
                    },
                    "exception": 1000
                }
            }),
            "JSON is empty",
        ),
    ];

    for (json_data, schema, test_name) in test_cases {
        println!("Testing case: {}", test_name);

        let attrs = &json_data["attributes"];
        let attr_schema = &schema["attributes"];

        // Create and encode with schema
        let mut fbb = FlatBufferBuilder::new();
        let mut common_schema = AttributeSchema::new();
        common_schema.add_attributes(attr_schema);

        let columns = to_columns(&mut fbb, &common_schema);
        let header = {
            let version = fbb.create_string("1.0.0");
            Header::create(
                &mut fbb,
                &HeaderArgs {
                    version: Some(version),
                    columns: Some(columns),
                    ..Default::default()
                },
            )
        };
        fbb.finish(header, None);

        // Decode and verify
        let finished_data = fbb.finished_data();
        let header_buf = root_as_header(finished_data).unwrap();

        let mut fbb = FlatBufferBuilder::new();
        let feature = {
            let (attr_buf, _) = to_fcb_attribute(&mut fbb, attrs, &common_schema);
            let city_object = {
                let id = fbb.create_string("test");
                CityObject::create(
                    &mut fbb,
                    &CityObjectArgs {
                        id: Some(id),
                        attributes: Some(attr_buf),
                        ..Default::default()
                    },
                )
            };
            let objects = fbb.create_vector(&[city_object]);
            let cf_id = fbb.create_string("test_feature");
            CityFeature::create(
                &mut fbb,
                &CityFeatureArgs {
                    id: Some(cf_id),
                    objects: Some(objects),
                    ..Default::default()
                },
            )
        };

        fbb.finish(feature, None);

        let finished_data = fbb.finished_data();
        let feature_buf = root_as_city_feature(finished_data).unwrap();
        let attributes = feature_buf.objects().unwrap().get(0).attributes().unwrap();

        let decoded = decode_attributes(header_buf.columns().unwrap(), attributes);
        assert_eq!(
            attrs, &decoded,
            "decoded data should match original for {}",
            test_name
        );
    }

    Ok(())
}
