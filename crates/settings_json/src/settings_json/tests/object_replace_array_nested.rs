use super::*;

#[test]
fn object_replace_array_nested() {
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

    // Array with trailing comma
    check_object_replace_array(
        r#"{
                "items": [
                    1,
                    2,
                    3,
                ]
            }"#
        .unindent(),
        &["items", "#1"],
        Some(json!(20)),
        r#"{
                "items": [
                    1,
                    20,
                    3,
                ]
            }"#
        .unindent(),
    );

    // Array with comments
    check_object_replace_array(
        r#"{
                "items": [
                    1, // first item
                    2, // second item
                    3  // third item
                ]
            }"#
        .unindent(),
        &["items", "#1"],
        Some(json!(20)),
        r#"{
                "items": [
                    1, // first item
                    20, // second item
                    3  // third item
                ]
            }"#
        .unindent(),
    );

    // Multiple arrays in object
    check_object_replace_array(
        r#"{
                "first": [1, 2, 3],
                "second": [4, 5, 6],
                "third": [7, 8, 9]
            }"#
        .unindent(),
        &["second", "#1"],
        Some(json!(50)),
        r#"{
                "first": [1, 2, 3],
                "second": [4, 50, 6],
                "third": [7, 8, 9]
            }"#
        .unindent(),
    );

    // Empty array - add first element
    check_object_replace_array(
        r#"{
                "empty": []
            }"#
        .unindent(),
        &["empty", "#0"],
        Some(json!("first")),
        r#"{
                "empty": ["first"]
            }"#
        .unindent(),
    );

    // Array of arrays
    check_object_replace_array(
        r#"{
                "matrix": [
                    [1, 2],
                    [3, 4],
                    [5, 6]
                ]
            }"#
        .unindent(),
        &["matrix", "#1", "#0"],
        Some(json!(30)),
        r#"{
                "matrix": [
                    [1, 2],
                    [30, 4],
                    [5, 6]
                ]
            }"#
        .unindent(),
    );

    // Replace nested object property in array element
    check_object_replace_array(
        r#"{
                "users": [
                    {
                        "name": "alice",
                        "address": {
                            "city": "NYC",
                            "zip": "10001"
                        }
                    }
                ]
            }"#
        .unindent(),
        &["users", "#0", "address", "city"],
        Some(json!("Boston")),
        r#"{
                "users": [
                    {
                        "name": "alice",
                        "address": {
                            "city": "Boston",
                            "zip": "10001"
                        }
                    }
                ]
            }"#
        .unindent(),
    );

    // Add element past end of array
    check_object_replace_array(
        r#"{
                "items": [1, 2]
            }"#
        .unindent(),
        &["items", "#5"],
        Some(json!(6)),
        r#"{
                "items": [1, 2, 6]
            }"#
        .unindent(),
    );

    // Complex nested structure
    check_object_replace_array(
        r#"{
                "app": {
                    "modules": [
                        {
                            "name": "auth",
                            "routes": [
                                {"path": "/login", "method": "POST"},
                                {"path": "/logout", "method": "POST"}
                            ]
                        },
                        {
                            "name": "api",
                            "routes": [
                                {"path": "/users", "method": "GET"},
                                {"path": "/users", "method": "POST"}
                            ]
                        }
                    ]
                }
            }"#
        .unindent(),
        &["app", "modules", "#1", "routes", "#0", "method"],
        Some(json!("PUT")),
        r#"{
                "app": {
                    "modules": [
                        {
                            "name": "auth",
                            "routes": [
                                {"path": "/login", "method": "POST"},
                                {"path": "/logout", "method": "POST"}
                            ]
                        },
                        {
                            "name": "api",
                            "routes": [
                                {"path": "/users", "method": "PUT"},
                                {"path": "/users", "method": "POST"}
                            ]
                        }
                    ]
                }
            }"#
        .unindent(),
    );

    // Escaped strings in array
    check_object_replace_array(
        r#"{
                "messages": ["hello", "world"]
            }"#
        .unindent(),
        &["messages", "#0"],
        Some(json!("hello \"quoted\" world")),
        r#"{
                "messages": ["hello \"quoted\" world", "world"]
            }"#
        .unindent(),
    );

    // Block comments
    check_object_replace_array(
        r#"{
                "data": [
                    /* first */ 1,
                    /* second */ 2,
                    /* third */ 3
                ]
            }"#
        .unindent(),
        &["data", "#1"],
        Some(json!(20)),
        r#"{
                "data": [
                    /* first */ 1,
                    /* second */ 20,
                    /* third */ 3
                ]
            }"#
        .unindent(),
    );

    // Inline array
    check_object_replace_array(
        r#"{"items": [1, 2, 3], "count": 3}"#.to_string(),
        &["items", "#1"],
        Some(json!(20)),
        r#"{"items": [1, 20, 3], "count": 3}"#.to_string(),
    );

    // Single element array
    check_object_replace_array(
        r#"{
                "single": [42]
            }"#
        .unindent(),
        &["single", "#0"],
        Some(json!(100)),
        r#"{
                "single": [100]
            }"#
        .unindent(),
    );

    // Inconsistent formatting
    check_object_replace_array(
        r#"{
                "messy": [1,
                    2,
                        3,
                4]
            }"#
        .unindent(),
        &["messy", "#2"],
        Some(json!(30)),
        r#"{
                "messy": [1,
                    2,
                        30,
                4]
            }"#
        .unindent(),
    );

    // Creates array if has numbered key
    check_object_replace_array(
        r#"{
                "array": {"foo": "bar"}
            }"#
        .unindent(),
        &["array", "#3"],
        Some(json!(4)),
        r#"{
                "array": [
                    4
                ]
            }"#
        .unindent(),
    );

    // Replace non-array element within array with array
    check_object_replace_array(
        r#"{
                "matrix": [
                    [1, 2],
                    [3, 4],
                    [5, 6]
                ]
            }"#
        .unindent(),
        &["matrix", "#1", "#0"],
        Some(json!(["foo", "bar"])),
        r#"{
                "matrix": [
                    [1, 2],
                    [[ "foo", "bar" ], 4],
                    [5, 6]
                ]
            }"#
        .unindent(),
    );
    // Replace non-array element within array with array
    check_object_replace_array(
        r#"{
                "matrix": [
                    [1, 2],
                    [3, 4],
                    [5, 6]
                ]
            }"#
        .unindent(),
        &["matrix", "#1", "#0", "#3"],
        Some(json!(["foo", "bar"])),
        r#"{
                "matrix": [
                    [1, 2],
                    [[ [ "foo", "bar" ] ], 4],
                    [5, 6]
                ]
            }"#
        .unindent(),
    );

    // Create array in key that doesn't exist
    check_object_replace_array(
        r#"{
                "foo": {}
            }"#
        .unindent(),
        &["foo", "bar", "#0"],
        Some(json!({"is_object": true})),
        r#"{
                "foo": {
                    "bar": [
                        {
                            "is_object": true
                        }
                    ]
                }
            }"#
        .unindent(),
    );
}
