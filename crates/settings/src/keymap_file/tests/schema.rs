use super::*;

#[test]
fn keymap_schema_for_unbind_excludes_null_and_unbind_action() {
    fn schema_allows(schema: &Value, expected: &Value) -> bool {
        match schema {
            Value::Object(object) => {
                if object.get("const") == Some(expected) {
                    return true;
                }
                if object.get("type") == Some(&Value::String("null".to_string()))
                    && expected == &Value::Null
                {
                    return true;
                }
                object.values().any(|value| schema_allows(value, expected))
            }
            Value::Array(items) => items.iter().any(|value| schema_allows(value, expected)),
            _ => false,
        }
    }

    let schema = KeymapFile::generate_json_schema_from_inventory();
    let unbind_schema = schema
        .pointer("/$defs/UnbindTargetAction")
        .expect("missing UnbindTargetAction schema");

    assert!(!schema_allows(unbind_schema, &Value::Null));
    assert!(!schema_allows(
        unbind_schema,
        &Value::String(Unbind::name_for_type().to_string())
    ));
    assert!(schema_allows(
        unbind_schema,
        &Value::String("test_keymap_file::StringAction".to_string())
    ));
    assert!(schema_allows(
        unbind_schema,
        &Value::String("test_keymap_file::InputAction".to_string())
    ));
}
