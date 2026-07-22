use super::*;

impl BlockChunks<'_> {
    /// Go to the next transform
    #[ztracing::instrument(skip_all)]
    fn advance(&mut self) {
        self.input_chunk = Chunk::default();
        self.transforms.next();
        while let Some(transform) = self.transforms.item() {
            if transform
                .block
                .as_ref()
                .is_some_and(|block| block.height() == 0)
            {
                self.transforms.next();
            } else {
                break;
            }
        }

        if self
            .transforms
            .item()
            .is_some_and(|transform| transform.block.is_none())
        {
            let start_input_row = self.transforms.start().1;
            let start_output_row = self.transforms.start().0;
            if start_output_row < self.max_output_row {
                let end_input_row = cmp::min(
                    self.transforms.end().1,
                    start_input_row + (self.max_output_row - start_output_row),
                );
                self.input_chunks.seek(start_input_row..end_input_row);
            }
        }
    }
}

pub struct StickyHeaderExcerpt<'a> {
    pub excerpt: &'a ExcerptBoundaryInfo,
}

impl<'a> Iterator for BlockChunks<'a> {
    type Item = Chunk<'a>;

    #[ztracing::instrument(skip_all)]
    fn next(&mut self) -> Option<Self::Item> {
        if self.output_row >= self.max_output_row {
            return None;
        }

        if self.line_count_overflow > RowDelta(0) {
            let lines = self.line_count_overflow.0.min(u128::BITS);
            self.line_count_overflow.0 -= lines;
            self.output_row += RowDelta(lines);
            return Some(Chunk {
                text: unsafe { std::str::from_utf8_unchecked(&NEWLINES[..lines as usize]) },
                chars: 1u128.unbounded_shl(lines).wrapping_sub(1),
                ..Default::default()
            });
        }

        let transform = self.transforms.item()?;
        if transform.block.is_some() {
            let block_start = self.transforms.start().0;
            let mut block_end = self.transforms.end().0;
            self.advance();
            if self.transforms.item().is_none() {
                block_end -= RowDelta(1);
            }

            let start_in_block = self.output_row - block_start;
            let end_in_block = cmp::min(self.max_output_row, block_end) - block_start;
            let line_count = end_in_block - start_in_block;
            let lines = RowDelta(line_count.0.min(u128::BITS));
            self.line_count_overflow = line_count - lines;
            self.output_row += lines;

            return Some(Chunk {
                text: unsafe { std::str::from_utf8_unchecked(&NEWLINES[..lines.0 as usize]) },
                chars: 1u128.unbounded_shl(lines.0).wrapping_sub(1),
                ..Default::default()
            });
        }

        if self.input_chunk.text.is_empty() {
            if let Some(input_chunk) = self.input_chunks.next() {
                self.input_chunk = input_chunk;
            } else {
                if self.output_row < self.max_output_row {
                    self.output_row.0 += 1;
                    self.advance();
                    if self.transforms.item().is_some() {
                        return Some(Chunk {
                            text: "\n",
                            chars: 1,
                            ..Default::default()
                        });
                    }
                }
                return None;
            }
        }

        let transform_end = self.transforms.end().0;
        let (prefix_rows, prefix_bytes) =
            offset_for_row(self.input_chunk.text, transform_end - self.output_row);
        self.output_row += prefix_rows;

        let (mut prefix, suffix) = self.input_chunk.text.split_at(prefix_bytes);
        self.input_chunk.text = suffix;
        self.input_chunk.tabs >>= prefix_bytes.saturating_sub(1);
        self.input_chunk.chars >>= prefix_bytes.saturating_sub(1);
        self.input_chunk.newlines >>= prefix_bytes.saturating_sub(1);

        let mut tabs = self.input_chunk.tabs;
        let mut chars = self.input_chunk.chars;
        let mut newlines = self.input_chunk.newlines;

        if self.masked {
            // Not great for multibyte text because to keep cursor math correct we
            // need to have the same number of chars in the input as output.
            let chars_count = prefix.chars().count();
            let bullet_len = chars_count;
            prefix = unsafe { std::str::from_utf8_unchecked(&BULLETS[..bullet_len]) };
            chars = 1u128.unbounded_shl(bullet_len as u32).wrapping_sub(1);
            tabs = 0;
            newlines = 0;
        }

        let chunk = Chunk {
            text: prefix,
            tabs,
            chars,
            newlines,
            ..self.input_chunk.clone()
        };

        if self.output_row == transform_end {
            self.advance();
        }

        Some(chunk)
    }
}

impl Iterator for BlockRows<'_> {
    type Item = RowInfo;

    #[ztracing::instrument(skip_all)]
    fn next(&mut self) -> Option<Self::Item> {
        if self.started {
            self.output_row.0 += 1;
        } else {
            self.started = true;
        }

        if self.output_row >= self.transforms.end().0 {
            self.transforms.next();
            while let Some(transform) = self.transforms.item() {
                if transform
                    .block
                    .as_ref()
                    .is_some_and(|block| block.height() == 0)
                {
                    self.transforms.next();
                } else {
                    break;
                }
            }

            let transform = self.transforms.item()?;
            if transform
                .block
                .as_ref()
                .is_none_or(|block| block.is_replacement())
            {
                self.input_rows.seek(self.transforms.start().1);
            }
        }

        let transform = self.transforms.item()?;
        if transform.block.as_ref().is_none_or(|block| {
            block.is_replacement()
                && self.transforms.start().0 == self.output_row
                && matches!(block, Block::FoldedBuffer { .. }).not()
        }) {
            self.input_rows.next()
        } else {
            Some(RowInfo::default())
        }
    }
}
