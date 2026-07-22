use super::*;

impl BufferSnapshot {
    pub fn as_rope(&self) -> &Rope {
        &self.visible_text
    }

    pub fn rope_for_version(&self, version: &clock::Global) -> Rope {
        let mut rope = Rope::new();

        let mut cursor = self
            .fragments
            .filter::<_, FragmentTextSummary>(&None, move |summary| {
                !version.observed_all(&summary.max_version)
            });
        cursor.next();

        let mut visible_cursor = self.visible_text.cursor(0);
        let mut deleted_cursor = self.deleted_text.cursor(0);

        while let Some(fragment) = cursor.item() {
            if cursor.start().visible > visible_cursor.offset() {
                let text = visible_cursor.slice(cursor.start().visible);
                rope.append(text);
            }

            if fragment.was_visible(version, &self.undo_map) {
                if fragment.visible {
                    let text = visible_cursor.slice(cursor.end().visible);
                    rope.append(text);
                } else {
                    deleted_cursor.seek_forward(cursor.start().deleted);
                    let text = deleted_cursor.slice(cursor.end().deleted);
                    rope.append(text);
                }
            } else if fragment.visible {
                visible_cursor.seek_forward(cursor.end().visible);
            }

            cursor.next();
        }

        if cursor.start().visible > visible_cursor.offset() {
            let text = visible_cursor.slice(cursor.start().visible);
            rope.append(text);
        }

        rope
    }

    pub fn remote_id(&self) -> BufferId {
        self.remote_id
    }

    pub fn replica_id(&self) -> ReplicaId {
        self.replica_id
    }

    pub fn row_count(&self) -> u32 {
        self.max_point().row + 1
    }

    pub fn len(&self) -> usize {
        self.visible_text.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn chars(&self) -> impl Iterator<Item = char> + '_ {
        self.chars_at(0)
    }

    pub fn chars_for_range<T: ToOffset>(&self, range: Range<T>) -> impl Iterator<Item = char> + '_ {
        self.text_for_range(range).flat_map(str::chars)
    }

    pub fn reversed_chars_for_range<T: ToOffset>(
        &self,
        range: Range<T>,
    ) -> impl Iterator<Item = char> + '_ {
        self.reversed_chunks_in_range(range)
            .flat_map(|chunk| chunk.chars().rev())
    }

    pub fn contains_str_at<T>(&self, position: T, needle: &str) -> bool
    where
        T: ToOffset,
    {
        let position = position.to_offset(self);
        position == self.clip_offset(position, Bias::Left)
            && self
                .bytes_in_range(position..self.len())
                .flatten()
                .copied()
                .take(needle.len())
                .eq(needle.bytes())
    }

    pub fn common_prefix_at<T>(&self, position: T, needle: &str) -> Range<T>
    where
        T: ToOffset + TextDimension,
    {
        let offset = position.to_offset(self);
        let common_prefix_len = needle
            .char_indices()
            .map(|(index, _)| index)
            .chain([needle.len()])
            .take_while(|&len| len <= offset)
            .filter(|&len| {
                let left = self
                    .chars_for_range(offset - len..offset)
                    .flat_map(char::to_lowercase);
                let right = needle[..len].chars().flat_map(char::to_lowercase);
                left.eq(right)
            })
            .last()
            .unwrap_or(0);
        let start_offset = offset - common_prefix_len;
        let start = self.text_summary_for_range(0..start_offset);
        start..position
    }

    pub fn text(&self) -> String {
        self.visible_text.to_string()
    }

    pub fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    pub fn deleted_text(&self) -> String {
        self.deleted_text.to_string()
    }

    pub fn text_summary(&self) -> TextSummary {
        self.visible_text.summary()
    }

    pub fn max_point(&self) -> Point {
        self.visible_text.max_point()
    }

    pub fn max_point_utf16(&self) -> PointUtf16 {
        self.visible_text.max_point_utf16()
    }

    pub fn point_to_offset(&self, point: Point) -> usize {
        self.visible_text.point_to_offset(point)
    }

    pub fn point_to_offset_utf16(&self, point: Point) -> OffsetUtf16 {
        self.visible_text.point_to_offset_utf16(point)
    }

    pub fn point_utf16_to_offset_utf16(&self, point: PointUtf16) -> OffsetUtf16 {
        self.visible_text.point_utf16_to_offset_utf16(point)
    }

    pub fn point_utf16_to_offset(&self, point: PointUtf16) -> usize {
        self.visible_text.point_utf16_to_offset(point)
    }

    pub fn unclipped_point_utf16_to_offset(&self, point: Unclipped<PointUtf16>) -> usize {
        self.visible_text.unclipped_point_utf16_to_offset(point)
    }

    pub fn unclipped_point_utf16_to_point(&self, point: Unclipped<PointUtf16>) -> Point {
        self.visible_text.unclipped_point_utf16_to_point(point)
    }

    pub fn offset_utf16_to_offset(&self, offset: OffsetUtf16) -> usize {
        self.visible_text.offset_utf16_to_offset(offset)
    }

    pub fn offset_to_offset_utf16(&self, offset: usize) -> OffsetUtf16 {
        self.visible_text.offset_to_offset_utf16(offset)
    }

    pub fn offset_to_point(&self, offset: usize) -> Point {
        self.visible_text.offset_to_point(offset)
    }

    pub fn offset_to_point_utf16(&self, offset: usize) -> PointUtf16 {
        self.visible_text.offset_to_point_utf16(offset)
    }

    pub fn point_to_point_utf16(&self, point: Point) -> PointUtf16 {
        self.visible_text.point_to_point_utf16(point)
    }

    pub fn point_utf16_to_point(&self, point: PointUtf16) -> Point {
        self.visible_text.point_utf16_to_point(point)
    }

    pub fn version(&self) -> &clock::Global {
        &self.version
    }

    pub fn chars_at<T: ToOffset>(&self, position: T) -> impl Iterator<Item = char> + '_ {
        let offset = position.to_offset(self);
        self.visible_text.chars_at(offset)
    }

    pub fn reversed_chars_at<T: ToOffset>(&self, position: T) -> impl Iterator<Item = char> + '_ {
        let offset = position.to_offset(self);
        self.visible_text.reversed_chars_at(offset)
    }

    pub fn reversed_chunks_in_range<T: ToOffset>(&self, range: Range<T>) -> rope::Chunks<'_> {
        let range = range.start.to_offset(self)..range.end.to_offset(self);
        self.visible_text.reversed_chunks_in_range(range)
    }

    pub fn bytes_in_range<T: ToOffset>(&self, range: Range<T>) -> rope::Bytes<'_> {
        let start = range.start.to_offset(self);
        let end = range.end.to_offset(self);
        self.visible_text.bytes_in_range(start..end)
    }

    pub fn reversed_bytes_in_range<T: ToOffset>(&self, range: Range<T>) -> rope::Bytes<'_> {
        let start = range.start.to_offset(self);
        let end = range.end.to_offset(self);
        self.visible_text.reversed_bytes_in_range(start..end)
    }

    pub fn text_for_range<T: ToOffset>(&self, range: Range<T>) -> Chunks<'_> {
        let start = range.start.to_offset(self);
        let end = range.end.to_offset(self);
        self.visible_text.chunks_in_range(start..end)
    }

    pub fn line_len(&self, row: u32) -> u32 {
        let row_start_offset = Point::new(row, 0).to_offset(self);
        let row_end_offset = if row >= self.max_point().row {
            self.len()
        } else {
            Point::new(row + 1, 0).to_previous_offset(self)
        };
        (row_end_offset - row_start_offset) as u32
    }

    /// A function to convert character offsets from e.g. user's `go.mod:22:33` input into byte-offset Point columns.
    pub fn point_from_external_input(&self, row: u32, characters: u32) -> Point {
        const MAX_BYTES_IN_UTF_8: u32 = 4;

        let row = row.min(self.max_point().row);
        let start = Point::new(row, 0);
        let end = self.clip_point(
            Point::new(
                row,
                characters
                    .saturating_mul(MAX_BYTES_IN_UTF_8)
                    .saturating_add(1),
            ),
            Bias::Right,
        );
        let range = start..end;
        let mut point = range.start;
        let mut remaining_columns = characters;

        for chunk in self.text_for_range(range) {
            for character in chunk.chars() {
                if remaining_columns == 0 {
                    return point;
                }
                remaining_columns -= 1;
                point.column += character.len_utf8() as u32;
            }
        }
        point
    }

    pub fn line_indents_in_row_range(
        &self,
        row_range: Range<u32>,
    ) -> impl Iterator<Item = (u32, LineIndent)> + '_ {
        let start = Point::new(row_range.start, 0).to_offset(self);
        let end = Point::new(row_range.end, self.line_len(row_range.end)).to_offset(self);

        let mut chunks = self.as_rope().chunks_in_range(start..end);
        let mut row = row_range.start;
        let mut done = false;
        std::iter::from_fn(move || {
            if done {
                None
            } else {
                let indent = (row, LineIndent::from_chunks(&mut chunks));
                done = !chunks.next_line();
                row += 1;
                Some(indent)
            }
        })
    }
}
