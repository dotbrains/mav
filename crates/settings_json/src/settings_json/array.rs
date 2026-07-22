use super::helpers::{construct_json_value, handle_possible_array_value};
use super::*;

pub fn replace_top_level_array_value_in_json_text(
    text: &str,
    key_path: &[impl AsRef<str>],
    new_value: Option<&Value>,
    replace_key: Option<&str>,
    array_index: usize,
    tab_size: usize,
) -> (Range<usize>, String) {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_json::LANGUAGE.into())
        .unwrap();

    let syntax_tree = parser.parse(text, None).unwrap();

    let mut cursor = syntax_tree.walk();

    if cursor.node().kind() == TS_DOCUMENT_KIND {
        cursor.goto_first_child();
    }

    while cursor.node().kind() != TS_ARRAY_KIND {
        if !cursor.goto_next_sibling() {
            let json_value = construct_json_value(key_path, new_value);
            let json_value = serde_json::json!([json_value]);
            return (0..text.len(), to_pretty_json(&json_value, tab_size, 0));
        }
    }

    // false if no children
    //
    cursor.goto_first_child();
    debug_assert_eq!(cursor.node().kind(), "[");

    let mut index = 0;

    while index <= array_index {
        let node = cursor.node();
        if !matches!(node.kind(), "[" | "]" | TS_COMMENT_KIND | ",")
            && !node.is_extra()
            && !node.is_missing()
        {
            if index == array_index {
                break;
            }
            index += 1;
        }
        if !cursor.goto_next_sibling() {
            if let Some(new_value) = new_value {
                return append_top_level_array_value_in_json_text(text, new_value, tab_size);
            } else {
                return (0..0, String::new());
            }
        }
    }

    let range = cursor.node().range();
    let indent_width = range.start_point.column;
    let offset = range.start_byte;
    let text_range = range.start_byte..range.end_byte;
    let value_str = &text[text_range.clone()];
    let needs_indent = range.start_point.row > 0;

    if new_value.is_none() && key_path.is_empty() {
        let mut remove_range = text_range;
        if index == 0 {
            while cursor.goto_next_sibling()
                && (cursor.node().is_extra() || cursor.node().is_missing())
            {}
            if cursor.node().kind() == "," {
                remove_range.end = cursor.node().range().end_byte;
            }
            if let Some(next_newline) = &text[remove_range.end + 1..].find('\n')
                && text[remove_range.end + 1..remove_range.end + next_newline]
                    .chars()
                    .all(|c| c.is_ascii_whitespace())
            {
                remove_range.end = remove_range.end + next_newline;
            }
        } else {
            while cursor.goto_previous_sibling()
                && (cursor.node().is_extra() || cursor.node().is_missing())
            {}
            if cursor.node().kind() == "," {
                remove_range.start = cursor.node().range().start_byte;
            }
        }
        (remove_range, String::new())
    } else {
        if let Some(array_replacement) = handle_possible_array_value(
            &cursor.node(),
            &cursor.node(),
            text,
            key_path,
            new_value,
            replace_key,
            tab_size,
        ) {
            return array_replacement;
        }
        let (mut replace_range, mut replace_value) =
            replace_value_in_json_text(value_str, key_path, tab_size, new_value, replace_key);

        replace_range.start += offset;
        replace_range.end += offset;

        if needs_indent {
            let increased_indent = format!("\n{space:width$}", space = ' ', width = indent_width);
            replace_value = replace_value.replace('\n', &increased_indent);
        } else {
            while let Some(idx) = replace_value.find("\n ") {
                replace_value.remove(idx + 1);
            }
            while let Some(idx) = replace_value.find("\n") {
                replace_value.replace_range(idx..idx + 1, " ");
            }
        }

        (replace_range, replace_value)
    }
}

pub fn append_top_level_array_value_in_json_text(
    text: &str,
    new_value: &Value,
    tab_size: usize,
) -> (Range<usize>, String) {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_json::LANGUAGE.into())
        .unwrap();
    let syntax_tree = parser.parse(text, None).unwrap();

    let mut cursor = syntax_tree.walk();

    if cursor.node().kind() == TS_DOCUMENT_KIND {
        cursor.goto_first_child();
    }

    while cursor.node().kind() != TS_ARRAY_KIND {
        if !cursor.goto_next_sibling() {
            let json_value = serde_json::json!([new_value]);
            return (0..text.len(), to_pretty_json(&json_value, tab_size, 0));
        }
    }

    let went_to_last_child = cursor.goto_last_child();
    debug_assert!(
        went_to_last_child && cursor.node().kind() == "]",
        "Malformed JSON syntax tree, expected `]` at end of array"
    );
    let close_bracket_start = cursor.node().start_byte();
    while cursor.goto_previous_sibling()
        && (cursor.node().is_extra() || cursor.node().is_missing())
        && !cursor.node().is_error()
    {}

    let mut comma_range = None;
    let mut prev_item_range = None;

    if cursor.node().kind() == "," || is_error_of_kind(&mut cursor, ",") {
        comma_range = Some(cursor.node().byte_range());
        while cursor.goto_previous_sibling()
            && (cursor.node().is_extra() || cursor.node().is_missing())
        {}

        debug_assert_ne!(cursor.node().kind(), "[");
        prev_item_range = Some(cursor.node().range());
    } else {
        while (cursor.node().is_extra() || cursor.node().is_missing())
            && cursor.goto_previous_sibling()
        {}
        if cursor.node().kind() != "[" {
            prev_item_range = Some(cursor.node().range());
        }
    }

    let (mut replace_range, mut replace_value) =
        replace_value_in_json_text::<&str>("", &[], tab_size, Some(new_value), None);

    replace_range.start = close_bracket_start;
    replace_range.end = close_bracket_start;

    let space = ' ';
    if let Some(prev_item_range) = prev_item_range {
        let needs_newline = prev_item_range.start_point.row > 0;
        let indent_width = text[..prev_item_range.start_byte].rfind('\n').map_or(
            prev_item_range.start_point.column,
            |idx| {
                prev_item_range.start_point.column
                    - text[idx + 1..prev_item_range.start_byte].trim_start().len()
            },
        );

        let prev_item_end = comma_range
            .as_ref()
            .map_or(prev_item_range.end_byte, |range| range.end);
        if text[prev_item_end..replace_range.start].trim().is_empty() {
            replace_range.start = prev_item_end;
        }

        if needs_newline {
            let increased_indent = format!("\n{space:width$}", width = indent_width);
            replace_value = replace_value.replace('\n', &increased_indent);
            replace_value.push('\n');
            replace_value.insert_str(0, &format!("\n{space:width$}", width = indent_width));
        } else {
            while let Some(idx) = replace_value.find("\n ") {
                replace_value.remove(idx + 1);
            }
            while let Some(idx) = replace_value.find('\n') {
                replace_value.replace_range(idx..idx + 1, " ");
            }
            replace_value.insert(0, ' ');
        }

        if comma_range.is_none() {
            replace_value.insert(0, ',');
        }
    } else if replace_value.contains('\n') || text.contains('\n') {
        if let Some(prev_newline) = text[..replace_range.start].rfind('\n')
            && text[prev_newline..replace_range.start].trim().is_empty()
        {
            replace_range.start = prev_newline;
        }
        let indent = format!("\n{space:width$}", width = tab_size);
        replace_value = replace_value.replace('\n', &indent);
        replace_value.insert_str(0, &indent);
        replace_value.push('\n');
    }
    return (replace_range, replace_value);

    fn is_error_of_kind(cursor: &mut tree_sitter::TreeCursor<'_>, kind: &str) -> bool {
        if cursor.node().kind() != "ERROR" {
            return false;
        }

        let descendant_index = cursor.descendant_index();
        let res = cursor.goto_first_child() && cursor.node().kind() == kind;
        cursor.goto_descendant(descendant_index);
        res
    }
}
