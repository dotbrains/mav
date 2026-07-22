use super::*;

#[test]
fn object_replace_array() {
    // Tests replacing values within arrays that are nested inside objects.
    // Uses "#N" syntax in key paths to indicate array indices.
    #[track_caller]
    fn check_object_replace_array(
        input: String,
        key_path: &[&str],
        value: Option<Value>,
        expected: String,
    ) {
        let result = replace_value_in_json_text(&input, key_path, 4, value.as_ref(), None);
        let mut result_str = input;
        result_str.replace_range(result.0, &result.1);
        pretty_assertions::assert_eq!(expected, result_str);
    }

    // Basic array element replacement
    check_object_replace_array(
        r#"{
                "a": [1, 3],
            }"#
        .unindent(),
        &["a", "#1"],
        Some(json!(2)),
        r#"{
                "a": [1, 2],
            }"#
        .unindent(),
    );

    // Replace first element
    check_object_replace_array(
        r#"{
                "items": [1, 2, 3]
            }"#
        .unindent(),
        &["items", "#0"],
        Some(json!(10)),
        r#"{
                "items": [10, 2, 3]
            }"#
        .unindent(),
    );

    // Replace last element
    check_object_replace_array(
        r#"{
                "items": [1, 2, 3]
            }"#
        .unindent(),
        &["items", "#2"],
        Some(json!(30)),
        r#"{
                "items": [1, 2, 30]
            }"#
        .unindent(),
    );

    // Replace string in array
    check_object_replace_array(
        r#"{
                "names": ["alice", "bob", "charlie"]
            }"#
        .unindent(),
        &["names", "#1"],
        Some(json!("robert")),
        r#"{
                "names": ["alice", "robert", "charlie"]
            }"#
        .unindent(),
    );

    // Replace boolean
    check_object_replace_array(
        r#"{
                "flags": [true, false, true]
            }"#
        .unindent(),
        &["flags", "#0"],
        Some(json!(false)),
        r#"{
                "flags": [false, false, true]
            }"#
        .unindent(),
    );

    // Replace null with value
    check_object_replace_array(
        r#"{
                "values": [null, 2, null]
            }"#
        .unindent(),
        &["values", "#0"],
        Some(json!(1)),
        r#"{
                "values": [1, 2, null]
            }"#
        .unindent(),
    );

    // Replace value with null
    check_object_replace_array(
        r#"{
                "data": [1, 2, 3]
            }"#
        .unindent(),
        &["data", "#1"],
        Some(json!(null)),
        r#"{
                "data": [1, null, 3]
            }"#
        .unindent(),
    );

    // Replace simple value with object
    check_object_replace_array(
        r#"{
                "list": [1, 2, 3]
            }"#
        .unindent(),
        &["list", "#1"],
        Some(json!({"value": 2, "label": "two"})),
        r#"{
                "list": [1, { "value": 2, "label": "two" }, 3]
            }"#
        .unindent(),
    );

    // Replace simple value with nested array
    check_object_replace_array(
        r#"{
                "matrix": [1, 2, 3]
            }"#
        .unindent(),
        &["matrix", "#1"],
        Some(json!([20, 21, 22])),
        r#"{
                "matrix": [1, [ 20, 21, 22 ], 3]
            }"#
        .unindent(),
    );

    // Replace object in array
    check_object_replace_array(
        r#"{
                "users": [
                    {"name": "alice"},
                    {"name": "bob"},
                    {"name": "charlie"}
                ]
            }"#
        .unindent(),
        &["users", "#1"],
        Some(json!({"name": "robert", "age": 30})),
        r#"{
                "users": [
                    {"name": "alice"},
                    { "name": "robert", "age": 30 },
                    {"name": "charlie"}
                ]
            }"#
        .unindent(),
    );

    // Replace property within object in array
    check_object_replace_array(
        r#"{
                "users": [
                    {"name": "alice", "age": 25},
                    {"name": "bob", "age": 30},
                    {"name": "charlie", "age": 35}
                ]
            }"#
        .unindent(),
        &["users", "#1", "age"],
        Some(json!(31)),
        r#"{
                "users": [
                    {"name": "alice", "age": 25},
                    {"name": "bob", "age": 31},
                    {"name": "charlie", "age": 35}
                ]
            }"#
        .unindent(),
    );

    // Add new property to object in array
    check_object_replace_array(
        r#"{
                "items": [
                    {"id": 1},
                    {"id": 2},
                    {"id": 3}
                ]
            }"#
        .unindent(),
        &["items", "#1", "name"],
        Some(json!("Item Two")),
        r#"{
                "items": [
                    {"id": 1},
                    {"name": "Item Two", "id": 2},
                    {"id": 3}
                ]
            }"#
        .unindent(),
    );

    // Remove property from object in array
    check_object_replace_array(
        r#"{
                "items": [
                    {"id": 1, "name": "one"},
                    {"id": 2, "name": "two"},
                    {"id": 3, "name": "three"}
                ]
            }"#
        .unindent(),
        &["items", "#1", "name"],
        None,
        r#"{
                "items": [
                    {"id": 1, "name": "one"},
                    {"id": 2},
                    {"id": 3, "name": "three"}
                ]
            }"#
        .unindent(),
    );

    // Deeply nested: array in object in array
    check_object_replace_array(
        r#"{
                "data": [
                    {
                        "values": [1, 2, 3]
                    },
                    {
                        "values": [4, 5, 6]
                    }
                ]
            }"#
        .unindent(),
        &["data", "#0", "values", "#1"],
        Some(json!(20)),
        r#"{
                "data": [
                    {
                        "values": [1, 20, 3]
                    },
                    {
                        "values": [4, 5, 6]
                    }
                ]
            }"#
        .unindent(),
    );

    // Multiple levels of nesting
    check_object_replace_array(
        r#"{
                "root": {
                    "level1": [
                        {
                            "level2": {
                                "level3": [10, 20, 30]
                            }
                        }
                    ]
                }
            }"#
        .unindent(),
        &["root", "level1", "#0", "level2", "level3", "#2"],
        Some(json!(300)),
        r#"{
                "root": {
                    "level1": [
                        {
                            "level2": {
                                "level3": [10, 20, 300]
                            }
                        }
                    ]
                }
            }"#
        .unindent(),
    );

    // Array with mixed types
    check_object_replace_array(
        r#"{
                "mixed": [1, "two", true, null, {"five": 5}]
            }"#
        .unindent(),
        &["mixed", "#3"],
        Some(json!({"four": 4})),
        r#"{
                "mixed": [1, "two", true, { "four": 4 }, {"five": 5}]
            }"#
        .unindent(),
    );

    // Replace with complex object
    check_object_replace_array(
        r#"{
                "config": [
                    "simple",
                    "values"
                ]
            }"#
        .unindent(),
        &["config", "#0"],
        Some(json!({
            "type": "complex",
            "settings": {
                "enabled": true,
                "level": 5
            }
        })),
        r#"{
                "config": [
                    {
                        "type": "complex",
                        "settings": {
                            "enabled": true,
                            "level": 5
                        }
                    },
                    "values"
                ]
            }"#
        .unindent(),
    );
}
