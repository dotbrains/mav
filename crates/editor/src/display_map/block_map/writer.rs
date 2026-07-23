use super::*;

impl BlockMapWriterCompanion<'_> {
    pub(crate) fn companion_view(&self) -> CompanionView<'_> {
        static EMPTY_PATCH: Patch<WrapRow> = Patch::empty();
        CompanionView {
            display_map_id: self.display_map_id,
            companion_wrap_snapshot: &self.companion_wrap_snapshot,
            companion_wrap_edits: &EMPTY_PATCH,
            companion: self.companion,
        }
    }
}

impl BlockMapWriter<'_> {
    #[ztracing::instrument(skip_all)]
    pub fn insert(
        &mut self,
        blocks: impl IntoIterator<Item = BlockProperties<Anchor>>,
    ) -> Vec<CustomBlockId> {
        let blocks = blocks.into_iter();
        let mut ids = Vec::with_capacity(blocks.size_hint().1.unwrap_or(0));
        let mut edits = Patch::default();
        let wrap_snapshot = self.block_map.wrap_snapshot.borrow().clone();
        let buffer = wrap_snapshot.buffer_snapshot();

        let mut previous_wrap_row_range: Option<Range<WrapRow>> = None;
        let mut companion_blocks = Vec::new();
        for block in blocks {
            if let BlockPlacement::Replace(_) = &block.placement {
                debug_assert!(block.height.unwrap() > 0);
            }

            let id = self.block_map.insert_block_raw(block.clone(), &buffer);
            ids.push(id);

            let start = block.placement.start().to_point(&buffer);
            let end = block.placement.end().to_point(&buffer);
            let start_wrap_row = wrap_snapshot
                .make_wrap_point(Point::new(start.row, 0), Bias::Left)
                .row();
            let end_wrap_row = wrap_snapshot
                .make_wrap_point(Point::new(end.row, 0), Bias::Left)
                .row();

            let (start_row, end_row) = {
                previous_wrap_row_range.take_if(|range| {
                    !range.contains(&start_wrap_row) || !range.contains(&end_wrap_row)
                });
                let range = previous_wrap_row_range.get_or_insert_with(|| {
                    let start_row =
                        wrap_snapshot.prev_row_boundary(WrapPoint::new(start_wrap_row, 0));
                    let end_row = wrap_snapshot
                        .next_row_boundary(WrapPoint::new(end_wrap_row, 0))
                        .unwrap_or(wrap_snapshot.max_point().row() + WrapRow(1));
                    start_row..end_row
                });
                (range.start, range.end)
            };

            // Insert a matching custom block in the companion (if any)
            if let Some(companion) = &mut self.companion
                && companion.inverse.is_some()
            {
                companion_blocks.extend(balancing_block(
                    &block,
                    &buffer,
                    companion.companion_wrap_snapshot.buffer(),
                    companion.display_map_id,
                    companion.companion,
                ));
            }

            edits = edits.compose([Edit {
                old: start_row..end_row,
                new: start_row..end_row,
            }]);
        }

        self.block_map.sync(
            &wrap_snapshot,
            edits,
            self.companion
                .as_ref()
                .map(BlockMapWriterCompanion::companion_view),
        );

        if let Some(companion) = &mut self.companion
            && let Some(inverse) = &mut companion.inverse
        {
            let companion_ids = inverse.companion_writer.insert(companion_blocks);
            companion
                .companion
                .custom_block_to_balancing_block(companion.display_map_id)
                .borrow_mut()
                .extend(ids.iter().copied().zip(companion_ids));
        }

        ids
    }

    #[ztracing::instrument(skip_all)]
    pub fn resize(&mut self, mut heights: HashMap<CustomBlockId, u32>) {
        let wrap_snapshot = self.block_map.wrap_snapshot.borrow().clone();
        let buffer = wrap_snapshot.buffer_snapshot();
        let mut edits = Patch::default();
        let mut last_block_buffer_row = None;

        let mut companion_heights = HashMap::default();
        for block in &mut self.block_map.custom_blocks {
            if let Some(new_height) = heights.remove(&block.id) {
                if let BlockPlacement::Replace(_) = &block.placement {
                    debug_assert!(new_height > 0);
                }

                if block.height != Some(new_height) {
                    let new_block = CustomBlock {
                        id: block.id,
                        placement: block.placement.clone(),
                        height: Some(new_height),
                        style: block.style,
                        render: block.render.clone(),
                        priority: block.priority,
                    };
                    let new_block = Arc::new(new_block);
                    *block = new_block.clone();
                    self.block_map
                        .custom_blocks_by_id
                        .insert(block.id, new_block);

                    if let Some(companion) = &self.companion
                        && companion.inverse.is_some()
                        && let Some(companion_block_id) = companion
                            .companion
                            .custom_block_to_balancing_block(companion.display_map_id)
                            .borrow()
                            .get(&block.id)
                            .copied()
                    {
                        companion_heights.insert(companion_block_id, new_height);
                    }

                    let start_row = block.placement.start().to_point(buffer).row;
                    let end_row = block.placement.end().to_point(buffer).row;
                    if last_block_buffer_row != Some(end_row) {
                        last_block_buffer_row = Some(end_row);
                        let start_wrap_row = wrap_snapshot
                            .make_wrap_point(Point::new(start_row, 0), Bias::Left)
                            .row();
                        let end_wrap_row = wrap_snapshot
                            .make_wrap_point(Point::new(end_row, 0), Bias::Left)
                            .row();
                        let start =
                            wrap_snapshot.prev_row_boundary(WrapPoint::new(start_wrap_row, 0));
                        let end = wrap_snapshot
                            .next_row_boundary(WrapPoint::new(end_wrap_row, 0))
                            .unwrap_or(wrap_snapshot.max_point().row() + WrapRow(1));
                        edits.push(Edit {
                            old: start..end,
                            new: start..end,
                        })
                    }
                }
            }
        }

        self.block_map.sync(
            &wrap_snapshot,
            edits,
            self.companion
                .as_ref()
                .map(BlockMapWriterCompanion::companion_view),
        );
        if let Some(companion) = &mut self.companion
            && let Some(inverse) = &mut companion.inverse
        {
            inverse.companion_writer.resize(companion_heights);
        }
    }

    #[ztracing::instrument(skip_all)]
    pub fn remove(&mut self, block_ids: HashSet<CustomBlockId>) {
        let wrap_snapshot = &*self.block_map.wrap_snapshot.borrow();
        let buffer = wrap_snapshot.buffer_snapshot();
        let mut edits = Patch::default();
        let mut last_block_buffer_row = None;
        let mut previous_wrap_row_range: Option<Range<WrapRow>> = None;
        let mut companion_block_ids: HashSet<CustomBlockId> = HashSet::default();
        self.block_map.custom_blocks.retain(|block| {
            if block_ids.contains(&block.id) {
                let start = block.placement.start().to_point(buffer);
                let end = block.placement.end().to_point(buffer);
                if last_block_buffer_row != Some(end.row) {
                    last_block_buffer_row = Some(end.row);
                    let start_wrap_row = wrap_snapshot
                        .make_wrap_point(Point::new(start.row, 0), Bias::Left)
                        .row();
                    let end_wrap_row = wrap_snapshot
                        .make_wrap_point(Point::new(end.row, 0), Bias::Left)
                        .row();
                    let (start_row, end_row) = {
                        previous_wrap_row_range.take_if(|range| {
                            !range.contains(&start_wrap_row) || !range.contains(&end_wrap_row)
                        });
                        let range = previous_wrap_row_range.get_or_insert_with(|| {
                            let start_row =
                                wrap_snapshot.prev_row_boundary(WrapPoint::new(start_wrap_row, 0));
                            let end_row = wrap_snapshot
                                .next_row_boundary(WrapPoint::new(end_wrap_row, 0))
                                .unwrap_or(wrap_snapshot.max_point().row() + WrapRow(1));
                            start_row..end_row
                        });
                        (range.start, range.end)
                    };

                    edits.push(Edit {
                        old: start_row..end_row,
                        new: start_row..end_row,
                    })
                }
                if let Some(companion) = &self.companion
                    && companion.inverse.is_some()
                {
                    companion_block_ids.extend(
                        companion
                            .companion
                            .custom_block_to_balancing_block(companion.display_map_id)
                            .borrow()
                            .get(&block.id)
                            .copied(),
                    );
                }
                false
            } else {
                true
            }
        });
        self.block_map
            .custom_blocks_by_id
            .retain(|id, _| !block_ids.contains(id));

        self.block_map.sync(
            wrap_snapshot,
            edits,
            self.companion
                .as_ref()
                .map(BlockMapWriterCompanion::companion_view),
        );
        if let Some(companion) = &mut self.companion
            && let Some(inverse) = &mut companion.inverse
        {
            companion
                .companion
                .custom_block_to_balancing_block(companion.display_map_id)
                .borrow_mut()
                .retain(|id, _| !block_ids.contains(&id));
            inverse.companion_writer.remove(companion_block_ids);
        }
    }

    #[ztracing::instrument(skip_all)]
    pub fn remove_intersecting_replace_blocks(
        &mut self,
        ranges: impl IntoIterator<Item = Range<MultiBufferOffset>>,
        inclusive: bool,
    ) {
        let wrap_snapshot = self.block_map.wrap_snapshot.borrow();
        let mut blocks_to_remove = HashSet::default();
        for range in ranges {
            for block in self.blocks_intersecting_buffer_range(range, inclusive) {
                if matches!(block.placement, BlockPlacement::Replace(_)) {
                    blocks_to_remove.insert(block.id);
                }
            }
        }
        drop(wrap_snapshot);
        self.remove(blocks_to_remove);
    }

    pub fn disable_header_for_buffer(&mut self, buffer_id: BufferId) {
        self.block_map
            .buffers_with_disabled_headers
            .insert(buffer_id);
    }

    #[ztracing::instrument(skip_all)]
    pub fn fold_buffers(
        &mut self,
        buffer_ids: impl IntoIterator<Item = BufferId>,
        multi_buffer: &MultiBuffer,
        cx: &App,
    ) {
        self.fold_or_unfold_buffers(true, buffer_ids, multi_buffer, cx);
    }

    #[ztracing::instrument(skip_all)]
    pub fn unfold_buffers(
        &mut self,
        buffer_ids: impl IntoIterator<Item = BufferId>,
        multi_buffer: &MultiBuffer,
        cx: &App,
    ) {
        self.fold_or_unfold_buffers(false, buffer_ids, multi_buffer, cx);
    }

    #[ztracing::instrument(skip_all)]
    pub(crate) fn fold_or_unfold_buffers(
        &mut self,
        fold: bool,
        buffer_ids: impl IntoIterator<Item = BufferId>,
        multi_buffer: &MultiBuffer,
        cx: &App,
    ) {
        let multi_buffer_snapshot = multi_buffer.snapshot(cx);
        let mut ranges = Vec::new();
        let mut companion_buffer_ids = HashSet::default();
        for buffer_id in buffer_ids {
            if fold {
                self.block_map.folded_buffers.insert(buffer_id);
            } else {
                self.block_map.folded_buffers.remove(&buffer_id);
            }
            ranges.extend(multi_buffer_snapshot.range_for_buffer(buffer_id));
            if let Some(companion) = &self.companion
                && companion.inverse.is_some()
            {
                if let Some(diff) = multi_buffer_snapshot.diff_for_buffer_id(buffer_id) {
                    let companion_buffer_id =
                        if companion.companion.is_rhs(companion.display_map_id) {
                            diff.base_text().remote_id()
                        } else {
                            diff.buffer_id()
                        };
                    companion_buffer_ids.insert(companion_buffer_id);
                }
            }
        }
        ranges.sort_unstable_by_key(|range| range.start);

        let mut edits = Patch::default();
        let wrap_snapshot = self.block_map.wrap_snapshot.borrow().clone();
        for range in ranges {
            let last_edit_row = cmp::min(
                wrap_snapshot.make_wrap_point(range.end, Bias::Right).row() + WrapRow(1),
                wrap_snapshot.max_point().row(),
            ) + WrapRow(1);
            let range = wrap_snapshot.make_wrap_point(range.start, Bias::Left).row()..last_edit_row;
            edits.push(Edit {
                old: range.clone(),
                new: range,
            });
        }

        self.block_map.sync(
            &wrap_snapshot,
            edits.clone(),
            self.companion
                .as_ref()
                .map(BlockMapWriterCompanion::companion_view),
        );
        if let Some(companion) = &mut self.companion
            && let Some(inverse) = &mut companion.inverse
        {
            inverse.companion_writer.fold_or_unfold_buffers(
                fold,
                companion_buffer_ids,
                inverse.companion_multibuffer,
                cx,
            );
        }
    }

    #[ztracing::instrument(skip_all)]
    pub(crate) fn blocks_intersecting_buffer_range(
        &self,
        range: Range<MultiBufferOffset>,
        inclusive: bool,
    ) -> &[Arc<CustomBlock>] {
        if range.is_empty() && !inclusive {
            return &[];
        }
        let wrap_snapshot = self.block_map.wrap_snapshot.borrow();
        let buffer = wrap_snapshot.buffer_snapshot();

        let start_block_ix = match self.block_map.custom_blocks.binary_search_by(|block| {
            let block_end = block.end().to_offset(buffer);
            block_end.cmp(&range.start).then(Ordering::Greater)
        }) {
            Ok(ix) | Err(ix) => ix,
        };
        let end_block_ix =
            match self.block_map.custom_blocks[start_block_ix..].binary_search_by(|block| {
                let block_start = block.start().to_offset(buffer);
                block_start.cmp(&range.end).then(if inclusive {
                    Ordering::Less
                } else {
                    Ordering::Greater
                })
            }) {
                Ok(ix) | Err(ix) => ix,
            };

        &self.block_map.custom_blocks[start_block_ix..][..end_block_ix]
    }
}
