use super::*;

impl DisplayMap {
    pub fn new(
        buffer: Entity<MultiBuffer>,
        font: Font,
        font_size: Pixels,
        wrap_width: Option<Pixels>,
        buffer_header_height: u32,
        excerpt_header_height: u32,
        fold_placeholder: FoldPlaceholder,
        diagnostics_max_severity: DiagnosticSeverity,
        cx: &mut Context<Self>,
    ) -> Self {
        let tab_size = Self::tab_size(&buffer, cx);
        // Important: obtain the snapshot BEFORE creating the subscription.
        // snapshot() may call sync() which publishes edits. If we subscribe first,
        // those edits would be captured but the InlayMap would already be at the
        // post-edit state, causing a desync.
        let buffer_snapshot = buffer.read(cx).snapshot(cx);
        let buffer_subscription = buffer.update(cx, |buffer, _| buffer.subscribe());
        let crease_map = CreaseMap::new(&buffer_snapshot);
        let (inlay_map, snapshot) = InlayMap::new(buffer_snapshot);
        let (fold_map, snapshot) = FoldMap::new(snapshot);
        let (tab_map, snapshot) = TabMap::new(snapshot, tab_size);
        let (wrap_map, snapshot) = WrapMap::new(snapshot, font, font_size, wrap_width, cx);
        let block_map = BlockMap::new(snapshot, buffer_header_height, excerpt_header_height);

        cx.observe(&wrap_map, |_, _, cx| cx.notify()).detach();

        DisplayMap {
            entity_id: cx.entity_id(),
            buffer,
            buffer_subscription,
            fold_map,
            inlay_map,
            tab_map,
            wrap_map,
            block_map,
            crease_map,
            fold_placeholder,
            diagnostics_max_severity,
            text_highlights: Default::default(),
            inlay_highlights: Default::default(),
            semantic_token_highlights: Default::default(),
            clip_at_line_ends: false,
            masked: false,
            companion: None,
            lsp_folding_crease_ids: HashMap::default(),
        }
    }

    pub(crate) fn set_companion(
        &mut self,
        companion: Option<(Entity<DisplayMap>, Entity<Companion>)>,
        cx: &mut Context<Self>,
    ) {
        let this = cx.weak_entity();
        // Reverting to no companion, recompute the block map to clear spacers
        // and balancing blocks.
        let Some((companion_display_map, companion)) = companion else {
            let Some((_, companion)) = self.companion.take() else {
                return;
            };
            assert_eq!(self.entity_id, companion.read(cx).rhs_display_map_id);
            let (snapshot, _edits) = self.sync_through_wrap(cx);
            let edits = Patch::new(vec![text::Edit {
                old: WrapRow(0)
                    ..self.block_map.wrap_snapshot.borrow().max_point().row() + WrapRow(1),
                new: WrapRow(0)..snapshot.max_point().row() + WrapRow(1),
            }]);
            self.block_map.deferred_edits.set(edits);
            self.block_map.retain_blocks_raw(&mut |block| {
                if companion
                    .read(cx)
                    .lhs_custom_block_to_balancing_block
                    .borrow()
                    .values()
                    .any(|id| *id == block.id)
                {
                    return false;
                }
                true
            });
            return;
        };
        assert_eq!(self.entity_id, companion.read(cx).rhs_display_map_id);

        // Note, throwing away the wrap edits because we defer spacer computation to the first render.
        let snapshot = {
            let edits = self.buffer_subscription.consume();
            let snapshot = self.buffer.read(cx).snapshot(cx);
            let tab_size = Self::tab_size(&self.buffer, cx);
            let (snapshot, edits) = self.inlay_map.sync(snapshot, edits.into_inner());
            let (mut writer, snapshot, edits) = self.fold_map.write(snapshot, edits);
            let (snapshot, edits) = self.tab_map.sync(snapshot, edits, tab_size);
            let (_snapshot, _edits) = self
                .wrap_map
                .update(cx, |wrap_map, cx| wrap_map.sync(snapshot, edits, cx));

            let (snapshot, edits) = writer.unfold_intersecting([Anchor::Min..Anchor::Max], true);
            let (snapshot, edits) = self.tab_map.sync(snapshot, edits, tab_size);
            let (snapshot, _edits) = self
                .wrap_map
                .update(cx, |wrap_map, cx| wrap_map.sync(snapshot, edits, cx));

            self.block_map.retain_blocks_raw(&mut |block| {
                !matches!(block.placement, BlockPlacement::Replace(_))
            });
            snapshot
        };

        let (companion_wrap_snapshot, _companion_wrap_edits) =
            companion_display_map.update(cx, |dm, cx| dm.sync_through_wrap(cx));

        let edits = Patch::new(vec![text::Edit {
            old: WrapRow(0)..self.block_map.wrap_snapshot.borrow().max_point().row() + WrapRow(1),
            new: WrapRow(0)..snapshot.max_point().row() + WrapRow(1),
        }]);
        self.block_map.deferred_edits.set(edits);

        let all_blocks: Vec<_> = self.block_map.blocks_raw().map(Clone::clone).collect();

        companion_display_map.update(cx, |companion_display_map, cx| {
            // Sync folded buffers from RHS to LHS. Also clean up stale
            // entries: the block map doesn't remove buffers from
            // `folded_buffers` when they leave the multibuffer, so we
            // unfold any RHS buffers whose companion mapping is missing.
            let rhs_snapshot = self.buffer.read(cx).snapshot(cx);
            let mut buffers_to_unfold = Vec::new();
            for my_buffer in self.folded_buffers() {
                let their_buffer = rhs_snapshot
                    .diff_for_buffer_id(*my_buffer)
                    .map(|diff| diff.base_text().remote_id());

                let Some(their_buffer) = their_buffer else {
                    buffers_to_unfold.push(*my_buffer);
                    continue;
                };

                companion_display_map
                    .block_map
                    .folded_buffers
                    .insert(their_buffer);
            }
            for buffer_id in buffers_to_unfold {
                self.block_map.folded_buffers.remove(&buffer_id);
            }

            for block in all_blocks {
                let Some(their_block) = block_map::balancing_block(
                    &block.properties(),
                    snapshot.buffer(),
                    companion_wrap_snapshot.buffer(),
                    self.entity_id,
                    companion.read(cx),
                ) else {
                    continue;
                };
                let their_id = companion_display_map
                    .block_map
                    .insert_block_raw(their_block, companion_wrap_snapshot.buffer());
                companion.update(cx, |companion, _cx| {
                    companion
                        .custom_block_to_balancing_block(self.entity_id)
                        .borrow_mut()
                        .insert(block.id, their_id);
                });
            }
            let companion_edits = Patch::new(vec![text::Edit {
                old: WrapRow(0)
                    ..companion_display_map
                        .block_map
                        .wrap_snapshot
                        .borrow()
                        .max_point()
                        .row()
                        + WrapRow(1),
                new: WrapRow(0)..companion_wrap_snapshot.max_point().row() + WrapRow(1),
            }]);
            companion_display_map
                .block_map
                .deferred_edits
                .set(companion_edits);
            companion_display_map.companion = Some((this, companion.clone()));
        });

        self.companion = Some((companion_display_map.downgrade(), companion));
    }

    pub(crate) fn companion(&self) -> Option<&Entity<Companion>> {
        self.companion.as_ref().map(|(_, c)| c)
    }

    fn sync_through_wrap(&mut self, cx: &mut App) -> (WrapSnapshot, WrapPatch) {
        let tab_size = Self::tab_size(&self.buffer, cx);
        let buffer_snapshot = self.buffer.read(cx).snapshot(cx);
        let edits = self.buffer_subscription.consume().into_inner();

        let (snapshot, edits) = self.inlay_map.sync(buffer_snapshot, edits);
        let (snapshot, edits) = self.fold_map.read(snapshot, edits);
        let (snapshot, edits) = self.tab_map.sync(snapshot, edits, tab_size);
        self.wrap_map
            .update(cx, |map, cx| map.sync(snapshot, edits, cx))
    }

    fn with_synced_companion_mut<R>(
        display_map_id: EntityId,
        companion: &Option<(WeakEntity<DisplayMap>, Entity<Companion>)>,
        cx: &mut App,
        callback: impl FnOnce(Option<CompanionViewMut<'_>>, &mut App) -> R,
    ) -> R {
        let Some((companion_display_map, companion)) = companion else {
            return callback(None, cx);
        };
        let Some(companion_display_map) = companion_display_map.upgrade() else {
            return callback(None, cx);
        };
        companion_display_map.update(cx, |companion_display_map, cx| {
            let (companion_wrap_snapshot, companion_wrap_edits) =
                companion_display_map.sync_through_wrap(cx);
            companion_display_map
                .buffer
                .update(cx, |companion_multibuffer, cx| {
                    companion.update(cx, |companion, cx| {
                        let companion_view = CompanionViewMut::new(
                            display_map_id,
                            companion_display_map.entity_id,
                            &companion_wrap_snapshot,
                            &companion_wrap_edits,
                            companion_multibuffer,
                            companion,
                            &mut companion_display_map.block_map,
                        );
                        callback(Some(companion_view), cx)
                    })
                })
        })
    }

    #[instrument(skip_all)]
    pub fn snapshot(&mut self, cx: &mut Context<Self>) -> DisplaySnapshot {
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

        let block_snapshot = self
            .block_map
            .read(
                self_wrap_snapshot.clone(),
                self_wrap_edits.clone(),
                companion_view,
            )
            .snapshot;

        if let Some((companion_dm, _)) = &self.companion {
            let _ = companion_dm.update(cx, |dm, _cx| {
                if let Some((companion_snapshot, companion_edits)) = companion_wrap_data {
                    let their_companion_ref = dm.companion.as_ref().map(|(_, c)| c.read(_cx));
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

        let companion_display_snapshot = self.companion.as_ref().and_then(|(companion_dm, _)| {
            companion_dm
                .update(cx, |dm, cx| Arc::new(dm.snapshot_simple(cx)))
                .ok()
        });

        DisplaySnapshot {
            display_map_id: self.entity_id,
            companion_display_snapshot,
            block_snapshot,
            diagnostics_max_severity: self.diagnostics_max_severity,
            crease_snapshot: self.crease_map.snapshot(),
            text_highlights: self.text_highlights.clone(),
            inlay_highlights: self.inlay_highlights.clone(),
            semantic_token_highlights: self.semantic_token_highlights.clone(),
            clip_at_line_ends: self.clip_at_line_ends,
            masked: self.masked,
            use_lsp_folding_ranges: !self.lsp_folding_crease_ids.is_empty(),
            fold_placeholder: self.fold_placeholder.clone(),
        }
    }

    fn snapshot_simple(&mut self, cx: &mut Context<Self>) -> DisplaySnapshot {
        let (wrap_snapshot, wrap_edits) = self.sync_through_wrap(cx);

        let block_snapshot = self
            .block_map
            .read(wrap_snapshot, wrap_edits, None)
            .snapshot;

        DisplaySnapshot {
            display_map_id: self.entity_id,
            companion_display_snapshot: None,
            block_snapshot,
            diagnostics_max_severity: self.diagnostics_max_severity,
            crease_snapshot: self.crease_map.snapshot(),
            text_highlights: self.text_highlights.clone(),
            inlay_highlights: self.inlay_highlights.clone(),
            semantic_token_highlights: self.semantic_token_highlights.clone(),
            clip_at_line_ends: self.clip_at_line_ends,
            masked: self.masked,
            use_lsp_folding_ranges: !self.lsp_folding_crease_ids.is_empty(),
            fold_placeholder: self.fold_placeholder.clone(),
        }
    }

    pub fn crease_snapshot(&self) -> CreaseSnapshot {
        self.crease_map.snapshot()
    }

    #[instrument(skip_all)]
    pub fn set_state(&mut self, other: &DisplaySnapshot, cx: &mut Context<Self>) {
        self.fold(
            other
                .folds_in_range(MultiBufferOffset(0)..other.buffer_snapshot().len())
                .map(|fold| {
                    Crease::simple(
                        fold.range.to_offset(other.buffer_snapshot()),
                        fold.placeholder.clone(),
                    )
                })
                .collect(),
            cx,
        );
        for buffer_id in &other.block_snapshot.buffers_with_disabled_headers {
            self.disable_header_for_buffer(*buffer_id, cx);
        }
    }
}
