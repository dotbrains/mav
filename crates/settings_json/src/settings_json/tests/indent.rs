use super::*;

#[test]
fn test_infer_json_indent_size() {
    let json_2_spaces = r#"{
  "key1": "value1",
  "nested": {
    "key2": "value2",
    "array": [
      1,
      2,
      3
    ]
  }
}"#;
    assert_eq!(infer_json_indent_size(json_2_spaces), 2);

    let json_4_spaces = r#"{
    "key1": "value1",
    "nested": {
        "key2": "value2",
        "array": [
            1,
            2,
            3
        ]
    }
}"#;
    assert_eq!(infer_json_indent_size(json_4_spaces), 4);

    let json_8_spaces = r#"{
        "key1": "value1",
        "nested": {
                "key2": "value2"
        }
}"#;
    assert_eq!(infer_json_indent_size(json_8_spaces), 8);

    let json_single_line = r#"{"key": "value", "nested": {"inner": "data"}}"#;
    assert_eq!(infer_json_indent_size(json_single_line), 2);

    let json_empty = r#"{}"#;
    assert_eq!(infer_json_indent_size(json_empty), 2);

    let json_array = r#"[
  {
    "id": 1,
    "name": "first"
  },
  {
    "id": 2,
    "name": "second"
  }
]"#;
    assert_eq!(infer_json_indent_size(json_array), 2);

    let json_mixed = r#"{
  "a": {
    "b": {
        "c": "value"
    }
  },
  "d": "value2"
}"#;
    assert_eq!(infer_json_indent_size(json_mixed), 2);
}
