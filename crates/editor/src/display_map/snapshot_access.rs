use super::*;

impl DisplaySnapshot {
    pub fn companion_snapshot(&self) -> Option<&DisplaySnapshot> {
        self.companion_display_snapshot.as_deref()
    }

    pub fn wrap_snapshot(&self) -> &WrapSnapshot {
        &self.block_snapshot.wrap_snapshot
    }
    pub fn tab_snapshot(&self) -> &TabSnapshot {
        &self.block_snapshot.wrap_snapshot.tab_snapshot
    }

    pub fn fold_snapshot(&self) -> &FoldSnapshot {
        &self.block_snapshot.wrap_snapshot.tab_snapshot.fold_snapshot
    }

    #[inline(always)]
    pub fn has_collapsed_content(&self) -> bool {
        self.fold_snapshot().has_folds() || self.block_snapshot.has_replacement_blocks()
    }

    pub fn inlay_snapshot(&self) -> &InlaySnapshot {
        &self
            .block_snapshot
            .wrap_snapshot
            .tab_snapshot
            .fold_snapshot
            .inlay_snapshot
    }

    pub fn buffer_snapshot(&self) -> &MultiBufferSnapshot {
        &self
            .block_snapshot
            .wrap_snapshot
            .tab_snapshot
            .fold_snapshot
            .inlay_snapshot
            .buffer
    }

    #[cfg(test)]
    pub fn fold_count(&self) -> usize {
        self.fold_snapshot().fold_count()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer_snapshot().len() == MultiBufferOffset(0)
    }

    /// Returns whether tree-sitter syntax highlighting should be used.
    /// Returns `false` if any buffer with semantic token highlights has the "full" mode setting,
    /// meaning LSP semantic tokens should replace tree-sitter highlighting.
    pub fn use_tree_sitter_for_syntax(&self, position: DisplayRow, cx: &App) -> bool {
        let position = DisplayPoint::new(position, 0);
        let Some((buffer_snapshot, ..)) = self.point_to_buffer_point(position.to_point(self))
        else {
            return false;
        };
        let settings = LanguageSettings::for_buffer_snapshot(&buffer_snapshot, None, cx);
        settings.semantic_tokens.use_tree_sitter()
    }

    pub fn row_infos(&self, start_row: DisplayRow) -> impl Iterator<Item = RowInfo> + '_ {
        self.block_snapshot.row_infos(BlockRow(start_row.0))
    }

    pub fn widest_line_number(&self) -> u32 {
        self.buffer_snapshot().widest_line_number()
    }

    #[instrument(skip_all)]
    pub fn prev_line_boundary(&self, mut point: MultiBufferPoint) -> (Point, DisplayPoint) {
        loop {
            let mut inlay_point = self.inlay_snapshot().to_inlay_point(point);
            let mut fold_point = self.fold_snapshot().to_fold_point(inlay_point, Bias::Left);
            fold_point.0.column = 0;
            inlay_point = fold_point.to_inlay_point(self.fold_snapshot());
            point = self.inlay_snapshot().to_buffer_point(inlay_point);

            let mut display_point = self.point_to_display_point(point, Bias::Left);
            *display_point.column_mut() = 0;
            let next_point = self.display_point_to_point(display_point, Bias::Left);
            if next_point == point {
                return (point, display_point);
            }
            point = next_point;
        }
    }

    #[instrument(skip_all)]
    pub fn next_line_boundary(
        &self,
        mut point: MultiBufferPoint,
    ) -> (MultiBufferPoint, DisplayPoint) {
        let original_point = point;
        loop {
            let mut inlay_point = self.inlay_snapshot().to_inlay_point(point);
            let mut fold_point = self.fold_snapshot().to_fold_point(inlay_point, Bias::Right);
            fold_point.0.column = self.fold_snapshot().line_len(fold_point.row());
            inlay_point = fold_point.to_inlay_point(self.fold_snapshot());
            point = self.inlay_snapshot().to_buffer_point(inlay_point);

            let mut display_point = self.point_to_display_point(point, Bias::Right);
            *display_point.column_mut() = self.line_len(display_point.row());
            let next_point = self.display_point_to_point(display_point, Bias::Right);
            if next_point == point || original_point == point || original_point == next_point {
                return (point, display_point);
            }
            point = next_point;
        }
    }

    // used by line_mode selections and tries to match vim behavior
    pub fn expand_to_line(&self, range: Range<Point>) -> Range<Point> {
        let new_start = MultiBufferPoint::new(range.start.row, 0);
        let new_end = if range.end.column > 0 {
            MultiBufferPoint::new(
                range.end.row,
                self.buffer_snapshot()
                    .line_len(MultiBufferRow(range.end.row)),
            )
        } else {
            range.end
        };

        new_start..new_end
    }

    #[instrument(skip_all)]
    pub fn point_to_display_point(&self, point: MultiBufferPoint, bias: Bias) -> DisplayPoint {
        let inlay_point = self.inlay_snapshot().to_inlay_point(point);
        let fold_point = self.fold_snapshot().to_fold_point(inlay_point, bias);
        let tab_point = self.tab_snapshot().fold_point_to_tab_point(fold_point);
        let wrap_point = self.wrap_snapshot().tab_point_to_wrap_point(tab_point);
        let block_point = self.block_snapshot.to_block_point(wrap_point);
        DisplayPoint(block_point)
    }

    /// Converts a buffer offset range into one or more `DisplayPoint` ranges
    /// that cover only actual buffer text, excluding any inlay hint text that
    /// falls within the range.
    pub fn isomorphic_display_point_ranges_for_buffer_range(
        &self,
        range: Range<MultiBufferOffset>,
    ) -> SmallVec<[Range<DisplayPoint>; 1]> {
        self.display_point_converter().map(range)
    }

    /// Returns a converter that maps buffer offset ranges to `DisplayPoint`
    /// ranges (as in [`Self::isomorphic_display_point_ranges_for_buffer_range`])
    /// while reusing cursor state across calls. Use this when converting many
    /// ranges in a single pass; the inputs must be supplied with non-decreasing
    /// offsets so the underlying cursors only advance forward.
    pub fn display_point_converter(&self) -> DisplayPointConverter<'_> {
        DisplayPointConverter {
            inlay_cursor: self.inlay_snapshot().buffer_offset_to_inlay_point_cursor(),
            fold_point_cursor: self.fold_snapshot().fold_point_cursor(),
            tab_point_cursor: self.tab_snapshot().tab_point_cursor(),
            wrap_point_cursor: self.wrap_snapshot().wrap_point_cursor(),
            block_point_cursor: self.block_snapshot.block_point_cursor(),
            prev_end: None,
        }
    }

    pub fn display_point_to_point(&self, point: DisplayPoint, bias: Bias) -> Point {
        self.inlay_snapshot()
            .to_buffer_point(self.display_point_to_inlay_point(point, bias))
    }

    pub fn display_point_to_inlay_offset(&self, point: DisplayPoint, bias: Bias) -> InlayOffset {
        self.inlay_snapshot()
            .to_offset(self.display_point_to_inlay_point(point, bias))
    }

    pub fn anchor_to_inlay_offset(&self, anchor: Anchor) -> InlayOffset {
        self.inlay_snapshot()
            .to_inlay_offset(anchor.to_offset(self.buffer_snapshot()))
    }

    pub fn display_point_to_anchor(&self, point: DisplayPoint, bias: Bias) -> Anchor {
        self.buffer_snapshot()
            .anchor_at(point.to_offset(self, bias), bias)
    }

    #[instrument(skip_all)]
    fn display_point_to_inlay_point(&self, point: DisplayPoint, bias: Bias) -> InlayPoint {
        let block_point = point.0;
        let wrap_point = self.block_snapshot.to_wrap_point(block_point, bias);
        let tab_point = self.wrap_snapshot().to_tab_point(wrap_point);
        let fold_point = self
            .tab_snapshot()
            .tab_point_to_fold_point(tab_point, bias)
            .0;
        fold_point.to_inlay_point(self.fold_snapshot())
    }

    #[instrument(skip_all)]
    pub fn display_point_to_fold_point(&self, point: DisplayPoint, bias: Bias) -> FoldPoint {
        let block_point = point.0;
        let wrap_point = self.block_snapshot.to_wrap_point(block_point, bias);
        let tab_point = self.wrap_snapshot().to_tab_point(wrap_point);
        self.tab_snapshot()
            .tab_point_to_fold_point(tab_point, bias)
            .0
    }

    #[instrument(skip_all)]
    pub fn fold_point_to_display_point(&self, fold_point: FoldPoint) -> DisplayPoint {
        let tab_point = self.tab_snapshot().fold_point_to_tab_point(fold_point);
        let wrap_point = self.wrap_snapshot().tab_point_to_wrap_point(tab_point);
        let block_point = self.block_snapshot.to_block_point(wrap_point);
        DisplayPoint(block_point)
    }

    pub fn max_point(&self) -> DisplayPoint {
        DisplayPoint(self.block_snapshot.max_point())
    }

    /// Returns text chunks starting at the given display row until the end of the file
    #[instrument(skip_all)]
    pub fn text_chunks(&self, display_row: DisplayRow) -> impl Iterator<Item = &str> {
        self.block_snapshot
            .chunks(
                BlockRow(display_row.0)..BlockRow(self.max_point().row().next_row().0),
                LanguageAwareStyling {
                    tree_sitter: false,
                    diagnostics: false,
                },
                self.masked,
                Highlights::default(),
            )
            .map(|h| h.text)
    }

    /// Returns text chunks starting at the end of the given display row in reverse until the start of the file
    #[instrument(skip_all)]
    pub fn reverse_text_chunks(&self, display_row: DisplayRow) -> impl Iterator<Item = &str> {
        (0..=display_row.0).rev().flat_map(move |row| {
            self.block_snapshot
                .chunks(
                    BlockRow(row)..BlockRow(row + 1),
                    LanguageAwareStyling {
                        tree_sitter: false,
                        diagnostics: false,
                    },
                    self.masked,
                    Highlights::default(),
                )
                .map(|h| h.text)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
        })
    }
}
