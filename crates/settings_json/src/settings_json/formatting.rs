use super::*;

pub fn infer_json_indent_size(text: &str) -> usize {
    const MAX_INDENT_SIZE: usize = 64;

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_json::LANGUAGE.into())
        .unwrap();

    let Some(syntax_tree) = parser.parse(text, None) else {
        return 4;
    };

    let mut cursor = syntax_tree.walk();
    let mut indent_counts = [0u32; MAX_INDENT_SIZE];

    // Traverse the tree to find indentation patterns
    fn visit_node(
        cursor: &mut tree_sitter::TreeCursor,
        indent_counts: &mut [u32; MAX_INDENT_SIZE],
        depth: usize,
    ) {
        if depth >= 3 {
            return;
        }
        let node = cursor.node();
        let node_kind = node.kind();

        // For objects and arrays, check the indentation of their first content child
        if matches!(node_kind, "object" | "array") {
            let container_column = node.start_position().column;
            let container_row = node.start_position().row;

            if cursor.goto_first_child() {
                // Skip the opening bracket
                loop {
                    let child = cursor.node();
                    let child_kind = child.kind();

                    // Look for the first actual content (pair for objects, value for arrays)
                    if (node_kind == "object" && child_kind == "pair")
                        || (node_kind == "array"
                            && !matches!(child_kind, "[" | "]" | "," | "comment"))
                    {
                        let child_column = child.start_position().column;
                        let child_row = child.start_position().row;

                        // Only count if the child is on a different line
                        if child_row > container_row && child_column > container_column {
                            let indent = child_column - container_column;
                            if indent > 0 && indent < MAX_INDENT_SIZE {
                                indent_counts[indent] += 1;
                            }
                        }
                        break;
                    }

                    if !cursor.goto_next_sibling() {
                        break;
                    }
                }
                cursor.goto_parent();
            }
        }

        // Recurse to children
        if cursor.goto_first_child() {
            loop {
                visit_node(cursor, indent_counts, depth + 1);
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
            cursor.goto_parent();
        }
    }

    visit_node(&mut cursor, &mut indent_counts, 0);

    // Find the indent size with the highest count
    let mut max_count = 0;
    let mut max_indent = 4;

    for (indent, &count) in indent_counts.iter().enumerate() {
        if count > max_count {
            max_count = count;
            max_indent = indent;
        }
    }

    if max_count == 0 { 2 } else { max_indent }
}

pub fn to_pretty_json(
    value: &impl Serialize,
    indent_size: usize,
    indent_prefix_len: usize,
) -> String {
    let mut output = Vec::new();
    let indent = " ".repeat(indent_size);
    let mut ser = serde_json::Serializer::with_formatter(
        &mut output,
        serde_json::ser::PrettyFormatter::with_indent(indent.as_bytes()),
    );

    value.serialize(&mut ser).unwrap();
    let text = String::from_utf8(output).unwrap();

    let mut adjusted_text = String::new();
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            adjusted_text.extend(std::iter::repeat(' ').take(indent_prefix_len));
        }
        adjusted_text.push_str(line);
        adjusted_text.push('\n');
    }
    adjusted_text.pop();
    adjusted_text
}

pub fn parse_json_with_comments<T: DeserializeOwned>(content: &str) -> Result<T> {
    let mut deserializer = serde_json_lenient::Deserializer::from_str(content);
    Ok(serde_path_to_error::deserialize(&mut deserializer)?)
}
