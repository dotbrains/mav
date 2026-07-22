use super::helpers::{construct_json_value, handle_possible_array_value};
use super::*;

pub fn replace_value_in_json_text<T: AsRef<str>>(
    text: &str,
    key_path: &[T],
    tab_size: usize,
    new_value: Option<&Value>,
    replace_key: Option<&str>,
) -> (Range<usize>, String) {
    static PAIR_QUERY: LazyLock<Query> = LazyLock::new(|| {
        Query::new(
            &tree_sitter_json::LANGUAGE.into(),
            "(pair key: (string) @key value: (_) @value)",
        )
        .expect("Failed to create PAIR_QUERY")
    });

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_json::LANGUAGE.into())
        .unwrap();
    let syntax_tree = parser.parse(text, None).unwrap();

    let mut cursor = tree_sitter::QueryCursor::new();

    let mut depth = 0;
    let mut last_value_range = 0..0;
    let mut first_key_start = None;
    let mut existing_value_range = 0..text.len();

    let mut matches = cursor.matches(&PAIR_QUERY, syntax_tree.root_node(), text.as_bytes());
    while let Some(mat) = matches.next() {
        if mat.captures.len() != 2 {
            continue;
        }

        let key_range = mat.captures[0].node.byte_range();
        let value_range = mat.captures[1].node.byte_range();

        // Don't enter sub objects until we find an exact
        // match for the current keypath
        if last_value_range.contains_inclusive(&value_range) {
            continue;
        }

        last_value_range = value_range.clone();

        if key_range.start > existing_value_range.end {
            break;
        }

        first_key_start.get_or_insert(key_range.start);

        let found_key = text
            .get(key_range.clone())
            .zip(key_path.get(depth))
            .and_then(|(key_text, key_path_value)| {
                serde_json::to_string(key_path_value.as_ref())
                    .ok()
                    .map(|key_path| depth < key_path.len() && key_text == key_path)
            })
            .unwrap_or(false);

        if found_key {
            existing_value_range = value_range;
            // Reset last value range when increasing in depth
            last_value_range = existing_value_range.start..existing_value_range.start;
            depth += 1;

            if depth == key_path.len() {
                break;
            }

            if let Some(array_replacement) = handle_possible_array_value(
                &mat.captures[0].node,
                &mat.captures[1].node,
                text,
                &key_path[depth..],
                new_value,
                replace_key,
                tab_size,
            ) {
                return array_replacement;
            }

            first_key_start = None;
        }
    }

    // We found the exact key we want
    if depth == key_path.len() {
        if let Some(new_value) = new_value {
            let new_val = to_pretty_json(new_value, tab_size, tab_size * depth);
            if let Some(replace_key) = replace_key.and_then(|str| serde_json::to_string(str).ok()) {
                let new_key = format!("{}: ", replace_key);
                if let Some(key_start) = text[..existing_value_range.start].rfind('"') {
                    if let Some(prev_key_start) = text[..key_start].rfind('"') {
                        existing_value_range.start = prev_key_start;
                    } else {
                        existing_value_range.start = key_start;
                    }
                }
                (existing_value_range, new_key + &new_val)
            } else {
                (existing_value_range, new_val)
            }
        } else {
            let mut removal_start = first_key_start.unwrap_or(existing_value_range.start);
            let mut removal_end = existing_value_range.end;

            // Find the actual key position by looking for the key in the pair
            // We need to extend the range to include the key, not just the value
            if let Some(key_start) = text[..existing_value_range.start].rfind('"') {
                if let Some(prev_key_start) = text[..key_start].rfind('"') {
                    removal_start = prev_key_start;
                } else {
                    removal_start = key_start;
                }
            }

            let mut removed_comma = false;
            // Look backward for a preceding comma first
            let preceding_text = text.get(0..removal_start).unwrap_or("");
            if let Some(comma_pos) = preceding_text.rfind(',') {
                // Check if there are only whitespace characters between the comma and our key
                let between_comma_and_key = text.get(comma_pos + 1..removal_start).unwrap_or("");
                if between_comma_and_key.trim().is_empty() {
                    removal_start = comma_pos;
                    removed_comma = true;
                }
            }
            if let Some(remaining_text) = text.get(existing_value_range.end..)
                && !removed_comma
            {
                let mut chars = remaining_text.char_indices();
                while let Some((offset, ch)) = chars.next() {
                    if ch == ',' {
                        removal_end = existing_value_range.end + offset + 1;
                        // Also consume whitespace after the comma
                        for (_, next_ch) in chars.by_ref() {
                            if next_ch.is_whitespace() {
                                removal_end += next_ch.len_utf8();
                            } else {
                                break;
                            }
                        }
                        break;
                    } else if !ch.is_whitespace() {
                        break;
                    }
                }
            }
            (removal_start..removal_end, String::new())
        }
    } else {
        if let Some(first_key_start) = first_key_start {
            // We have key paths, construct the sub objects
            let new_key = key_path[depth].as_ref();
            // We don't have the key, construct the nested objects
            let new_value = construct_json_value(&key_path[(depth + 1)..], new_value);

            let mut row = 0;
            let mut column = 0;
            for (ix, char) in text.char_indices() {
                if ix == first_key_start {
                    break;
                }
                if char == '\n' {
                    row += 1;
                    column = 0;
                } else {
                    column += char.len_utf8();
                }
            }

            if row > 0 {
                // depth is 0 based, but division needs to be 1 based.
                let new_val = to_pretty_json(&new_value, column / (depth + 1), column);
                let space = ' ';
                let content = format!("\"{new_key}\": {new_val},\n{space:width$}", width = column);
                (first_key_start..first_key_start, content)
            } else {
                let new_val = serde_json::to_string(&new_value).unwrap();
                let mut content = format!(r#""{new_key}": {new_val},"#);
                content.push(' ');
                (first_key_start..first_key_start, content)
            }
        } else {
            // We don't have the key, construct the nested objects
            let new_value = construct_json_value(&key_path[depth..], new_value);
            let indent_prefix_len = tab_size * depth;
            let mut new_val = to_pretty_json(&new_value, tab_size, indent_prefix_len);
            if depth == 0 {
                new_val.push('\n');
            }
            // best effort to keep comments with best effort indentation
            let mut replace_text = &text[existing_value_range.clone()];
            while let Some(comment_start) = replace_text.rfind("//") {
                if let Some(comment_end) = replace_text[comment_start..].find('\n') {
                    let mut comment_with_indent_start = replace_text[..comment_start]
                        .rfind('\n')
                        .unwrap_or(comment_start);
                    if !replace_text[comment_with_indent_start..comment_start]
                        .trim()
                        .is_empty()
                    {
                        comment_with_indent_start = comment_start;
                    }
                    new_val.insert_str(
                        1,
                        &replace_text[comment_with_indent_start..comment_start + comment_end],
                    );
                }
                replace_text = &replace_text[..comment_start];
            }

            (existing_value_range, new_val)
        }
    }
}
