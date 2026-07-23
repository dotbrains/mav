use super::*;

impl DisplayMap {
    pub fn fold<T: Clone + ToOffset>(&mut self, creases: Vec<Crease<T>>, cx: &mut Context<Self>) {
        if self.companion().is_some() {
            return;
        }

        let buffer_snapshot = self.buffer.read(cx).snapshot(cx);
        let edits = self.buffer_subscription.consume().into_inner();
        let tab_size = Self::tab_size(&self.buffer, cx);

        let (snapshot, edits) = self.inlay_map.sync(buffer_snapshot.clone(), edits);
        let (mut fold_map, snapshot, edits) = self.fold_map.write(snapshot, edits);
        let (snapshot, edits) = self.tab_map.sync(snapshot, edits, tab_size);
        let (snapshot, edits) = self
            .wrap_map
            .update(cx, |map, cx| map.sync(snapshot, edits, cx));
        self.block_map.read(snapshot, edits, None);

        let inline = creases.iter().filter_map(|crease| {
            if let Crease::Inline {
                range, placeholder, ..
            } = crease
            {
                Some((range.clone(), placeholder.clone()))
            } else {
                None
            }
        });
        let (snapshot, edits) = fold_map.fold(inline);

        let (snapshot, edits) = self.tab_map.sync(snapshot, edits, tab_size);
        let (snapshot, edits) = self
            .wrap_map
            .update(cx, |map, cx| map.sync(snapshot, edits, cx));

        let blocks = creases
            .into_iter()
            .filter_map(|crease| {
                if let Crease::Block {
                    range,
                    block_height,
                    render_block,
                    block_style,
                    block_priority,
                    ..
                } = crease
                {
                    Some((
                        range,
                        render_block,
                        block_height,
                        block_style,
                        block_priority,
                    ))
                } else {
                    None
                }
            })
            .map(|(range, render, height, style, priority)| {
                let start = buffer_snapshot.anchor_before(range.start);
                let end = buffer_snapshot.anchor_after(range.end);
                BlockProperties {
                    placement: BlockPlacement::Replace(start..=end),
                    render,
                    height: Some(height),
                    style,
                    priority,
                }
            });

        self.block_map.write(snapshot, edits, None).insert(blocks);
    }

    /// Removes any folds with the given ranges.
    #[instrument(skip_all)]
    pub fn remove_folds_with_type<T: ToOffset>(
        &mut self,
        ranges: impl IntoIterator<Item = Range<T>>,
        type_id: TypeId,
        cx: &mut Context<Self>,
    ) {
        let snapshot = self.buffer.read(cx).snapshot(cx);
        let edits = self.buffer_subscription.consume().into_inner();
        let tab_size = Self::tab_size(&self.buffer, cx);

        let (snapshot, edits) = self.inlay_map.sync(snapshot, edits);
        let (mut fold_map, snapshot, edits) = self.fold_map.write(snapshot, edits);
        let (snapshot, edits) = self.tab_map.sync(snapshot, edits, tab_size);
        let (snapshot, edits) = self
            .wrap_map
            .update(cx, |map, cx| map.sync(snapshot, edits, cx));
        self.block_map.read(snapshot, edits, None);

        let (snapshot, edits) = fold_map.remove_folds(ranges, type_id);
        let (snapshot, edits) = self.tab_map.sync(snapshot, edits, tab_size);
        let (self_new_wrap_snapshot, self_new_wrap_edits) = self
            .wrap_map
            .update(cx, |map, cx| map.sync(snapshot, edits, cx));

        self.block_map
            .write(self_new_wrap_snapshot, self_new_wrap_edits, None);
    }

    /// Removes any folds whose ranges intersect any of the given ranges.
    #[instrument(skip_all)]
    pub fn unfold_intersecting<T: ToOffset>(
        &mut self,
        ranges: impl IntoIterator<Item = Range<T>>,
        inclusive: bool,
        cx: &mut Context<Self>,
    ) -> WrapSnapshot {
        let snapshot = self.buffer.read(cx).snapshot(cx);
        let offset_ranges = ranges
            .into_iter()
            .map(|range| range.start.to_offset(&snapshot)..range.end.to_offset(&snapshot))
            .collect::<Vec<_>>();
        let edits = self.buffer_subscription.consume().into_inner();
        let tab_size = Self::tab_size(&self.buffer, cx);

        let (snapshot, edits) = self.inlay_map.sync(snapshot, edits);
        let (mut fold_map, snapshot, edits) = self.fold_map.write(snapshot, edits);
        let (snapshot, edits) = self.tab_map.sync(snapshot, edits, tab_size);
        let (snapshot, edits) = self
            .wrap_map
            .update(cx, |map, cx| map.sync(snapshot, edits, cx));
        self.block_map.read(snapshot, edits, None);

        let (snapshot, edits) =
            fold_map.unfold_intersecting(offset_ranges.iter().cloned(), inclusive);
        let (snapshot, edits) = self.tab_map.sync(snapshot, edits, tab_size);
        let (self_new_wrap_snapshot, self_new_wrap_edits) = self
            .wrap_map
            .update(cx, |map, cx| map.sync(snapshot, edits, cx));

        self.block_map
            .write(self_new_wrap_snapshot.clone(), self_new_wrap_edits, None)
            .remove_intersecting_replace_blocks(offset_ranges, inclusive);

        self_new_wrap_snapshot
    }

    #[instrument(skip_all)]
    pub fn disable_header_for_buffer(&mut self, buffer_id: BufferId, cx: &mut Context<Self>) {
        let (self_wrap_snapshot, self_wrap_edits) = self.sync_through_wrap(cx);
        self.block_map
            .write(self_wrap_snapshot, self_wrap_edits, None)
            .disable_header_for_buffer(buffer_id);
    }

    #[instrument(skip_all)]
    pub fn fold_buffers(
        &mut self,
        buffer_ids: impl IntoIterator<Item = language::BufferId>,
        cx: &mut App,
    ) {
        let buffer_ids: Vec<_> = buffer_ids.into_iter().collect();

        let (self_wrap_snapshot, self_wrap_edits) = self.sync_through_wrap(cx);

        Self::with_synced_companion_mut(
            self.entity_id,
            &self.companion,
            cx,
            |companion_view, cx| {
                self.block_map
                    .write(
                        self_wrap_snapshot.clone(),
                        self_wrap_edits.clone(),
                        companion_view,
                    )
                    .fold_buffers(buffer_ids.iter().copied(), self.buffer.read(cx), cx);
            },
        )
    }

    #[instrument(skip_all)]
    pub fn unfold_buffers(
        &mut self,
        buffer_ids: impl IntoIterator<Item = language::BufferId>,
        cx: &mut Context<Self>,
    ) {
        let buffer_ids: Vec<_> = buffer_ids.into_iter().collect();

        let (self_wrap_snapshot, self_wrap_edits) = self.sync_through_wrap(cx);

        Self::with_synced_companion_mut(
            self.entity_id,
            &self.companion,
            cx,
            |companion_view, cx| {
                self.block_map
                    .write(
                        self_wrap_snapshot.clone(),
                        self_wrap_edits.clone(),
                        companion_view,
                    )
                    .unfold_buffers(buffer_ids.iter().copied(), self.buffer.read(cx), cx);
            },
        )
    }

    #[instrument(skip_all)]
    pub(crate) fn is_buffer_folded(&self, buffer_id: language::BufferId) -> bool {
        self.block_map.folded_buffers.contains(&buffer_id)
    }

    #[instrument(skip_all)]
    pub(crate) fn folded_buffers(&self) -> &HashSet<BufferId> {
        &self.block_map.folded_buffers
    }

    #[instrument(skip_all)]
    pub fn insert_creases(
        &mut self,
        creases: impl IntoIterator<Item = Crease<Anchor>>,
        cx: &mut Context<Self>,
    ) -> Vec<CreaseId> {
        let snapshot = self.buffer.read(cx).snapshot(cx);
        self.crease_map.insert(creases, &snapshot)
    }

    #[instrument(skip_all)]
    pub fn remove_creases(
        &mut self,
        crease_ids: impl IntoIterator<Item = CreaseId>,
        cx: &mut Context<Self>,
    ) -> Vec<(CreaseId, Range<Anchor>)> {
        let snapshot = self.buffer.read(cx).snapshot(cx);
        self.crease_map.remove(crease_ids, &snapshot)
    }

    /// Replaces the LSP folding-range creases for a single buffer.
    /// Converts the supplied buffer-anchor ranges into multi-buffer creases
    /// by mapping them through the appropriate excerpts.
    pub(crate) fn set_lsp_folding_ranges(
        &mut self,
        buffer_id: BufferId,
        ranges: Vec<LspFoldingRange>,
        cx: &mut Context<Self>,
    ) {
        let snapshot = self.buffer.read(cx).snapshot(cx);

        let old_ids = self
            .lsp_folding_crease_ids
            .remove(&buffer_id)
            .unwrap_or_default();
        if !old_ids.is_empty() {
            self.crease_map.remove(old_ids, &snapshot);
        }

        if ranges.is_empty() {
            return;
        }

        let base_placeholder = self.fold_placeholder.clone();
        let creases = ranges.into_iter().filter_map(|folding_range| {
            let mb_range =
                snapshot.buffer_anchor_range_to_anchor_range(folding_range.range.clone())?;
            let placeholder = if let Some(collapsed_text) = folding_range.collapsed_text {
                FoldPlaceholder {
                    render: Arc::new({
                        let collapsed_text = collapsed_text.clone();
                        move |fold_id, _fold_range, cx: &mut gpui::App| {
                            use gpui::{Element as _, ParentElement as _};
                            FoldPlaceholder::fold_element(fold_id, cx)
                                .child(collapsed_text.clone())
                                .into_any()
                        }
                    }),
                    constrain_width: false,
                    merge_adjacent: base_placeholder.merge_adjacent,
                    type_tag: base_placeholder.type_tag,
                    collapsed_text: Some(collapsed_text),
                }
            } else {
                base_placeholder.clone()
            };
            Some(Crease::simple(mb_range, placeholder))
        });

        let new_ids = self.crease_map.insert(creases, &snapshot);
        if !new_ids.is_empty() {
            self.lsp_folding_crease_ids.insert(buffer_id, new_ids);
        }
    }

    /// Removes all LSP folding-range creases for a single buffer.
    pub(crate) fn clear_lsp_folding_ranges(&mut self, buffer_id: BufferId, cx: &mut Context<Self>) {
        if let Some(old_ids) = self.lsp_folding_crease_ids.remove(&buffer_id) {
            let snapshot = self.buffer.read(cx).snapshot(cx);
            self.crease_map.remove(old_ids, &snapshot);
        }
    }

    /// Returns `true` when at least one buffer has LSP folding-range creases.
    pub(crate) fn has_lsp_folding_ranges(&self) -> bool {
        !self.lsp_folding_crease_ids.is_empty()
    }
}
