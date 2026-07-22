use collections::HashSet;
use dap::inline_value::{InlineValueLocation, VariableLookupKind, VariableScope};
use language;
use std::ops::Range;
use text::Point;

pub(super) fn provide_inline_values(
    captures: impl Iterator<Item = (Range<usize>, language::DebuggerTextObject)>,
    snapshot: &language::BufferSnapshot,
    max_row: usize,
) -> Vec<InlineValueLocation> {
    let mut variables = Vec::new();
    let mut variable_position = HashSet::default();
    let mut scopes = Vec::new();

    let active_debug_line_offset = snapshot.point_to_offset(Point::new(max_row as u32, 0));

    for (capture_range, capture_kind) in captures {
        match capture_kind {
            language::DebuggerTextObject::Variable => {
                let variable_name = snapshot
                    .text_for_range(capture_range.clone())
                    .collect::<String>();
                let point = snapshot.offset_to_point(capture_range.end);

                while scopes
                    .last()
                    .is_some_and(|scope: &Range<_>| !scope.contains(&capture_range.start))
                {
                    scopes.pop();
                }

                if point.row as usize > max_row {
                    break;
                }

                let scope = if scopes
                    .last()
                    .is_none_or(|scope| !scope.contains(&active_debug_line_offset))
                {
                    VariableScope::Global
                } else {
                    VariableScope::Local
                };

                if variable_position.insert(capture_range.end) {
                    variables.push(InlineValueLocation {
                        variable_name,
                        scope,
                        lookup: VariableLookupKind::Variable,
                        row: point.row as usize,
                        column: point.column as usize,
                    });
                }
            }
            language::DebuggerTextObject::Scope => {
                while scopes.last().map_or_else(
                    || false,
                    |scope: &Range<usize>| {
                        !(scope.contains(&capture_range.start)
                            && scope.contains(&capture_range.end))
                    },
                ) {
                    scopes.pop();
                }
                scopes.push(capture_range);
            }
        }
    }

    variables
}
