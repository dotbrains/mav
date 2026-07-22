use super::*;

#[test]
fn test_convert_ruff_schema() {
    use super::RuffLspAdapter;

    let raw_schema = serde_json::json!({
        "line-length": {
            "doc": "The line length to use when enforcing long-lines violations",
            "default": "88",
            "value_type": "int",
            "scope": null,
            "example": "line-length = 120",
            "deprecated": null
        },
        "lint.select": {
            "doc": "A list of rule codes or prefixes to enable",
            "default": "[\"E4\", \"E7\", \"E9\", \"F\"]",
            "value_type": "list[RuleSelector]",
            "scope": null,
            "example": "select = [\"E4\", \"E7\", \"E9\", \"F\", \"B\", \"Q\"]",
            "deprecated": null
        },
        "lint.isort.case-sensitive": {
            "doc": "Sort imports taking into account case sensitivity.",
            "default": "false",
            "value_type": "bool",
            "scope": null,
            "example": "case-sensitive = true",
            "deprecated": null
        },
        "format.quote-style": {
            "doc": "Configures the preferred quote character for strings.",
            "default": "\"double\"",
            "value_type": "\"double\" | \"single\" | \"preserve\"",
            "scope": null,
            "example": "quote-style = \"single\"",
            "deprecated": null
        }
    });

    let converted = RuffLspAdapter::convert_ruff_schema(&raw_schema);

    assert!(converted.is_object());
    assert_eq!(
        converted.get("type").and_then(|v| v.as_str()),
        Some("object")
    );

    let properties = converted
        .get("properties")
        .expect("should have properties")
        .as_object()
        .expect("properties should be an object");

    assert!(properties.contains_key("line-length"));
    assert!(properties.contains_key("lint"));
    assert!(properties.contains_key("format"));

    let line_length = properties
        .get("line-length")
        .expect("should have line-length")
        .as_object()
        .expect("line-length should be an object");

    assert_eq!(
        line_length.get("type").and_then(|v| v.as_str()),
        Some("integer")
    );
    assert_eq!(
        line_length.get("default").and_then(|v| v.as_str()),
        Some("88")
    );

    let lint = properties
        .get("lint")
        .expect("should have lint")
        .as_object()
        .expect("lint should be an object");

    let lint_props = lint
        .get("properties")
        .expect("lint should have properties")
        .as_object()
        .expect("lint properties should be an object");

    assert!(lint_props.contains_key("select"));
    assert!(lint_props.contains_key("isort"));

    let select = lint_props.get("select").expect("should have select");
    assert_eq!(select.get("type").and_then(|v| v.as_str()), Some("array"));

    let isort = lint_props
        .get("isort")
        .expect("should have isort")
        .as_object()
        .expect("isort should be an object");

    let isort_props = isort
        .get("properties")
        .expect("isort should have properties")
        .as_object()
        .expect("isort properties should be an object");

    let case_sensitive = isort_props
        .get("case-sensitive")
        .expect("should have case-sensitive");

    assert_eq!(
        case_sensitive.get("type").and_then(|v| v.as_str()),
        Some("boolean")
    );
    assert!(case_sensitive.get("markdownDescription").is_some());

    let format = properties
        .get("format")
        .expect("should have format")
        .as_object()
        .expect("format should be an object");

    let format_props = format
        .get("properties")
        .expect("format should have properties")
        .as_object()
        .expect("format properties should be an object");

    let quote_style = format_props
        .get("quote-style")
        .expect("should have quote-style");

    assert_eq!(
        quote_style.get("type").and_then(|v| v.as_str()),
        Some("string")
    );

    let enum_values = quote_style
        .get("enum")
        .expect("should have enum")
        .as_array()
        .expect("enum should be an array");

    assert_eq!(enum_values.len(), 3);
    assert!(enum_values.contains(&serde_json::json!("double")));
    assert!(enum_values.contains(&serde_json::json!("single")));
    assert!(enum_values.contains(&serde_json::json!("preserve")));
}
