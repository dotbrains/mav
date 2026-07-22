use super::*;

pub(super) fn construct_json_value(
    key_path: &[impl AsRef<str>],
    new_value: Option<&serde_json::Value>,
) -> serde_json::Value {
    let mut new_value =
        serde_json::to_value(new_value.unwrap_or(&serde_json::Value::Null)).unwrap();
    for key in key_path.iter().rev() {
        if parse_index_key(key.as_ref()).is_some() {
            new_value = serde_json::json!([new_value]);
        } else {
            new_value = serde_json::json!({ key.as_ref().to_string(): new_value });
        }
    }
    return new_value;
}

fn parse_index_key(index_key: &str) -> Option<usize> {
    index_key.strip_prefix('#')?.parse().ok()
}

pub(super) fn handle_possible_array_value(
    key_node: &tree_sitter::Node,
    value_node: &tree_sitter::Node,
    text: &str,
    remaining_key_path: &[impl AsRef<str>],
    new_value: Option<&Value>,
    replace_key: Option<&str>,
    tab_size: usize,
) -> Option<(Range<usize>, String)> {
    if remaining_key_path.is_empty() {
        return None;
    }
    let key_path = remaining_key_path;
    let index = parse_index_key(key_path[0].as_ref())?;

    let value_is_array = value_node.kind() == TS_ARRAY_KIND;

    let array_str = if value_is_array {
        &text[value_node.byte_range()]
    } else {
        ""
    };

    let (mut replace_range, mut replace_value) = replace_top_level_array_value_in_json_text(
        array_str,
        &key_path[1..],
        new_value,
        replace_key,
        index,
        tab_size,
    );

    if value_is_array {
        replace_range.start += value_node.start_byte();
        replace_range.end += value_node.start_byte();
    } else {
        // replace the full value if it wasn't an array
        replace_range = value_node.byte_range();
    }
    let non_whitespace_char_count = replace_value.len()
        - replace_value
            .chars()
            .filter(char::is_ascii_whitespace)
            .count();
    let needs_indent = replace_value.ends_with('\n')
        || (replace_value
            .chars()
            .zip(replace_value.chars().skip(1))
            .any(|(c, next_c)| c == '\n' && !next_c.is_ascii_whitespace()));
    let contains_comment = (replace_value.contains("//") && replace_value.contains('\n'))
        || (replace_value.contains("/*") && replace_value.contains("*/"));
    if needs_indent {
        let indent_width = key_node.start_position().column;
        let increased_indent = format!("\n{space:width$}", space = ' ', width = indent_width);
        replace_value = replace_value.replace('\n', &increased_indent);
    } else if non_whitespace_char_count < 32 && !contains_comment {
        // remove indentation
        while let Some(idx) = replace_value.find("\n ") {
            replace_value.remove(idx);
        }
        while let Some(idx) = replace_value.find("  ") {
            replace_value.remove(idx);
        }
    }
    return Some((replace_range, replace_value));
}
