use super::*;

#[derive(Clone)]
pub(super) struct MultiBufferCursor<'a, MBD, BD> {
    pub(super) excerpts: Cursor<'a, 'static, Excerpt, ExcerptDimension<MBD>>,
    pub(super) diff_transforms: Cursor<'a, 'static, DiffTransform, DiffTransforms<MBD>>,
    pub(super) cached_region: OnceCell<Option<MultiBufferRegion<'a, MBD, BD>>>,
    pub(super) snapshot: &'a MultiBufferSnapshot,
}

#[derive(Clone)]
pub(super) struct MultiBufferRegion<'a, MBD, BD> {
    pub(super) buffer: &'a BufferSnapshot,
    pub(super) is_main_buffer: bool,
    pub(super) diff_hunk_status: Option<DiffHunkStatus>,
    pub(super) excerpt: &'a Excerpt,
    pub(super) buffer_range: Range<BD>,
    pub(super) range: Range<MBD>,
    pub(super) has_trailing_newline: bool,
}

impl<'a, MBD, BD> MultiBufferCursor<'a, MBD, BD>
where
    MBD: MultiBufferDimension + Ord + Sub + ops::AddAssign<<MBD as Sub>::Output>,
    BD: TextDimension + AddAssign<<MBD as Sub>::Output>,
{
    #[instrument(skip_all)]
    pub(super) fn seek(&mut self, position: &MBD) {
        let position = OutputDimension(*position);
        self.cached_region.take();
        self.diff_transforms.seek(&position, Bias::Right);
        if self.diff_transforms.item().is_none()
            && self.diff_transforms.start().output_dimension == position
        {
            self.diff_transforms.prev();
        }

        let mut excerpt_position = self.diff_transforms.start().excerpt_dimension;
        if let Some(DiffTransform::BufferContent { .. }) = self.diff_transforms.item() {
            let overshoot = position - self.diff_transforms.start().output_dimension;
            excerpt_position += overshoot;
        }

        self.excerpts.seek(&excerpt_position, Bias::Right);
        if self.excerpts.item().is_none() && excerpt_position == *self.excerpts.start() {
            self.excerpts.prev();
        }
    }

    pub(super) fn seek_forward(&mut self, position: &MBD) {
        let position = OutputDimension(*position);
        self.cached_region.take();
        self.diff_transforms.seek_forward(&position, Bias::Right);
        if self.diff_transforms.item().is_none()
            && self.diff_transforms.start().output_dimension == position
        {
            self.diff_transforms.prev();
        }

        let overshoot = position - self.diff_transforms.start().output_dimension;
        let mut excerpt_position = self.diff_transforms.start().excerpt_dimension;
        if let Some(DiffTransform::BufferContent { .. }) = self.diff_transforms.item() {
            excerpt_position += overshoot;
        }

        self.excerpts.seek_forward(&excerpt_position, Bias::Right);
        if self.excerpts.item().is_none() && excerpt_position == *self.excerpts.start() {
            self.excerpts.prev();
        }
    }

    pub(super) fn next_excerpt(&mut self) {
        self.excerpts.next();
        self.seek_to_start_of_current_excerpt();
    }

    pub(super) fn prev_excerpt(&mut self) {
        self.excerpts.prev();
        self.seek_to_start_of_current_excerpt();
    }

    pub(super) fn seek_to_start_of_current_excerpt(&mut self) {
        self.cached_region.take();

        if self.diff_transforms.seek(self.excerpts.start(), Bias::Left)
            && self.diff_transforms.start().excerpt_dimension < *self.excerpts.start()
            && self.diff_transforms.next_item().is_some()
        {
            self.diff_transforms.next();
        }
    }

    pub(super) fn next_excerpt_forwards(&mut self) {
        self.excerpts.next();
        self.seek_to_start_of_current_excerpt_forward();
    }

    pub(super) fn seek_to_start_of_current_excerpt_forward(&mut self) {
        self.cached_region.take();

        if self
            .diff_transforms
            .seek_forward(self.excerpts.start(), Bias::Left)
            && self.diff_transforms.start().excerpt_dimension < *self.excerpts.start()
            && self.diff_transforms.next_item().is_some()
        {
            self.diff_transforms.next();
        }
    }

    pub(super) fn next(&mut self) {
        self.cached_region.take();
        match self
            .diff_transforms
            .end()
            .excerpt_dimension
            .cmp(&self.excerpts.end())
        {
            cmp::Ordering::Less => self.diff_transforms.next(),
            cmp::Ordering::Greater => self.excerpts.next(),
            cmp::Ordering::Equal => {
                self.diff_transforms.next();
                if self.diff_transforms.end().excerpt_dimension > self.excerpts.end()
                    || self.diff_transforms.item().is_none()
                {
                    self.excerpts.next();
                } else if let Some(DiffTransform::DeletedHunk { hunk_info, .. }) =
                    self.diff_transforms.item()
                    && self
                        .excerpts
                        .item()
                        .is_some_and(|excerpt| excerpt.end_anchor() != hunk_info.excerpt_end)
                {
                    self.excerpts.next();
                }
            }
        }
    }

    pub(super) fn prev(&mut self) {
        self.cached_region.take();
        match self
            .diff_transforms
            .start()
            .excerpt_dimension
            .cmp(self.excerpts.start())
        {
            cmp::Ordering::Less => self.excerpts.prev(),
            cmp::Ordering::Greater => self.diff_transforms.prev(),
            cmp::Ordering::Equal => {
                self.diff_transforms.prev();
                if self.diff_transforms.start().excerpt_dimension < *self.excerpts.start()
                    || self.diff_transforms.item().is_none()
                {
                    self.excerpts.prev();
                }
            }
        }
    }

    pub(super) fn region(&self) -> Option<&MultiBufferRegion<'a, MBD, BD>> {
        self.cached_region
            .get_or_init(|| self.build_region())
            .as_ref()
    }

    pub(super) fn is_at_start_of_excerpt(&mut self) -> bool {
        if self.diff_transforms.start().excerpt_dimension > *self.excerpts.start() {
            return false;
        } else if self.diff_transforms.start().excerpt_dimension < *self.excerpts.start() {
            return true;
        }

        self.diff_transforms.prev();
        let prev_transform = self.diff_transforms.item();
        self.diff_transforms.next();

        prev_transform.is_none_or(|next_transform| {
            matches!(next_transform, DiffTransform::BufferContent { .. })
        })
    }

    pub(super) fn is_at_end_of_excerpt(&self) -> bool {
        if self.diff_transforms.end().excerpt_dimension < self.excerpts.end() {
            return false;
        } else if self.diff_transforms.end().excerpt_dimension > self.excerpts.end()
            || self.diff_transforms.item().is_none()
        {
            return true;
        }

        let next_transform = self.diff_transforms.next_item();
        next_transform.is_none_or(|next_transform| match next_transform {
            DiffTransform::BufferContent { .. } => true,
            DiffTransform::DeletedHunk { hunk_info, .. } => self
                .excerpts
                .item()
                .is_some_and(|excerpt| excerpt.end_anchor() != hunk_info.excerpt_end),
        })
    }

    pub(super) fn main_buffer_position(&self) -> Option<BD> {
        let excerpt = self.excerpts.item()?;
        let buffer = excerpt.buffer_snapshot(self.snapshot);
        let buffer_context_start = excerpt.range.context.start.summary::<BD>(buffer);
        let mut buffer_start = buffer_context_start;
        let overshoot = self.diff_transforms.end().excerpt_dimension - *self.excerpts.start();
        buffer_start += overshoot;
        Some(buffer_start)
    }

    pub(super) fn buffer_position_at(&self, output_position: &MBD) -> Option<BD> {
        let excerpt = self.excerpts.item()?;
        let buffer = excerpt.buffer_snapshot(self.snapshot);
        let buffer_context_start = excerpt.range.context.start.summary::<BD>(buffer);
        let mut excerpt_offset = self.diff_transforms.start().excerpt_dimension;
        if let Some(DiffTransform::BufferContent { .. }) = self.diff_transforms.item() {
            excerpt_offset += *output_position - self.diff_transforms.start().output_dimension.0;
        }
        let mut result = buffer_context_start;
        result += excerpt_offset - *self.excerpts.start();
        Some(result)
    }

    pub(super) fn build_region(&self) -> Option<MultiBufferRegion<'a, MBD, BD>> {
        let excerpt = self.excerpts.item()?;
        match self.diff_transforms.item()? {
            DiffTransform::DeletedHunk {
                buffer_id,
                base_text_byte_range,
                has_trailing_newline,
                hunk_info,
                ..
            } => {
                let diff = find_diff_state(&self.snapshot.diffs, *buffer_id)?;
                let buffer = diff.base_text();
                let mut rope_cursor = buffer.as_rope().cursor(0);
                let buffer_start = rope_cursor.summary::<BD>(base_text_byte_range.start);
                let buffer_range_len = rope_cursor.summary::<BD>(base_text_byte_range.end);
                let mut buffer_end = buffer_start;
                TextDimension::add_assign(&mut buffer_end, &buffer_range_len);
                let start = self.diff_transforms.start().output_dimension.0;
                let end = self.diff_transforms.end().output_dimension.0;
                Some(MultiBufferRegion {
                    buffer,
                    excerpt,
                    has_trailing_newline: *has_trailing_newline,
                    is_main_buffer: false,
                    diff_hunk_status: Some(DiffHunkStatus::deleted(
                        hunk_info.hunk_secondary_status,
                    )),
                    buffer_range: buffer_start..buffer_end,
                    range: start..end,
                })
            }
            DiffTransform::BufferContent {
                inserted_hunk_info, ..
            } => {
                let buffer = excerpt.buffer_snapshot(self.snapshot);
                let buffer_context_start = excerpt.range.context.start.summary::<BD>(buffer);

                let mut start = self.diff_transforms.start().output_dimension.0;
                let mut buffer_start = buffer_context_start;
                if self.diff_transforms.start().excerpt_dimension < *self.excerpts.start() {
                    let overshoot =
                        *self.excerpts.start() - self.diff_transforms.start().excerpt_dimension;
                    start += overshoot;
                } else {
                    let overshoot =
                        self.diff_transforms.start().excerpt_dimension - *self.excerpts.start();
                    buffer_start += overshoot;
                }

                let mut end;
                let mut buffer_end;
                let has_trailing_newline;
                let transform_end = self.diff_transforms.end();
                if transform_end.excerpt_dimension < self.excerpts.end() {
                    let overshoot = transform_end.excerpt_dimension - *self.excerpts.start();
                    end = transform_end.output_dimension.0;
                    buffer_end = buffer_context_start;
                    buffer_end += overshoot;
                    has_trailing_newline = false;
                } else {
                    let overshoot =
                        self.excerpts.end() - self.diff_transforms.start().excerpt_dimension;
                    end = self.diff_transforms.start().output_dimension.0;
                    end += overshoot;
                    buffer_end = excerpt.range.context.end.summary::<BD>(buffer);
                    has_trailing_newline = excerpt.has_trailing_newline;
                };

                let diff_hunk_status = inserted_hunk_info.map(|info| {
                    if info.is_logically_deleted {
                        DiffHunkStatus::deleted(info.hunk_secondary_status)
                    } else {
                        DiffHunkStatus::added(info.hunk_secondary_status)
                    }
                });

                Some(MultiBufferRegion {
                    buffer,
                    excerpt,
                    has_trailing_newline,
                    is_main_buffer: true,
                    diff_hunk_status,
                    buffer_range: buffer_start..buffer_end,
                    range: start..end,
                })
            }
        }
    }

    pub(super) fn fetch_excerpt_with_range(&self) -> Option<(&'a Excerpt, Range<MBD>)> {
        let excerpt = self.excerpts.item()?;
        match self.diff_transforms.item()? {
            &DiffTransform::DeletedHunk { .. } => {
                let start = self.diff_transforms.start().output_dimension.0;
                let end = self.diff_transforms.end().output_dimension.0;
                Some((excerpt, start..end))
            }
            DiffTransform::BufferContent { .. } => {
                let mut start = self.diff_transforms.start().output_dimension.0;
                if self.diff_transforms.start().excerpt_dimension < *self.excerpts.start() {
                    let overshoot =
                        *self.excerpts.start() - self.diff_transforms.start().excerpt_dimension;
                    start += overshoot;
                }

                let mut end;
                let transform_end = self.diff_transforms.end();
                if transform_end.excerpt_dimension < self.excerpts.end() {
                    end = transform_end.output_dimension.0;
                } else {
                    let overshoot =
                        self.excerpts.end() - self.diff_transforms.start().excerpt_dimension;
                    end = self.diff_transforms.start().output_dimension.0;
                    end += overshoot;
                };

                Some((excerpt, start..end))
            }
        }
    }

    pub(super) fn excerpt(&self) -> Option<&'a Excerpt> {
        self.excerpts.item()
    }
}
