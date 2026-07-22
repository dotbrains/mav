use super::*;

impl BlockMap {
    #[ztracing::instrument(skip_all, fields(edits = ?edits))]
    fn sync(
        &self,
        wrap_snapshot: &WrapSnapshot,
        mut edits: WrapPatch,
        companion_view: Option<CompanionView>,
    ) {
        let buffer = wrap_snapshot.buffer_snapshot();

        edits = self.deferred_edits.take().compose(edits);

        let max_point = wrap_snapshot.max_point();

        // Handle changing the last excerpt if it is empty.
        if buffer.trailing_excerpt_update_count()
            != self
                .wrap_snapshot
                .borrow()
                .buffer_snapshot()
                .trailing_excerpt_update_count()
        {
            let edit_start = wrap_snapshot.prev_row_boundary(max_point);
            let edit_end = max_point.row() + WrapRow(1); // this is end of file
            edits = edits.compose([WrapEdit {
                old: edit_start..edit_end,
                new: edit_start..edit_end,
            }]);
        }

        // Pull in companion edits to ensure we recompute spacers in ranges that have changed in the companion.
        if let Some(CompanionView {
            companion_wrap_snapshot: companion_new_snapshot,
            companion_wrap_edits: companion_edits,
            companion,
            display_map_id,
            ..
        }) = companion_view
        {
            let mut companion_edits_in_my_space: Vec<WrapEdit> = companion_edits
                .clone()
                .into_inner()
                .iter()
                .map(|edit| {
                    let companion_start = companion_new_snapshot
                        .to_point(WrapPoint::new(edit.new.start, 0), Bias::Left);
                    let companion_end = companion_new_snapshot
                        .to_point(WrapPoint::new(edit.new.end, 0), Bias::Left);

                    let my_start = companion
                        .convert_point_from_companion(
                            display_map_id,
                            wrap_snapshot.buffer_snapshot(),
                            companion_new_snapshot.buffer_snapshot(),
                            companion_start,
                        )
                        .start;
                    let my_end = companion
                        .convert_point_from_companion(
                            display_map_id,
                            wrap_snapshot.buffer_snapshot(),
                            companion_new_snapshot.buffer_snapshot(),
                            companion_end,
                        )
                        .end;

                    let mut my_start = wrap_snapshot.make_wrap_point(my_start, Bias::Left);
                    let mut my_end = wrap_snapshot.make_wrap_point(my_end, Bias::Left);
                    // TODO(split-diff) should use trailing_excerpt_update_count for the second case
                    if my_end.column() > 0 || my_end == max_point {
                        *my_end.row_mut() += 1;
                        *my_end.column_mut() = 0;
                    }

                    // Empty edits won't survive Patch::compose, but we still need to make sure
                    // we recompute spacers when we get them.
                    if my_start.row() == my_end.row() {
                        if my_end.row() <= max_point.row() {
                            *my_end.row_mut() += 1;
                            *my_end.column_mut() = 0;
                        } else if my_start.row() > WrapRow(0) {
                            *my_start.row_mut() += 1;
                            *my_start.column_mut() = 0;
                        }
                    }

                    WrapEdit {
                        old: my_start.row()..my_end.row(),
                        new: my_start.row()..my_end.row(),
                    }
                })
                .collect();

            companion_edits_in_my_space.sort_by_key(|edit| edit.old.start);
            let mut merged_edits: Vec<WrapEdit> = Vec::new();
            for edit in companion_edits_in_my_space {
                if let Some(last) = merged_edits.last_mut() {
                    if edit.old.start <= last.old.end {
                        last.old.end = last.old.end.max(edit.old.end);
                        last.new.end = last.new.end.max(edit.new.end);
                        continue;
                    }
                }
                merged_edits.push(edit);
            }

            edits = edits.compose(merged_edits);
        }

        let edits = edits.into_inner();
        if edits.is_empty() {
            return;
        }

        let mut transforms = self.transforms.borrow_mut();
        let mut new_transforms = SumTree::default();
        let mut cursor = transforms.cursor::<WrapRow>(());
        let mut last_block_ix = 0;
        let mut blocks_in_edit = Vec::new();
        let mut edits = edits.into_iter().peekable();

        let mut inlay_point_cursor = wrap_snapshot.inlay_point_cursor();
        let mut tab_point_cursor = wrap_snapshot.tab_point_cursor();
        let mut fold_point_cursor = wrap_snapshot.fold_point_cursor();
        let mut wrap_point_cursor = wrap_snapshot.wrap_point_cursor();

        while let Some(edit) = edits.next() {
            let span = ztracing::debug_span!("while edits", edit = ?edit);
            let _enter = span.enter();

            let mut old_start = edit.old.start;
            let mut new_start = edit.new.start;

            // Only preserve transforms that:
            // * Strictly precedes this edit
            // * Isomorphic transforms that end *at* the start of the edit
            // * Below blocks that end at the start of the edit
            // However, if we hit a replace block that ends at the start of the edit we want to reconstruct it.
            new_transforms.append(cursor.slice(&old_start, Bias::Left), ());
            if let Some(transform) = cursor.item()
                && transform.summary.input_rows > WrapRow(0)
                && cursor.end() == old_start
                && transform.block.as_ref().is_none_or(|b| !b.is_replacement())
            {
                // Preserve the transform (push and next)
                new_transforms.push(transform.clone(), ());
                cursor.next();

                // Preserve below blocks at start of edit
                while let Some(transform) = cursor.item() {
                    if transform.block.as_ref().is_some_and(|b| b.place_below()) {
                        new_transforms.push(transform.clone(), ());
                        cursor.next();
                    } else {
                        break;
                    }
                }
            }

            // Ensure the edit starts at a transform boundary.
            // If the edit starts within an isomorphic transform, preserve its prefix
            // If the edit lands within a replacement block, expand the edit to include the start of the replaced input range
            let transform = cursor.item().unwrap();
            let transform_rows_before_edit = old_start - *cursor.start();
            if transform_rows_before_edit > RowDelta(0) {
                if transform.block.is_none() {
                    // Preserve any portion of the old isomorphic transform that precedes this edit.
                    push_isomorphic(
                        &mut new_transforms,
                        transform_rows_before_edit,
                        wrap_snapshot,
                    );
                } else {
                    // We landed within a block that replaces some lines, so we
                    // extend the edit to start at the beginning of the
                    // replacement.
                    debug_assert!(transform.summary.input_rows > WrapRow(0));
                    old_start -= transform_rows_before_edit;
                    new_start -= transform_rows_before_edit;
                }
            }

            // Decide where the edit ends
            // * It should end at a transform boundary
            // * Coalesce edits that intersect the same transform
            let mut old_end = edit.old.end;
            let mut new_end = edit.new.end;
            loop {
                let span = ztracing::debug_span!("decide where edit ends loop");
                let _enter = span.enter();
                // Seek to the transform starting at or after the end of the edit
                cursor.seek(&old_end, Bias::Left);
                cursor.next();

                // Extend edit to the end of the discarded transform so it is reconstructed in full
                let transform_rows_after_edit = *cursor.start() - old_end;
                old_end += transform_rows_after_edit;
                new_end += transform_rows_after_edit;

                // Combine this edit with any subsequent edits that intersect the same transform.
                while let Some(next_edit) = edits.peek() {
                    if next_edit.old.start <= *cursor.start() {
                        old_end = next_edit.old.end;
                        new_end = next_edit.new.end;
                        cursor.seek(&old_end, Bias::Left);
                        cursor.next();
                        edits.next();
                    } else {
                        break;
                    }
                }

                if *cursor.start() == old_end {
                    break;
                }
            }

            // Discard below blocks at the end of the edit. They'll be reconstructed.
            while let Some(transform) = cursor.item() {
                if transform
                    .block
                    .as_ref()
                    .is_some_and(|b| b.place_below() || matches!(b, Block::Spacer { .. }))
                {
                    cursor.next();
                } else {
                    break;
                }
            }

            // Find the blocks within this edited region.
            let new_buffer_start = wrap_snapshot.to_point(WrapPoint::new(new_start, 0), Bias::Left);
            let start_bound = Bound::Included(new_buffer_start);
            let start_block_ix =
                match self.custom_blocks[last_block_ix..].binary_search_by(|probe| {
                    probe
                        .start()
                        .to_point(buffer)
                        .cmp(&new_buffer_start)
                        // Move left until we find the index of the first block starting within this edit
                        .then(Ordering::Greater)
                }) {
                    Ok(ix) | Err(ix) => last_block_ix + ix,
                };

            let end_bound;
            let end_block_ix = if new_end > max_point.row() {
                end_bound = Bound::Unbounded;
                self.custom_blocks.len()
            } else {
                let new_buffer_end = wrap_snapshot.to_point(WrapPoint::new(new_end, 0), Bias::Left);
                end_bound = Bound::Excluded(new_buffer_end);
                match self.custom_blocks[start_block_ix..].binary_search_by(|probe| {
                    probe
                        .start()
                        .to_point(buffer)
                        .cmp(&new_buffer_end)
                        .then(Ordering::Greater)
                }) {
                    Ok(ix) | Err(ix) => start_block_ix + ix,
                }
            };
            last_block_ix = end_block_ix;

            debug_assert!(blocks_in_edit.is_empty());
            // + 8 is chosen arbitrarily to cover some multibuffer headers
            blocks_in_edit
                .reserve(end_block_ix - start_block_ix + if buffer.is_singleton() { 0 } else { 8 });

            blocks_in_edit.extend(
                self.custom_blocks[start_block_ix..end_block_ix]
                    .iter()
                    .filter_map(|block| {
                        let placement = block.placement.to_wrap_row(wrap_snapshot)?;
                        if !matches!(placement, BlockPlacement::Replace(_))
                            && wrap_snapshot.intersects_fold(Point::new(
                                block
                                    .placement
                                    .start()
                                    .to_point(wrap_snapshot.buffer_snapshot())
                                    .row,
                                0,
                            ))
                        {
                            return None;
                        }
                        if let BlockPlacement::Above(row) = placement
                            && row < new_start
                        {
                            return None;
                        }
                        Some((placement, Block::Custom(block.clone())))
                    }),
            );

            blocks_in_edit.extend(self.header_and_footer_blocks(
                buffer,
                (start_bound, end_bound),
                |point, bias| {
                    wrap_point_cursor
                        .map(
                            tab_point_cursor.map(
                                fold_point_cursor.map(inlay_point_cursor.map(point, bias), bias),
                            ),
                        )
                        .row()
                },
            ));

            if let Some(CompanionView {
                companion_wrap_snapshot: companion_snapshot,
                companion,
                display_map_id,
                ..
            }) = companion_view
            {
                blocks_in_edit.extend(self.spacer_blocks(
                    (start_bound, end_bound),
                    wrap_snapshot,
                    companion_snapshot,
                    companion,
                    display_map_id,
                ));
            }

            BlockMap::sort_blocks(&mut blocks_in_edit);

            // For each of these blocks, insert a new isomorphic transform preceding the block,
            // and then insert the block itself.
            let mut just_processed_folded_buffer = false;
            for (block_placement, block) in blocks_in_edit.drain(..) {
                let span =
                    ztracing::debug_span!("for block in edits", block_height = block.height());
                let _enter = span.enter();

                let mut summary = TransformSummary {
                    input_rows: WrapRow(0),
                    output_rows: BlockRow(block.height()),
                    longest_row: BlockRow(0),
                    longest_row_chars: 0,
                    has_replacement_blocks: false,
                };

                let rows_before_block;
                let input_rows = new_transforms.summary().input_rows;
                match &block_placement {
                    &BlockPlacement::Above(position) => {
                        let Some(delta) = position.checked_sub(input_rows) else {
                            continue;
                        };
                        rows_before_block = delta;
                        just_processed_folded_buffer = false;
                    }
                    &BlockPlacement::Near(position) | &BlockPlacement::Below(position) => {
                        if just_processed_folded_buffer {
                            continue;
                        }
                        let Some(delta) = (position + RowDelta(1)).checked_sub(input_rows) else {
                            continue;
                        };
                        rows_before_block = delta;
                    }
                    BlockPlacement::Replace(range) => {
                        let Some(delta) = range.start().checked_sub(input_rows) else {
                            continue;
                        };
                        rows_before_block = delta;
                        summary.input_rows = WrapRow(1) + (*range.end() - *range.start());
                        just_processed_folded_buffer = matches!(block, Block::FoldedBuffer { .. });
                    }
                }

                push_isomorphic(&mut new_transforms, rows_before_block, wrap_snapshot);
                new_transforms.push(
                    Transform {
                        summary,
                        block: Some(block),
                    },
                    (),
                );
            }

            // Insert an isomorphic transform after the final block.
            let rows_after_last_block =
                RowDelta(new_end.0).saturating_sub(RowDelta(new_transforms.summary().input_rows.0));
            push_isomorphic(&mut new_transforms, rows_after_last_block, wrap_snapshot);
        }

        new_transforms.append(cursor.suffix(), ());
        debug_assert_eq!(
            new_transforms.summary().input_rows,
            wrap_snapshot.max_point().row() + WrapRow(1),
        );

        drop(cursor);
        *transforms = new_transforms;
    }
}
