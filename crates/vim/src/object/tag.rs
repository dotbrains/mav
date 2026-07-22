use super::*;

pub fn surrounding_html_tag(
    map: &DisplaySnapshot,
    head: DisplayPoint,
    range: Range<DisplayPoint>,
    around: bool,
) -> Option<Range<DisplayPoint>> {
    fn read_tag(chars: impl Iterator<Item = char>) -> String {
        chars
            .take_while(|c| c.is_alphanumeric() || *c == ':' || *c == '-' || *c == '_' || *c == '.')
            .collect()
    }
    fn open_tag(mut chars: impl Iterator<Item = char>) -> Option<String> {
        if Some('<') != chars.next() {
            return None;
        }
        Some(read_tag(chars))
    }
    fn close_tag(mut chars: impl Iterator<Item = char>) -> Option<String> {
        if (Some('<'), Some('/')) != (chars.next(), chars.next()) {
            return None;
        }
        Some(read_tag(chars))
    }

    let snapshot = &map.buffer_snapshot();
    let head_offset = head.to_offset(map, Bias::Left);
    let range_start = range.start.to_offset(map, Bias::Left);
    let range_end = range.end.to_offset(map, Bias::Left);
    let head_is_start = head_offset <= range_start;

    let results = snapshot.map_excerpt_ranges(
        range_start..range_end,
        |buffer, _excerpt_range, input_buffer_range| {
            let buffer_offset = if head_is_start {
                input_buffer_range.start
            } else {
                input_buffer_range.end
            };

            let Some(layer) = buffer.syntax_layer_at(buffer_offset) else {
                return Vec::new();
            };
            let mut cursor = layer.node().walk();
            let mut last_child_node = cursor.node();
            while cursor.goto_first_child_for_byte(buffer_offset.0).is_some() {
                last_child_node = cursor.node();
            }

            let mut last_child_node = Some(last_child_node);
            while let Some(cur_node) = last_child_node {
                if cur_node.child_count() >= 2 {
                    let first_child = cur_node.child(0);
                    let last_child = cur_node.child(cur_node.child_count() as u32 - 1);
                    if let (Some(first_child), Some(last_child)) = (first_child, last_child) {
                        let open_tag = open_tag(buffer.chars_for_range(first_child.byte_range()));
                        let close_tag = close_tag(buffer.chars_for_range(last_child.byte_range()));
                        let is_valid = if range_end.saturating_sub(range_start) <= 1 {
                            buffer_offset.0 <= last_child.end_byte()
                        } else {
                            input_buffer_range.start.0 >= first_child.start_byte()
                                && input_buffer_range.end.0 <= last_child.start_byte() + 1
                        };
                        if open_tag.is_some() && open_tag == close_tag && is_valid {
                            let buffer_range = if around {
                                first_child.byte_range().start..last_child.byte_range().end
                            } else {
                                first_child.byte_range().end..last_child.byte_range().start
                            };
                            return vec![(
                                BufferOffset(buffer_range.start)..BufferOffset(buffer_range.end),
                                (),
                            )];
                        }
                    }
                }
                last_child_node = cur_node.parent();
            }
            Vec::new()
        },
    )?;

    let (result, ()) = results.into_iter().next()?;
    Some(result.start.to_display_point(map)..result.end.to_display_point(map))
}
