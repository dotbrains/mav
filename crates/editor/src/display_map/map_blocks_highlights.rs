use super::*;

impl DisplayMap {
    pub fn insert_blocks(
        &mut self,
        blocks: impl IntoIterator<Item = BlockProperties<Anchor>>,
        cx: &mut Context<Self>,
    ) -> Vec<CustomBlockId> {
        let (self_wrap_snapshot, self_wrap_edits) = self.sync_through_wrap(cx);
        Self::with_synced_companion_mut(
            self.entity_id,
            &self.companion,
            cx,
            |companion_view, _cx| {
                self.block_map
                    .write(
                        self_wrap_snapshot.clone(),
                        self_wrap_edits.clone(),
                        companion_view,
                    )
                    .insert(blocks)
            },
        )
    }

    #[instrument(skip_all)]
    pub fn resize_blocks(&mut self, heights: HashMap<CustomBlockId, u32>, cx: &mut Context<Self>) {
        let (self_wrap_snapshot, self_wrap_edits) = self.sync_through_wrap(cx);

        Self::with_synced_companion_mut(
            self.entity_id,
            &self.companion,
            cx,
            |companion_view, _cx| {
                self.block_map
                    .write(
                        self_wrap_snapshot.clone(),
                        self_wrap_edits.clone(),
                        companion_view,
                    )
                    .resize(heights);
            },
        )
    }

    #[instrument(skip_all)]
    pub fn replace_blocks(&mut self, renderers: HashMap<CustomBlockId, RenderBlock>) {
        self.block_map.replace_blocks(renderers);
    }

    #[instrument(skip_all)]
    pub fn remove_blocks(&mut self, ids: HashSet<CustomBlockId>, cx: &mut Context<Self>) {
        let (self_wrap_snapshot, self_wrap_edits) = self.sync_through_wrap(cx);

        Self::with_synced_companion_mut(
            self.entity_id,
            &self.companion,
            cx,
            |companion_view, _cx| {
                self.block_map
                    .write(
                        self_wrap_snapshot.clone(),
                        self_wrap_edits.clone(),
                        companion_view,
                    )
                    .remove(ids);
            },
        )
    }

    #[instrument(skip_all)]
    pub fn row_for_block(
        &mut self,
        block_id: CustomBlockId,
        cx: &mut Context<Self>,
    ) -> Option<DisplayRow> {
        let (self_wrap_snapshot, self_wrap_edits) = self.sync_through_wrap(cx);

        let companion_wrap_data = self.companion.as_ref().and_then(|(companion_dm, _)| {
            companion_dm
                .update(cx, |dm, cx| dm.sync_through_wrap(cx))
                .ok()
        });

        let companion_ref = self.companion.as_ref().map(|(_, c)| c.read(cx));
        let companion_view = companion_wrap_data.as_ref().zip(companion_ref).map(
            |((snapshot, edits), companion)| {
                CompanionView::new(self.entity_id, snapshot, edits, companion)
            },
        );

        let block_map = self.block_map.read(
            self_wrap_snapshot.clone(),
            self_wrap_edits.clone(),
            companion_view,
        );
        let block_row = block_map.row_for_block(block_id)?;

        if let Some((companion_dm, _)) = &self.companion {
            let _ = companion_dm.update(cx, |dm, cx| {
                if let Some((companion_snapshot, companion_edits)) = companion_wrap_data {
                    let their_companion_ref = dm.companion.as_ref().map(|(_, c)| c.read(cx));
                    dm.block_map.read(
                        companion_snapshot,
                        companion_edits,
                        their_companion_ref.map(|c| {
                            CompanionView::new(
                                dm.entity_id,
                                &self_wrap_snapshot,
                                &self_wrap_edits,
                                c,
                            )
                        }),
                    );
                }
            });
        }

        Some(DisplayRow(block_row.0))
    }

    #[instrument(skip_all)]
    pub fn highlight_text(
        &mut self,
        key: HighlightKey,
        mut ranges: Vec<Range<Anchor>>,
        style: HighlightStyle,
        merge: bool,
        cx: &App,
    ) {
        let multi_buffer_snapshot = self.buffer.read(cx).snapshot(cx);
        match Arc::make_mut(&mut self.text_highlights).entry(key) {
            Entry::Occupied(mut slot) => match Arc::get_mut(slot.get_mut()) {
                Some((_, previous_ranges)) if merge => {
                    previous_ranges.extend(ranges);
                    previous_ranges.sort_by(|a, b| a.start.cmp(&b.start, &multi_buffer_snapshot));
                }
                Some((previous_style, previous_ranges)) => {
                    *previous_style = style;
                    *previous_ranges = ranges;
                    previous_ranges.sort_by(|a, b| a.start.cmp(&b.start, &multi_buffer_snapshot));
                }
                None if merge => {
                    ranges.extend(slot.get().1.iter().cloned());
                    ranges.sort_by(|a, b| a.start.cmp(&b.start, &multi_buffer_snapshot));
                    slot.insert(Arc::new((style, ranges)));
                }
                None => {
                    ranges.sort_by(|a, b| a.start.cmp(&b.start, &multi_buffer_snapshot));
                    slot.insert(Arc::new((style, ranges)));
                }
            },
            Entry::Vacant(slot) => {
                ranges.sort_by(|a, b| a.start.cmp(&b.start, &multi_buffer_snapshot));
                slot.insert(Arc::new((style, ranges)));
            }
        }
    }

    #[instrument(skip_all)]
    pub(crate) fn highlight_inlays(
        &mut self,
        key: HighlightKey,
        highlights: Vec<InlayHighlight>,
        style: HighlightStyle,
    ) {
        for highlight in highlights {
            let update = self.inlay_highlights.update(&key, |highlights| {
                highlights.insert(highlight.inlay, (style, highlight.clone()))
            });
            if update.is_none() {
                self.inlay_highlights.insert(
                    key,
                    TreeMap::from_ordered_entries([(highlight.inlay, (style, highlight))]),
                );
            }
        }
    }

    #[instrument(skip_all)]
    pub fn text_highlights(&self, key: HighlightKey) -> Option<(HighlightStyle, &[Range<Anchor>])> {
        let highlights = self.text_highlights.get(&key)?;
        Some((highlights.0, &highlights.1))
    }

    pub fn all_text_highlights(
        &self,
    ) -> impl Iterator<Item = (&HighlightKey, &Arc<(HighlightStyle, Vec<Range<Anchor>>)>)> {
        self.text_highlights.iter()
    }

    pub fn all_semantic_token_highlights(
        &self,
    ) -> impl Iterator<
        Item = (
            &BufferId,
            &(Arc<[SemanticTokenHighlight]>, Arc<HighlightStyleInterner>),
        ),
    > {
        self.semantic_token_highlights.iter()
    }

    pub fn clear_highlights(&mut self, key: HighlightKey) -> bool {
        let mut cleared = Arc::make_mut(&mut self.text_highlights)
            .remove(&key)
            .is_some();
        cleared |= self.inlay_highlights.remove(&key).is_some();
        cleared
    }

    pub fn clear_highlights_with(&mut self, f: &mut dyn FnMut(&HighlightKey) -> bool) -> bool {
        let mut cleared = false;
        Arc::make_mut(&mut self.text_highlights).retain(|k, _| {
            let b = !f(k);
            cleared |= b;
            b
        });
        self.inlay_highlights.retain(|k, _| {
            let b = !f(k);
            cleared |= b;
            b
        });
        cleared
    }

    pub fn set_font(&self, font: Font, font_size: Pixels, cx: &mut Context<Self>) -> bool {
        self.wrap_map
            .update(cx, |map, cx| map.set_font_with_size(font, font_size, cx))
    }

    pub fn set_wrap_width(&self, width: Option<Pixels>, cx: &mut Context<Self>) -> bool {
        self.wrap_map
            .update(cx, |map, cx| map.set_wrap_width(width, cx))
    }

    #[instrument(skip_all)]
    pub fn update_fold_widths(
        &mut self,
        widths: impl IntoIterator<Item = (ChunkRendererId, Pixels)>,
        cx: &mut Context<Self>,
    ) -> bool {
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

        let (snapshot, edits) = fold_map.update_fold_widths(widths);
        let widths_changed = !edits.is_empty();
        let (snapshot, edits) = self.tab_map.sync(snapshot, edits, tab_size);
        let (self_new_wrap_snapshot, self_new_wrap_edits) = self
            .wrap_map
            .update(cx, |map, cx| map.sync(snapshot, edits, cx));

        self.block_map
            .read(self_new_wrap_snapshot, self_new_wrap_edits, None);

        widths_changed
    }

    pub(crate) fn current_inlays(&self) -> impl Iterator<Item = &Inlay> + Default {
        self.inlay_map.current_inlays()
    }

    #[instrument(skip_all)]
    pub(crate) fn splice_inlays(
        &mut self,
        to_remove: &[InlayId],
        to_insert: Vec<Inlay>,
        cx: &mut Context<Self>,
    ) {
        if to_remove.is_empty() && to_insert.is_empty() {
            return;
        }
        let buffer_snapshot = self.buffer.read(cx).snapshot(cx);
        let edits = self.buffer_subscription.consume().into_inner();
        let tab_size = Self::tab_size(&self.buffer, cx);

        let companion_wrap_data = self.companion.as_ref().and_then(|(companion_dm, _)| {
            companion_dm
                .update(cx, |dm, cx| dm.sync_through_wrap(cx))
                .ok()
        });

        let (snapshot, edits) = self.inlay_map.sync(buffer_snapshot, edits);
        let (snapshot, edits) = self.fold_map.read(snapshot, edits);
        let (snapshot, edits) = self.tab_map.sync(snapshot, edits, tab_size);
        let (snapshot, edits) = self
            .wrap_map
            .update(cx, |map, cx| map.sync(snapshot, edits, cx));

        {
            let companion_ref = self.companion.as_ref().map(|(_, c)| c.read(cx));
            let companion_view = companion_wrap_data.as_ref().zip(companion_ref).map(
                |((snapshot, edits), companion)| {
                    CompanionView::new(self.entity_id, snapshot, edits, companion)
                },
            );
            self.block_map.read(snapshot, edits, companion_view);
        }

        let (snapshot, edits) = self.inlay_map.splice(to_remove, to_insert);
        let (snapshot, edits) = self.fold_map.read(snapshot, edits);
        let (snapshot, edits) = self.tab_map.sync(snapshot, edits, tab_size);
        let (self_new_wrap_snapshot, self_new_wrap_edits) = self
            .wrap_map
            .update(cx, |map, cx| map.sync(snapshot, edits, cx));

        let (self_wrap_snapshot, self_wrap_edits) =
            (self_new_wrap_snapshot.clone(), self_new_wrap_edits.clone());

        {
            let companion_ref = self.companion.as_ref().map(|(_, c)| c.read(cx));
            let companion_view = companion_wrap_data.as_ref().zip(companion_ref).map(
                |((snapshot, edits), companion)| {
                    CompanionView::new(self.entity_id, snapshot, edits, companion)
                },
            );
            self.block_map
                .read(self_new_wrap_snapshot, self_new_wrap_edits, companion_view);
        }

        if let Some((companion_dm, _)) = &self.companion {
            let _ = companion_dm.update(cx, |dm, cx| {
                if let Some((companion_snapshot, companion_edits)) = companion_wrap_data {
                    let their_companion_ref = dm.companion.as_ref().map(|(_, c)| c.read(cx));
                    dm.block_map.read(
                        companion_snapshot,
                        companion_edits,
                        their_companion_ref.map(|c| {
                            CompanionView::new(
                                dm.entity_id,
                                &self_wrap_snapshot,
                                &self_wrap_edits,
                                c,
                            )
                        }),
                    );
                }
            });
        }
    }

    #[instrument(skip_all)]
    pub(crate) fn tab_size(buffer: &Entity<MultiBuffer>, cx: &App) -> NonZeroU32 {
        if let Some(buffer) = buffer.read(cx).as_singleton().map(|buffer| buffer.read(cx)) {
            LanguageSettings::for_buffer(buffer, cx).tab_size
        } else {
            AllLanguageSettings::get_global(cx).defaults.tab_size
        }
    }

    #[cfg(test)]
    pub fn is_rewrapping(&self, cx: &gpui::App) -> bool {
        self.wrap_map.read(cx).is_rewrapping()
    }

    pub fn invalidate_semantic_highlights(&mut self, buffer_id: BufferId) {
        Arc::make_mut(&mut self.semantic_token_highlights).remove(&buffer_id);
    }
}
