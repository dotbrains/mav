use super::*;

impl BlockSnapshot {
    #[inline(always)]
    pub fn has_replacement_blocks(&self) -> bool {
        self.transforms.summary().has_replacement_blocks
    }

    #[cfg(test)]
    #[ztracing::instrument(skip_all)]
    pub fn text(&self) -> String {
        self.chunks(
            BlockRow(0)..self.transforms.summary().output_rows,
            LanguageAwareStyling {
                tree_sitter: false,
                diagnostics: false,
            },
            false,
            Highlights::default(),
        )
        .map(|chunk| chunk.text)
        .collect()
    }

    #[ztracing::instrument(skip_all)]
    pub(crate) fn chunks<'a>(
        &'a self,
        rows: Range<BlockRow>,
        language_aware: LanguageAwareStyling,
        masked: bool,
        highlights: Highlights<'a>,
    ) -> BlockChunks<'a> {
        let max_output_row = cmp::min(rows.end, self.transforms.summary().output_rows);

        let mut cursor = self.transforms.cursor::<Dimensions<BlockRow, WrapRow>>(());
        cursor.seek(&rows.start, Bias::Right);
        let transform_output_start = cursor.start().0;
        let transform_input_start = cursor.start().1;

        let mut input_start = transform_input_start;
        let mut input_end = transform_input_start;
        if let Some(transform) = cursor.item()
            && transform.block.is_none()
        {
            input_start += rows.start - transform_output_start;
            input_end += cmp::min(
                rows.end - transform_output_start,
                RowDelta(transform.summary.input_rows.0),
            );
        }

        BlockChunks {
            input_chunks: self.wrap_snapshot.chunks(
                input_start..input_end,
                language_aware,
                highlights,
            ),
            input_chunk: Default::default(),
            transforms: cursor,
            output_row: rows.start,
            line_count_overflow: RowDelta(0),
            max_output_row,
            masked,
        }
    }

    #[ztracing::instrument(skip_all)]
    pub(crate) fn row_infos(&self, start_row: BlockRow) -> BlockRows<'_> {
        let mut cursor = self.transforms.cursor::<Dimensions<BlockRow, WrapRow>>(());
        cursor.seek(&start_row, Bias::Right);
        let Dimensions(output_start, input_start, _) = cursor.start();
        let overshoot = if cursor
            .item()
            .is_some_and(|transform| transform.block.is_none())
        {
            start_row - *output_start
        } else {
            RowDelta(0)
        };
        let input_start_row = *input_start + overshoot;
        BlockRows {
            transforms: cursor,
            input_rows: self.wrap_snapshot.row_infos(input_start_row),
            output_row: start_row,
            started: false,
        }
    }

    #[ztracing::instrument(skip_all)]
    pub fn blocks_in_range(
        &self,
        rows: Range<BlockRow>,
    ) -> impl Iterator<Item = (BlockRow, &Block)> {
        let mut cursor = self.transforms.cursor::<BlockRow>(());
        cursor.seek(&rows.start, Bias::Left);
        while *cursor.start() < rows.start && cursor.end() <= rows.start {
            cursor.next();
        }

        std::iter::from_fn(move || {
            while let Some(transform) = cursor.item() {
                let start_row = *cursor.start();
                if start_row > rows.end
                    || (start_row == rows.end
                        && transform
                            .block
                            .as_ref()
                            .is_some_and(|block| block.height() > 0))
                {
                    break;
                }
                if let Some(block) = &transform.block {
                    cursor.next();
                    return Some((start_row, block));
                } else {
                    cursor.next();
                }
            }
            None
        })
    }

    #[ztracing::instrument(skip_all)]
    pub(crate) fn sticky_header_excerpt(&self, position: f64) -> Option<StickyHeaderExcerpt<'_>> {
        let top_row = position as u32;
        let mut cursor = self.transforms.cursor::<BlockRow>(());
        cursor.seek(&BlockRow(top_row), Bias::Right);

        while let Some(transform) = cursor.item() {
            match &transform.block {
                Some(
                    Block::ExcerptBoundary { excerpt, .. } | Block::BufferHeader { excerpt, .. },
                ) => {
                    return Some(StickyHeaderExcerpt { excerpt });
                }
                Some(block) if block.is_buffer_header() => return None,
                _ => {
                    cursor.prev();
                    continue;
                }
            }
        }

        None
    }

    #[ztracing::instrument(skip_all)]
    pub fn block_for_id(&self, block_id: BlockId) -> Option<Block> {
        let buffer = self.wrap_snapshot.buffer_snapshot();
        let wrap_point = match block_id {
            BlockId::Custom(custom_block_id) => {
                let custom_block = self.custom_blocks_by_id.get(&custom_block_id)?;
                return Some(Block::Custom(custom_block.clone()));
            }
            BlockId::ExcerptBoundary(start_anchor) => {
                let start_point = start_anchor.to_point(&buffer);
                self.wrap_snapshot.make_wrap_point(start_point, Bias::Left)
            }
            BlockId::FoldedBuffer(buffer_id) => self.wrap_snapshot.make_wrap_point(
                buffer
                    .anchor_in_excerpt(buffer.excerpts_for_buffer(buffer_id).next()?.context.start)?
                    .to_point(buffer),
                Bias::Left,
            ),
            BlockId::Spacer(_) => return None,
        };
        let wrap_row = wrap_point.row();

        let mut cursor = self.transforms.cursor::<WrapRow>(());
        cursor.seek(&wrap_row, Bias::Left);

        while let Some(transform) = cursor.item() {
            if let Some(block) = transform.block.as_ref() {
                if block.id() == block_id {
                    return Some(block.clone());
                }
            } else if *cursor.start() > wrap_row {
                break;
            }

            cursor.next();
        }

        None
    }

    #[ztracing::instrument(skip_all)]
    pub fn max_point(&self) -> BlockPoint {
        let row = self
            .transforms
            .summary()
            .output_rows
            .saturating_sub(RowDelta(1));
        BlockPoint::new(row, self.line_len(row))
    }

    #[ztracing::instrument(skip_all)]
    pub fn longest_row(&self) -> BlockRow {
        self.transforms.summary().longest_row
    }

    #[ztracing::instrument(skip_all)]
    pub fn longest_row_in_range(&self, range: Range<BlockRow>) -> BlockRow {
        let mut cursor = self.transforms.cursor::<Dimensions<BlockRow, WrapRow>>(());
        cursor.seek(&range.start, Bias::Right);

        let mut longest_row = range.start;
        let mut longest_row_chars = 0;
        if let Some(transform) = cursor.item() {
            if transform.block.is_none() {
                let &Dimensions(output_start, input_start, _) = cursor.start();
                let overshoot = range.start - output_start;
                let wrap_start_row = input_start + WrapRow(overshoot.0);
                let wrap_end_row = cmp::min(
                    input_start + WrapRow((range.end - output_start).0),
                    cursor.end().1,
                );
                let summary = self
                    .wrap_snapshot
                    .text_summary_for_range(wrap_start_row..wrap_end_row);
                longest_row = BlockRow(range.start.0 + summary.longest_row);
                longest_row_chars = summary.longest_row_chars;
            }
            cursor.next();
        }

        let cursor_start_row = cursor.start().0;
        if range.end > cursor_start_row {
            let summary = cursor.summary::<_, TransformSummary>(&range.end, Bias::Right);
            if summary.longest_row_chars > longest_row_chars {
                longest_row = cursor_start_row + summary.longest_row;
                longest_row_chars = summary.longest_row_chars;
            }

            if let Some(transform) = cursor.item()
                && transform.block.is_none()
            {
                let &Dimensions(output_start, input_start, _) = cursor.start();
                let overshoot = range.end - output_start;
                let wrap_start_row = input_start;
                let wrap_end_row = input_start + overshoot;
                let summary = self
                    .wrap_snapshot
                    .text_summary_for_range(wrap_start_row..wrap_end_row);
                if summary.longest_row_chars > longest_row_chars {
                    longest_row = output_start + RowDelta(summary.longest_row);
                }
            }
        }

        longest_row
    }

    #[ztracing::instrument(skip_all)]
    pub(crate) fn line_len(&self, row: BlockRow) -> u32 {
        let (start, _, item) =
            self.transforms
                .find::<Dimensions<BlockRow, WrapRow>, _>((), &row, Bias::Right);
        if let Some(transform) = item {
            let Dimensions(output_start, input_start, _) = start;
            let overshoot = row - output_start;
            if transform.block.is_some() {
                0
            } else {
                self.wrap_snapshot.line_len(input_start + overshoot)
            }
        } else if row == BlockRow(0) {
            0
        } else {
            panic!("row out of range");
        }
    }

    #[ztracing::instrument(skip_all)]
    pub(crate) fn is_block_line(&self, row: BlockRow) -> bool {
        let (_, _, item) = self.transforms.find::<BlockRow, _>((), &row, Bias::Right);
        item.is_some_and(|t| t.block.is_some())
    }

    #[ztracing::instrument(skip_all)]
    pub(crate) fn is_folded_buffer_header(&self, row: BlockRow) -> bool {
        let (_, _, item) = self.transforms.find::<BlockRow, _>((), &row, Bias::Right);
        let Some(transform) = item else {
            return false;
        };
        matches!(transform.block, Some(Block::FoldedBuffer { .. }))
    }

    #[ztracing::instrument(skip_all)]
    pub(crate) fn is_line_replaced(&self, row: MultiBufferRow) -> bool {
        let wrap_point = self
            .wrap_snapshot
            .make_wrap_point(Point::new(row.0, 0), Bias::Left);
        let (_, _, item) = self
            .transforms
            .find::<WrapRow, _>((), &wrap_point.row(), Bias::Right);
        item.is_some_and(|transform| {
            transform
                .block
                .as_ref()
                .is_some_and(|block| block.is_replacement())
        })
    }

    #[ztracing::instrument(skip_all)]
    pub fn clip_point(&self, point: BlockPoint, bias: Bias) -> BlockPoint {
        let mut cursor = self.transforms.cursor::<Dimensions<BlockRow, WrapRow>>(());
        cursor.seek(&BlockRow(point.row), Bias::Right);

        let max_input_row = self.transforms.summary().input_rows;
        let mut search_left = (bias == Bias::Left && cursor.start().1 > WrapRow(0))
            || cursor.end().1 == max_input_row;
        let mut reversed = false;

        loop {
            if let Some(transform) = cursor.item() {
                let Dimensions(output_start_row, input_start_row, _) = cursor.start();
                let Dimensions(output_end_row, input_end_row, _) = cursor.end();
                let output_start = Point::new(output_start_row.0, 0);
                let input_start = Point::new(input_start_row.0, 0);
                let input_end = Point::new(input_end_row.0, 0);

                match transform.block.as_ref() {
                    Some(block) => {
                        if block.is_replacement()
                            && (((bias == Bias::Left || search_left) && output_start <= point.0)
                                || (!search_left && output_start >= point.0))
                        {
                            return BlockPoint(output_start);
                        }
                    }
                    None => {
                        let input_point = if point.row >= output_end_row.0 {
                            let line_len = self.wrap_snapshot.line_len(input_end_row - RowDelta(1));
                            self.wrap_snapshot.clip_point(
                                WrapPoint::new(input_end_row - RowDelta(1), line_len),
                                bias,
                            )
                        } else {
                            let output_overshoot = point.0.saturating_sub(output_start);
                            self.wrap_snapshot
                                .clip_point(WrapPoint(input_start + output_overshoot), bias)
                        };

                        if (input_start..input_end).contains(&input_point.0) {
                            let input_overshoot = input_point.0.saturating_sub(input_start);
                            return BlockPoint(output_start + input_overshoot);
                        }
                    }
                }

                if search_left {
                    cursor.prev();
                } else {
                    cursor.next();
                }
            } else if reversed {
                return self.max_point();
            } else {
                reversed = true;
                search_left = !search_left;
                cursor.seek(&BlockRow(point.row), Bias::Right);
            }
        }
    }

    pub fn block_point_cursor(&self) -> BlockPointCursor<'_> {
        BlockPointCursor {
            snapshot: self,
            cursor: self.transforms.cursor::<Dimensions<WrapRow, BlockRow>>(()),
        }
    }

    #[ztracing::instrument(skip_all)]
    pub fn to_block_point(&self, wrap_point: WrapPoint) -> BlockPoint {
        let (start, _, item) = self.transforms.find::<Dimensions<WrapRow, BlockRow>, _>(
            (),
            &wrap_point.row(),
            Bias::Right,
        );
        if let Some(transform) = item {
            if transform.block.is_some() {
                BlockPoint::new(start.1, 0)
            } else {
                let Dimensions(input_start_row, output_start_row, _) = start;
                let input_start = Point::new(input_start_row.0, 0);
                let output_start = Point::new(output_start_row.0, 0);
                let input_overshoot = wrap_point.0 - input_start;
                BlockPoint(output_start + input_overshoot)
            }
        } else {
            self.max_point()
        }
    }

    #[ztracing::instrument(skip_all)]
    pub fn to_wrap_point(&self, block_point: BlockPoint, bias: Bias) -> WrapPoint {
        let (start, end, item) = self.transforms.find::<Dimensions<BlockRow, WrapRow>, _>(
            (),
            &BlockRow(block_point.row),
            Bias::Right,
        );
        if let Some(transform) = item {
            match transform.block.as_ref() {
                Some(block) => {
                    if block.place_below() {
                        let wrap_row = start.1 - RowDelta(1);
                        WrapPoint::new(wrap_row, self.wrap_snapshot.line_len(wrap_row))
                    } else if block.place_above() {
                        WrapPoint::new(start.1, 0)
                    } else if bias == Bias::Left {
                        WrapPoint::new(start.1, 0)
                    } else {
                        let wrap_row = end.1 - RowDelta(1);
                        WrapPoint::new(wrap_row, self.wrap_snapshot.line_len(wrap_row))
                    }
                }
                None => {
                    let overshoot = block_point.row() - start.0;
                    let wrap_row = start.1 + RowDelta(overshoot.0);
                    WrapPoint::new(wrap_row, block_point.column)
                }
            }
        } else {
            self.wrap_snapshot.max_point()
        }
    }
}
