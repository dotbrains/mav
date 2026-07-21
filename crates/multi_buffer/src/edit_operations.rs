use super::*;

impl MultiBuffer {
    pub fn edit<I, S, T>(
        &mut self,
        edits: I,
        autoindent_mode: Option<AutoindentMode>,
        cx: &mut Context<Self>,
    ) where
        I: IntoIterator<Item = (Range<S>, T)>,
        S: ToOffset,
        T: Into<Arc<str>>,
    {
        self.edit_internal(edits, autoindent_mode, true, cx);
    }

    pub fn edit_non_coalesce<I, S, T>(
        &mut self,
        edits: I,
        autoindent_mode: Option<AutoindentMode>,
        cx: &mut Context<Self>,
    ) where
        I: IntoIterator<Item = (Range<S>, T)>,
        S: ToOffset,
        T: Into<Arc<str>>,
    {
        self.edit_internal(edits, autoindent_mode, false, cx);
    }

    fn edit_internal<I, S, T>(
        &mut self,
        edits: I,
        autoindent_mode: Option<AutoindentMode>,
        coalesce_adjacent: bool,
        cx: &mut Context<Self>,
    ) where
        I: IntoIterator<Item = (Range<S>, T)>,
        S: ToOffset,
        T: Into<Arc<str>>,
    {
        if self.read_only() || self.buffers.is_empty() {
            return;
        }
        self.sync_mut(cx);
        let edits = edits
            .into_iter()
            .map(|(range, new_text)| {
                let mut range = range.start.to_offset(self.snapshot.get_mut())
                    ..range.end.to_offset(self.snapshot.get_mut());
                if range.start > range.end {
                    mem::swap(&mut range.start, &mut range.end);
                }
                (range, new_text.into())
            })
            .collect::<Vec<_>>();

        return edit_internal(self, edits, autoindent_mode, coalesce_adjacent, cx);

        // Non-generic part of edit, hoisted out to avoid blowing up LLVM IR.
        fn edit_internal(
            this: &mut MultiBuffer,
            edits: Vec<(Range<MultiBufferOffset>, Arc<str>)>,
            mut autoindent_mode: Option<AutoindentMode>,
            coalesce_adjacent: bool,
            cx: &mut Context<MultiBuffer>,
        ) {
            let original_indent_columns = match &mut autoindent_mode {
                Some(AutoindentMode::Block {
                    original_indent_columns,
                }) => mem::take(original_indent_columns),
                _ => Default::default(),
            };

            let buffer_edits = MultiBuffer::convert_edits_to_buffer_edits(
                edits,
                this.snapshot.get_mut(),
                &original_indent_columns,
            );

            let mut buffer_ids = Vec::with_capacity(buffer_edits.len());
            for (buffer_id, mut edits) in buffer_edits {
                buffer_ids.push(buffer_id);
                edits.sort_by_key(|edit| edit.range.start);
                this.buffers[&buffer_id].buffer.update(cx, |buffer, cx| {
                    let mut edits = edits.into_iter().peekable();
                    let mut insertions = Vec::new();
                    let mut original_indent_columns = Vec::new();
                    let mut deletions = Vec::new();
                    let empty_str: Arc<str> = Arc::default();
                    while let Some(BufferEdit {
                        mut range,
                        mut new_text,
                        mut is_insertion,
                        original_indent_column,
                    }) = edits.next()
                    {
                        while let Some(BufferEdit {
                            range: next_range,
                            is_insertion: next_is_insertion,
                            new_text: next_new_text,
                            ..
                        }) = edits.peek()
                        {
                            let should_coalesce = if coalesce_adjacent {
                                range.end >= next_range.start
                            } else {
                                range.end > next_range.start
                            };

                            if should_coalesce {
                                range.end = cmp::max(next_range.end, range.end);
                                is_insertion |= *next_is_insertion;
                                new_text = format!("{new_text}{next_new_text}").into();
                                edits.next();
                            } else {
                                break;
                            }
                        }

                        if is_insertion {
                            original_indent_columns.push(original_indent_column);
                            insertions.push((
                                buffer.anchor_before(range.start)..buffer.anchor_before(range.end),
                                new_text.clone(),
                            ));
                        } else if !range.is_empty() {
                            deletions.push((
                                buffer.anchor_before(range.start)..buffer.anchor_before(range.end),
                                empty_str.clone(),
                            ));
                        }
                    }

                    let deletion_autoindent_mode =
                        if let Some(AutoindentMode::Block { .. }) = autoindent_mode {
                            Some(AutoindentMode::Block {
                                original_indent_columns: Default::default(),
                            })
                        } else {
                            autoindent_mode.clone()
                        };
                    let insertion_autoindent_mode =
                        if let Some(AutoindentMode::Block { .. }) = autoindent_mode {
                            Some(AutoindentMode::Block {
                                original_indent_columns,
                            })
                        } else {
                            autoindent_mode.clone()
                        };

                    if coalesce_adjacent {
                        buffer.edit(deletions, deletion_autoindent_mode, cx);
                        buffer.edit(insertions, insertion_autoindent_mode, cx);
                    } else {
                        buffer.edit_non_coalesce(deletions, deletion_autoindent_mode, cx);
                        buffer.edit_non_coalesce(insertions, insertion_autoindent_mode, cx);
                    }
                })
            }

            cx.emit(Event::BuffersEdited { buffer_ids });
        }
    }

    pub(super) fn convert_edits_to_buffer_edits(
        edits: Vec<(Range<MultiBufferOffset>, Arc<str>)>,
        snapshot: &MultiBufferSnapshot,
        original_indent_columns: &[Option<u32>],
    ) -> HashMap<BufferId, Vec<BufferEdit>> {
        let mut buffer_edits: HashMap<BufferId, Vec<BufferEdit>> = Default::default();
        let mut cursor = snapshot.cursor::<MultiBufferOffset, BufferOffset>();
        for (ix, (range, new_text)) in edits.into_iter().enumerate() {
            let original_indent_column = original_indent_columns.get(ix).copied().flatten();

            cursor.seek(&range.start);
            let mut start_region = cursor.region().expect("start offset out of bounds");
            if !start_region.is_main_buffer {
                cursor.next();
                if let Some(region) = cursor.region() {
                    start_region = region;
                } else {
                    continue;
                }
            }

            if range.end < start_region.range.start {
                continue;
            }

            let start_region = start_region.clone();
            if range.end > start_region.range.end {
                cursor.seek_forward(&range.end);
            }
            let mut end_region = cursor.region().expect("end offset out of bounds");
            if !end_region.is_main_buffer {
                cursor.prev();
                if let Some(region) = cursor.region() {
                    end_region = region;
                } else {
                    continue;
                }
            }

            if range.start > end_region.range.end {
                continue;
            }

            let start_overshoot = range.start.saturating_sub(start_region.range.start);
            let end_overshoot = range.end.saturating_sub(end_region.range.start);
            let buffer_start = (start_region.buffer_range.start + start_overshoot)
                .min(start_region.buffer_range.end);
            let buffer_end =
                (end_region.buffer_range.start + end_overshoot).min(end_region.buffer_range.end);

            if start_region.excerpt == end_region.excerpt {
                if start_region.buffer.capability == Capability::ReadWrite
                    && start_region.is_main_buffer
                {
                    buffer_edits
                        .entry(start_region.buffer.remote_id())
                        .or_default()
                        .push(BufferEdit {
                            range: buffer_start..buffer_end,
                            new_text,
                            is_insertion: true,
                            original_indent_column,
                        });
                }
            } else {
                let start_excerpt_range = buffer_start..start_region.buffer_range.end;
                let end_excerpt_range = end_region.buffer_range.start..buffer_end;
                if start_region.buffer.capability == Capability::ReadWrite
                    && start_region.is_main_buffer
                {
                    buffer_edits
                        .entry(start_region.buffer.remote_id())
                        .or_default()
                        .push(BufferEdit {
                            range: start_excerpt_range,
                            new_text: new_text.clone(),
                            is_insertion: true,
                            original_indent_column,
                        });
                }
                if end_region.buffer.capability == Capability::ReadWrite
                    && end_region.is_main_buffer
                {
                    buffer_edits
                        .entry(end_region.buffer.remote_id())
                        .or_default()
                        .push(BufferEdit {
                            range: end_excerpt_range,
                            new_text: new_text.clone(),
                            is_insertion: false,
                            original_indent_column,
                        });
                }
                let end_region_excerpt = end_region.excerpt.clone();

                cursor.seek(&range.start);
                cursor.next_excerpt();
                while let Some(region) = cursor.region() {
                    if region.excerpt == &end_region_excerpt {
                        break;
                    }
                    if region.buffer.capability == Capability::ReadWrite && region.is_main_buffer {
                        buffer_edits
                            .entry(region.buffer.remote_id())
                            .or_default()
                            .push(BufferEdit {
                                range: region.buffer_range.clone(),
                                new_text: new_text.clone(),
                                is_insertion: false,
                                original_indent_column,
                            });
                    }
                    cursor.next_excerpt();
                }
            }
        }
        buffer_edits
    }

    pub fn autoindent_ranges<I, S>(&mut self, ranges: I, cx: &mut Context<Self>)
    where
        I: IntoIterator<Item = Range<S>>,
        S: ToOffset,
    {
        if self.read_only() || self.buffers.is_empty() {
            return;
        }
        self.sync_mut(cx);
        let empty = Arc::<str>::from("");
        let edits = ranges
            .into_iter()
            .map(|range| {
                let mut range = range.start.to_offset(self.snapshot.get_mut())
                    ..range.end.to_offset(&self.snapshot.get_mut());
                if range.start > range.end {
                    mem::swap(&mut range.start, &mut range.end);
                }
                (range, empty.clone())
            })
            .collect::<Vec<_>>();

        return autoindent_ranges_internal(self, edits, cx);

        fn autoindent_ranges_internal(
            this: &mut MultiBuffer,
            edits: Vec<(Range<MultiBufferOffset>, Arc<str>)>,
            cx: &mut Context<MultiBuffer>,
        ) {
            let buffer_edits =
                MultiBuffer::convert_edits_to_buffer_edits(edits, this.snapshot.get_mut(), &[]);

            let mut buffer_ids = Vec::new();
            for (buffer_id, mut edits) in buffer_edits {
                buffer_ids.push(buffer_id);
                edits.sort_unstable_by_key(|edit| edit.range.start);

                let mut ranges: Vec<Range<BufferOffset>> = Vec::new();
                for edit in edits {
                    if let Some(last_range) = ranges.last_mut()
                        && edit.range.start <= last_range.end
                    {
                        last_range.end = last_range.end.max(edit.range.end);
                        continue;
                    }
                    ranges.push(edit.range);
                }

                this.buffers[&buffer_id].buffer.update(cx, |buffer, cx| {
                    buffer.autoindent_ranges(ranges, cx);
                })
            }

            cx.emit(Event::BuffersEdited { buffer_ids });
        }
    }
}
