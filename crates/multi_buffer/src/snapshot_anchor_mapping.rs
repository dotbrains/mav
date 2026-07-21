use super::*;

impl MultiBufferSnapshot {
    pub fn anchor_before<T: ToOffset>(&self, position: T) -> Anchor {
        self.anchor_at(position, Bias::Left)
    }

    pub fn anchor_after<T: ToOffset>(&self, position: T) -> Anchor {
        self.anchor_at(position, Bias::Right)
    }

    pub fn anchor_at<T: ToOffset>(&self, position: T, mut bias: Bias) -> Anchor {
        let offset = position.to_offset(self);

        // Find the given position in the diff transforms. Determine the corresponding
        // offset in the excerpts, and whether the position is within a deleted hunk.
        let mut diff_transforms = self
            .diff_transforms
            .cursor::<Dimensions<MultiBufferOffset, ExcerptOffset>>(());
        diff_transforms.seek(&offset, Bias::Right);

        if offset == diff_transforms.start().0
            && bias == Bias::Left
            && let Some(prev_item) = diff_transforms.prev_item()
            && let DiffTransform::DeletedHunk { .. } = prev_item
        {
            diff_transforms.prev();
        }
        let offset_in_transform = offset - diff_transforms.start().0;
        let mut excerpt_offset = diff_transforms.start().1;
        let mut diff_base_anchor = None;
        if let Some(DiffTransform::DeletedHunk {
            buffer_id,
            base_text_byte_range,
            has_trailing_newline,
            ..
        }) = diff_transforms.item()
        {
            let diff = self.diff_state(*buffer_id).expect("missing diff");
            if offset_in_transform > base_text_byte_range.len() {
                debug_assert!(*has_trailing_newline);
                bias = Bias::Right;
            } else {
                diff_base_anchor = Some(
                    diff.base_text()
                        .anchor_at(base_text_byte_range.start + offset_in_transform, bias),
                );
                bias = Bias::Left;
            }
        } else {
            excerpt_offset += MultiBufferOffset(offset_in_transform);
        };

        let mut excerpts = self
            .excerpts
            .cursor::<Dimensions<ExcerptOffset, ExcerptSummary>>(());
        excerpts.seek(&excerpt_offset, Bias::Right);
        if excerpts.item().is_none() && excerpt_offset == excerpts.start().0 && bias == Bias::Left {
            excerpts.prev();
        }
        if let Some(excerpt) = excerpts.item() {
            let buffer_snapshot = excerpt.buffer_snapshot(self);
            let mut overshoot = excerpt_offset.saturating_sub(excerpts.start().0);
            if excerpt.has_trailing_newline && excerpt_offset == excerpts.end().0 {
                overshoot -= 1;
                bias = Bias::Right;
            }

            let buffer_start = excerpt.range.context.start.to_offset(&buffer_snapshot);
            let text_anchor = excerpt.clip_anchor(
                buffer_snapshot.anchor_at(buffer_start + overshoot, bias),
                self,
            );
            let anchor = ExcerptAnchor::in_buffer(excerpt.path_key_index, text_anchor);
            let anchor = match diff_base_anchor {
                Some(diff_base_anchor) => anchor.with_diff_base_anchor(diff_base_anchor),
                None => anchor,
            };
            anchor.into()
        } else if excerpt_offset == ExcerptDimension(MultiBufferOffset::ZERO) && bias == Bias::Left
        {
            Anchor::Min
        } else {
            Anchor::Max
        }
    }

    /// Lifts a buffer anchor to a multibuffer anchor without checking against excerpt boundaries. Returns `None` if there are no excerpts for the buffer
    pub fn anchor_in_buffer(&self, anchor: text::Anchor) -> Option<Anchor> {
        let path_key_index = self.path_key_index_for_buffer(anchor.buffer_id)?;
        Some(Anchor::in_buffer(path_key_index, anchor))
    }

    /// Lifts a buffer anchor range to a multibuffer anchor range without checking against excerpt boundaries. Returns `None` if there are no excerpts for the buffer.
    pub fn anchor_range_in_buffer(&self, range: Range<text::Anchor>) -> Option<Range<Anchor>> {
        if range.start.buffer_id != range.end.buffer_id {
            return None;
        }

        let path_key_index = self.path_key_index_for_buffer(range.start.buffer_id)?;
        Some(Anchor::range_in_buffer(path_key_index, range))
    }

    /// Creates a multibuffer anchor for the given buffer anchor, if it is contained in any excerpt.
    pub fn anchor_in_excerpt(&self, text_anchor: text::Anchor) -> Option<Anchor> {
        let excerpts = {
            let buffer_id = text_anchor.buffer_id;
            if let Some(buffer_state) = self.buffers.get(&buffer_id) {
                let path_key = buffer_state.path_key.clone();
                let mut cursor = self.excerpts.cursor::<PathKey>(());
                cursor.seek_forward(&path_key, Bias::Left);
                Some(iter::from_fn(move || {
                    let excerpt = cursor.item()?;
                    if excerpt.path_key != path_key {
                        return None;
                    }
                    cursor.next();
                    Some(excerpt)
                }))
            } else {
                None
            }
            .into_iter()
            .flatten()
        };
        for excerpt in excerpts {
            let buffer_snapshot = excerpt.buffer_snapshot(self);
            if excerpt.range.contains(&text_anchor, &buffer_snapshot) {
                return Some(Anchor::in_buffer(excerpt.path_key_index, text_anchor));
            }
        }

        None
    }

    /// Creates a multibuffer anchor for the given buffer anchor, if it is contained in any excerpt.
    pub fn buffer_anchor_range_to_anchor_range(
        &self,
        text_anchor: Range<text::Anchor>,
    ) -> Option<Range<Anchor>> {
        if self.is_singleton() {
            let excerpt = self.excerpts.first()?;
            let buffer_snapshot = excerpt.buffer_snapshot(self);
            if excerpt.range.contains(&text_anchor.start, &buffer_snapshot)
                && excerpt.range.contains(&text_anchor.end, &buffer_snapshot)
            {
                return Some(Anchor::range_in_buffer(excerpt.path_key_index, text_anchor));
            }
        }

        // for each search match

        let mut buffer_snapshot = None;
        for excerpt in {
            let this = &self;
            let buffer_id = text_anchor.start.buffer_id;
            if let Some(buffer_state) = this.buffers.get(&buffer_id) {
                let path_key = buffer_state.path_key.clone();
                let mut cursor = this.excerpts.cursor::<PathKey>(());
                cursor.seek_forward(&path_key, Bias::Left);
                Some(iter::from_fn(move || {
                    let excerpt = cursor.item()?;
                    if excerpt.path_key != path_key {
                        return None;
                    }
                    cursor.next();
                    Some(excerpt)
                }))
            } else {
                None
            }
            .into_iter()
            .flatten()
        } {
            let buffer_snapshot =
                buffer_snapshot.get_or_insert_with(|| excerpt.buffer_snapshot(self));
            if excerpt.range.contains(&text_anchor.start, &buffer_snapshot)
                && excerpt.range.contains(&text_anchor.end, &buffer_snapshot)
            {
                return Some(Anchor::range_in_buffer(excerpt.path_key_index, text_anchor));
            }
        }

        None
    }

    /// Returns a buffer anchor and its buffer snapshot for the given anchor, if it is in the multibuffer.
    pub fn anchor_to_buffer_anchor(
        &self,
        anchor: Anchor,
    ) -> Option<(text::Anchor, &BufferSnapshot)> {
        match anchor {
            Anchor::Min => {
                let excerpt = self.excerpts.first()?;
                let buffer = excerpt.buffer_snapshot(self);
                Some((excerpt.range.context.start, buffer))
            }
            Anchor::Excerpt(excerpt_anchor) => {
                let buffer = self.buffer_for_id(excerpt_anchor.buffer_id())?;
                Some((excerpt_anchor.text_anchor, buffer))
            }
            Anchor::Max => {
                let excerpt = self.excerpts.last()?;
                let buffer = excerpt.buffer_snapshot(self);
                Some((excerpt.range.context.end, buffer))
            }
        }
    }

    pub fn can_resolve(&self, anchor: &Anchor) -> bool {
        match anchor {
            // todo(lw): should be `!self.is_empty()`
            Anchor::Min | Anchor::Max => true,
            Anchor::Excerpt(excerpt_anchor) => {
                let Some(target) = excerpt_anchor.try_seek_target(self) else {
                    return false;
                };
                let mut cursor = self.excerpts.cursor::<ExcerptSummary>(());
                cursor.seek(&target, Bias::Left);
                let Some(excerpt) = cursor.item() else {
                    return false;
                };
                excerpt
                    .buffer_snapshot(self)
                    .can_resolve(&excerpt_anchor.text_anchor())
            }
        }
    }

    pub fn excerpts(&self) -> impl Iterator<Item = ExcerptRange<text::Anchor>> {
        self.excerpts.iter().map(|excerpt| excerpt.range.clone())
    }
}
