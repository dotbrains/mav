use super::*;

#[test]
fn test_extract_subtest_name() {
    // Interpreted string literal
    let input_double_quoted = r#""subtest with double quotes""#;
    let result = extract_subtest_name(input_double_quoted);
    assert_eq!(result, Some(r#"subtest_with_double_quotes"#.to_string()));

    let input_double_quoted_with_backticks = r#""test with `backticks` inside""#;
    let result = extract_subtest_name(input_double_quoted_with_backticks);
    assert_eq!(result, Some(r#"test_with_`backticks`_inside"#.to_string()));

    // Raw string literal
    let input_with_backticks = r#"`subtest with backticks`"#;
    let result = extract_subtest_name(input_with_backticks);
    assert_eq!(result, Some(r#"subtest_with_backticks"#.to_string()));

    let input_raw_with_quotes = r#"`test with "quotes" and other chars`"#;
    let result = extract_subtest_name(input_raw_with_quotes);
    assert_eq!(
        result,
        Some(r#"test_with_\"quotes\"_and_other_chars"#.to_string())
    );

    let input_multiline = r#"`subtest with
    multiline
    backticks`"#;
    let result = extract_subtest_name(input_multiline);
    assert_eq!(
        result,
        Some(r#"subtest_with_________multiline_________backticks"#.to_string())
    );

    let input_with_double_quotes = r#"`test with "double quotes"`"#;
    let result = extract_subtest_name(input_with_double_quotes);
    assert_eq!(result, Some(r#"test_with_\"double_quotes\""#.to_string()));
}
