use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GotoDefinitionKind {
    Symbol,
    Declaration,
    Type,
    Implementation,
}

pub enum FormatTarget {
    Buffers(HashSet<Entity<Buffer>>),
    Ranges(Vec<Range<MultiBufferPoint>>),
}

#[derive(Clone, Debug)]
pub enum JumpData {
    MultiBufferRow {
        row: MultiBufferRow,
        line_offset_from_top: u32,
    },
    MultiBufferPoint {
        anchor: language::Anchor,
        position: Point,
        line_offset_from_top: u32,
    },
}

pub enum MultibufferSelectionMode {
    First,
    All,
}

/// If select range has more than one line, point the cursor to range.start.
pub fn collapse_multiline_range(range: Range<Point>) -> Range<Point> {
    if range.start.row == range.end.row {
        range
    } else {
        range.start..range.start
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RewrapOptions {
    pub override_language_settings: bool,
    pub preserve_existing_whitespace: bool,
    pub line_length: Option<usize>,
}
