use anyhow::Result;
use flatbuffers::FlatBufferBuilder;
use flatcitybuf::{
    attribute::{AttributeSchema, AttributeSchemaMethods},
    fcb_deserializer::decode_attributes,
    fcb_serializer::{to_fcb_attribute, to_fcb_columns},
    root_as_city_feature, root_as_header, CityFeature, CityFeatureArgs, CityObject, CityObjectArgs,
    Header, HeaderArgs,
};
use serde_json::json;

#[test]
fn test_attribute_serialization() -> Result<()> {
    let json_data = json!({
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
            "exceptional": null
        }
    });
    let schema = json!({
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
            "exceptional": 1000
        }
    });
    let attrs = &json_data["attributes"];
    let attr_schema = &schema["attributes"];

    // Test case 1: Using common schema
    {
        let mut fbb = FlatBufferBuilder::new();
        let mut common_schema = AttributeSchema::new();
        common_schema.add_attributes(attr_schema);

        let columns = to_fcb_columns(&mut fbb, &common_schema);
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
        let finished_data = fbb.finished_data();
        let header_buf = root_as_header(finished_data).unwrap();
        let mut fbb = FlatBufferBuilder::new();
        let feature = {
            let (attr_buf, _) = to_fcb_attribute(&mut fbb, attrs, &common_schema);
            let city_object = {
                let id = fbb.create_string("hoge");
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
            let cf_id = fbb.create_string("hoge");
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
        // Verify encoded data
        assert!(!attributes.is_empty());

        let decoded = decode_attributes(header_buf.columns().unwrap(), attributes);
        assert_eq!(attrs, &decoded);
    }

    Ok(())
}
