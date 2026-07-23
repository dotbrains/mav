use super::*;

pub struct LayoutState {
    pub(crate) hitbox: Hitbox,
    pub(crate) batched_text_runs: Vec<BatchedTextRun>,
    pub(crate) rects: Vec<LayoutRect>,
    pub(crate) relative_highlighted_ranges: Vec<(Range, Hsla)>,
    pub(crate) cursor: Option<CursorLayout>,
    pub(crate) ime_cursor_bounds: Option<Bounds<Pixels>>,
    pub(crate) background_color: Hsla,
    pub(crate) dimensions: TerminalBounds,
    pub(crate) mode: Modes,
    pub(crate) display_offset: usize,
    pub(crate) hyperlink_tooltip: Option<AnyElement>,
    pub(crate) block_below_cursor_element: Option<AnyElement>,
    pub(crate) base_text_style: TextStyle,
    pub(crate) content_mode: ContentMode,
}

/// Helper struct for converting terminal cursor points to displayed cursor points.
#[derive(Copy, Clone)]
pub(crate) struct DisplayCursor {
    pub(crate) line: i32,
    pub(crate) col: usize,
}

impl DisplayCursor {
    pub(crate) fn from(cursor_point: Point, display_offset: usize) -> Self {
        Self {
            line: cursor_point.line + display_offset as i32,
            col: cursor_point.column,
        }
    }

    pub fn line(&self) -> i32 {
        self.line
    }

    pub fn col(&self) -> usize {
        self.col
    }
}

#[derive(Copy, Clone, Debug, Default)]
pub struct LayoutPoint {
    pub(crate) line: i32,
    pub(crate) column: i32,
}

impl LayoutPoint {
    pub(crate) fn new(line: i32, column: i32) -> Self {
        Self { line, column }
    }

    pub fn line(&self) -> i32 {
        self.line
    }

    pub fn column(&self) -> i32 {
        self.column
    }
}

/// A batched text run that combines multiple adjacent cells with the same style
#[derive(Debug)]
pub struct BatchedTextRun {
    pub start_point: LayoutPoint,
    pub text: String,
    pub cell_count: usize,
    pub style: TextRun,
    pub font_size: AbsoluteLength,
}

impl BatchedTextRun {
    pub(super) fn new_from_char(
        start_point: LayoutPoint,
        c: char,
        style: TextRun,
        font_size: AbsoluteLength,
    ) -> Self {
        let mut text = String::with_capacity(100); // Pre-allocate for typical line length
        text.push(c);
        BatchedTextRun {
            start_point,
            text,
            cell_count: 1,
            style,
            font_size,
        }
    }

    pub(super) fn can_append(&self, other_style: &TextRun) -> bool {
        self.style.font == other_style.font
            && self.style.color == other_style.color
            && self.style.background_color == other_style.background_color
            && self.style.underline == other_style.underline
            && self.style.strikethrough == other_style.strikethrough
    }

    pub(super) fn append_char(&mut self, c: char) {
        self.append_char_internal(c, true);
    }

    pub(super) fn append_zero_width_chars(&mut self, chars: &[char]) {
        for &c in chars {
            self.append_char_internal(c, false);
        }
    }

    fn append_char_internal(&mut self, c: char, counts_cell: bool) {
        self.text.push(c);
        if counts_cell {
            self.cell_count += 1;
        }
        self.style.len += c.len_utf8();
    }

    pub fn paint(
        &self,
        origin: GpuiPoint<Pixels>,
        dimensions: &TerminalBounds,
        window: &mut Window,
        cx: &mut App,
    ) {
        let pos = GpuiPoint::new(
            origin.x + self.start_point.column as f32 * dimensions.cell_width,
            origin.y + self.start_point.line as f32 * dimensions.line_height,
        );

        window
            .text_system()
            .shape_line(
                self.text.clone().into(),
                self.font_size.to_pixels(window.rem_size()),
                std::slice::from_ref(&self.style),
                Some(dimensions.cell_width),
            )
            .paint(
                pos,
                dimensions.line_height,
                gpui::TextAlign::Left,
                None,
                window,
                cx,
            )
            .log_err();
    }
}

#[derive(Clone, Debug, Default)]
pub struct LayoutRect {
    point: LayoutPoint,
    num_of_cells: usize,
    color: Hsla,
}

impl LayoutRect {
    pub(crate) fn new(point: LayoutPoint, num_of_cells: usize, color: Hsla) -> LayoutRect {
        LayoutRect {
            point,
            num_of_cells,
            color,
        }
    }

    pub fn paint(
        &self,
        origin: GpuiPoint<Pixels>,
        dimensions: &TerminalBounds,
        window: &mut Window,
    ) {
        let position = {
            let layout_point = self.point;
            point(
                (origin.x + layout_point.column as f32 * dimensions.cell_width).floor(),
                origin.y + layout_point.line as f32 * dimensions.line_height,
            )
        };
        let size = point(
            (dimensions.cell_width * self.num_of_cells as f32).ceil(),
            dimensions.line_height,
        )
        .into();

        window.paint_quad(fill(Bounds::new(position, size), self.color));
    }
}

/// Represents a rectangular region with a specific background color
#[derive(Debug, Clone)]
pub(crate) struct BackgroundRegion {
    pub(crate) start_line: i32,
    pub(crate) start_col: i32,
    pub(crate) end_line: i32,
    pub(crate) end_col: i32,
    pub(crate) color: Hsla,
}

impl BackgroundRegion {
    pub(crate) fn new(line: i32, col: i32, color: Hsla) -> Self {
        BackgroundRegion {
            start_line: line,
            start_col: col,
            end_line: line,
            end_col: col,
            color,
        }
    }

    /// Check if this region can be merged with another region
    pub(crate) fn can_merge_with(&self, other: &BackgroundRegion) -> bool {
        if self.color != other.color {
            return false;
        }

        // Check if regions are adjacent horizontally
        if self.start_line == other.start_line && self.end_line == other.end_line {
            return self.end_col + 1 == other.start_col || other.end_col + 1 == self.start_col;
        }

        // Check if regions are adjacent vertically with same column span
        if self.start_col == other.start_col && self.end_col == other.end_col {
            return self.end_line + 1 == other.start_line || other.end_line + 1 == self.start_line;
        }

        false
    }

    /// Merge this region with another region
    pub(crate) fn merge_with(&mut self, other: &BackgroundRegion) {
        self.start_line = self.start_line.min(other.start_line);
        self.start_col = self.start_col.min(other.start_col);
        self.end_line = self.end_line.max(other.end_line);
        self.end_col = self.end_col.max(other.end_col);
    }
}

pub trait TerminalLayoutCell {
    fn point(&self) -> Point;
    fn cell(&self) -> &Cell;
}

impl TerminalLayoutCell for IndexedCell {
    fn point(&self) -> Point {
        self.point
    }

    fn cell(&self) -> &Cell {
        &self.cell
    }
}

impl TerminalLayoutCell for &IndexedCell {
    fn point(&self) -> Point {
        self.point
    }

    fn cell(&self) -> &Cell {
        &self.cell
    }
}

/// Merge background regions to minimize the number of rectangles
pub(crate) fn merge_background_regions(regions: Vec<BackgroundRegion>) -> Vec<BackgroundRegion> {
    if regions.is_empty() {
        return regions;
    }

    let mut merged = regions;
    let mut changed = true;

    // Keep merging until no more merges are possible
    while changed {
        changed = false;
        let mut i = 0;

        while i < merged.len() {
            let mut j = i + 1;
            while j < merged.len() {
                if merged[i].can_merge_with(&merged[j]) {
                    let other = merged.remove(j);
                    merged[i].merge_with(&other);
                    changed = true;
                } else {
                    j += 1;
                }
            }
            i += 1;
        }
    }

    merged
}
