use super::*;

pub struct MultiBufferChunks<'a> {
    pub(super) excerpts: Cursor<'a, 'static, Excerpt, ExcerptOffset>,
    pub(super) diff_transforms:
        Cursor<'a, 'static, DiffTransform, Dimensions<MultiBufferOffset, ExcerptOffset>>,
    pub(super) diff_base_chunks: Option<(BufferId, BufferChunks<'a>)>,
    pub(super) buffer_chunk: Option<Chunk<'a>>,
    pub(super) range: Range<MultiBufferOffset>,
    pub(super) excerpt_offset_range: Range<ExcerptOffset>,
    pub(super) excerpt_chunks: Option<ExcerptChunks<'a>>,
    pub(super) language_aware: LanguageAwareStyling,
    pub(super) snapshot: &'a MultiBufferSnapshot,
}

pub struct ReversedMultiBufferChunks<'a> {
    pub(super) cursor: MultiBufferCursor<'a, MultiBufferOffset, BufferOffset>,
    pub(super) current_chunks: Option<rope::Chunks<'a>>,
    pub(super) start: MultiBufferOffset,
    pub(super) offset: MultiBufferOffset,
}

impl<'a> MultiBufferChunks<'a> {
    pub fn offset(&self) -> MultiBufferOffset {
        self.range.start
    }

    pub fn seek(&mut self, range: Range<MultiBufferOffset>) {
        self.diff_transforms.seek(&range.end, Bias::Right);
        let mut excerpt_end = self.diff_transforms.start().1;
        if let Some(DiffTransform::BufferContent { .. }) = self.diff_transforms.item() {
            let overshoot = range.end - self.diff_transforms.start().0;
            excerpt_end += overshoot;
        }

        self.diff_transforms.seek(&range.start, Bias::Right);
        let mut excerpt_start = self.diff_transforms.start().1;
        if let Some(DiffTransform::BufferContent { .. }) = self.diff_transforms.item() {
            let overshoot = range.start - self.diff_transforms.start().0;
            excerpt_start += overshoot;
        }

        self.seek_to_excerpt_offset_range(excerpt_start..excerpt_end);
        self.buffer_chunk.take();
        self.range = range;
    }

    fn seek_to_excerpt_offset_range(&mut self, new_range: Range<ExcerptOffset>) {
        self.excerpt_offset_range = new_range.clone();
        self.excerpts.seek(&new_range.start, Bias::Right);
        if let Some(excerpt) = self.excerpts.item() {
            let excerpt_start = *self.excerpts.start();
            if let Some(excerpt_chunks) = self
                .excerpt_chunks
                .as_mut()
                .filter(|chunks| excerpt.end_anchor() == chunks.end)
            {
                excerpt.seek_chunks(
                    excerpt_chunks,
                    (self.excerpt_offset_range.start - excerpt_start)
                        ..(self.excerpt_offset_range.end - excerpt_start),
                    self.snapshot,
                );
            } else {
                self.excerpt_chunks = Some(excerpt.chunks_in_range(
                    (self.excerpt_offset_range.start - excerpt_start)
                        ..(self.excerpt_offset_range.end - excerpt_start),
                    self.language_aware,
                    self.snapshot,
                ));
            }
        } else {
            self.excerpt_chunks = None;
        }
    }

    #[ztracing::instrument(skip_all)]
    fn next_excerpt_chunk(&mut self) -> Option<Chunk<'a>> {
        loop {
            if self.excerpt_offset_range.is_empty() {
                return None;
            } else if let Some(chunk) = self.excerpt_chunks.as_mut()?.next() {
                self.excerpt_offset_range.start += chunk.text.len();
                return Some(chunk);
            } else {
                self.excerpts.next();
                let excerpt = self.excerpts.item()?;
                self.excerpt_chunks = Some(excerpt.chunks_in_range(
                    0..(self.excerpt_offset_range.end - *self.excerpts.start()),
                    self.language_aware,
                    self.snapshot,
                ));
            }
        }
    }
}

impl<'a> Iterator for ReversedMultiBufferChunks<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        let mut region = self.cursor.region()?;
        if self.offset == region.range.start {
            self.cursor.prev();
            region = self.cursor.region()?;
            let start_overshoot = self.start.saturating_sub(region.range.start);
            self.current_chunks = Some(region.buffer.reversed_chunks_in_range(
                region.buffer_range.start + start_overshoot..region.buffer_range.end,
            ));
        }

        if self.offset == region.range.end && region.has_trailing_newline {
            self.offset -= 1;
            Some("\n")
        } else {
            let chunk = self.current_chunks.as_mut().unwrap().next()?;
            self.offset -= chunk.len();
            Some(chunk)
        }
    }
}

impl<'a> Iterator for MultiBufferChunks<'a> {
    type Item = Chunk<'a>;

    #[ztracing::instrument(skip_all)]
    fn next(&mut self) -> Option<Chunk<'a>> {
        if self.range.start >= self.range.end {
            return None;
        }
        while self
            .diff_transforms
            .item()
            .is_some_and(|_| self.range.start >= self.diff_transforms.end().0)
        {
            self.diff_transforms.next();
        }

        let diff_transform_start = self.diff_transforms.start().0;
        let diff_transform_end = self.diff_transforms.end().0;
        debug_assert!(
            self.range.start < diff_transform_end,
            "{:?} < {:?} of ({1:?}..{2:?})",
            self.range.start,
            diff_transform_end,
            diff_transform_start
        );

        let diff_transform = self.diff_transforms.item()?;
        match diff_transform {
            DiffTransform::BufferContent { .. } => {
                let chunk = if let Some(chunk) = &mut self.buffer_chunk {
                    chunk
                } else {
                    let chunk = self.next_excerpt_chunk().unwrap();
                    self.buffer_chunk.insert(chunk)
                };

                let chunk_end = self.range.start + chunk.text.len();
                let diff_transform_end = diff_transform_end.min(self.range.end);

                let split_idx = if diff_transform_end < chunk_end {
                    chunk
                        .text
                        .ceil_char_boundary(diff_transform_end - self.range.start)
                } else {
                    chunk.text.len()
                };

                if split_idx < chunk.text.len() {
                    let (before, after) = chunk.text.split_at(split_idx);
                    self.range.start += split_idx;
                    let mask = 1u128.unbounded_shl(split_idx as u32).wrapping_sub(1);
                    let chars = chunk.chars & mask;
                    let tabs = chunk.tabs & mask;
                    let newlines = chunk.newlines & mask;

                    chunk.text = after;
                    chunk.chars = chunk.chars >> split_idx;
                    chunk.tabs = chunk.tabs >> split_idx;
                    chunk.newlines = chunk.newlines >> split_idx;

                    Some(Chunk {
                        text: before,
                        chars,
                        tabs,
                        newlines,
                        ..chunk.clone()
                    })
                } else {
                    self.range.start = chunk_end;
                    self.buffer_chunk.take()
                }
            }
            DiffTransform::DeletedHunk {
                buffer_id,
                base_text_byte_range,
                has_trailing_newline,
                ..
            } => {
                let base_text_start =
                    base_text_byte_range.start + (self.range.start - diff_transform_start);
                let base_text_end =
                    base_text_byte_range.start + (self.range.end - diff_transform_start);
                let base_text_end = base_text_end.min(base_text_byte_range.end);

                let mut chunks = if let Some((_, mut chunks)) = self
                    .diff_base_chunks
                    .take()
                    .filter(|(id, _)| id == buffer_id)
                {
                    if chunks.range().start != base_text_start || chunks.range().end < base_text_end
                    {
                        chunks.seek(base_text_start..base_text_end);
                    }
                    chunks
                } else {
                    let base_buffer =
                        &find_diff_state(&self.snapshot.diffs, *buffer_id)?.base_text();
                    base_buffer.chunks(base_text_start..base_text_end, self.language_aware)
                };

                let chunk = if let Some(chunk) = chunks.next() {
                    self.range.start += chunk.text.len();
                    self.diff_base_chunks = Some((*buffer_id, chunks));
                    chunk
                } else {
                    debug_assert!(has_trailing_newline);
                    self.range.start += "\n".len();
                    Chunk {
                        text: "\n",
                        chars: 1u128,
                        newlines: 1u128,
                        ..Default::default()
                    }
                };
                Some(chunk)
            }
        }
    }
}
