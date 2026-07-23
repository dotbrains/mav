use super::*;

#[cfg(test)]
impl SplittableEditor {
    fn check_invariants(&self, quiesced: bool, cx: &mut App) {
        use text::Bias;

        use crate::display_map::Block;
        use crate::display_map::DisplayRow;

        let rhs_snapshot = self
            .rhs_editor
            .update(cx, |editor, cx| editor.display_snapshot(cx));

        let Some(lhs) = self.lhs.as_ref() else {
            assert!(
                rhs_snapshot.companion_snapshot().is_none(),
                "rhs display snapshot should not have a companion when unsplit"
            );

            let shared_scroll_anchor = self
                .rhs_editor
                .read(cx)
                .scroll_manager
                .shared_scroll_anchor(cx);
            if let Some(display_map_id) = shared_scroll_anchor.display_map_id {
                assert_eq!(
                    display_map_id, rhs_snapshot.display_map_id,
                    "unsplit editor should not retain a scroll anchor native to a torn-down split companion"
                );
            }

            let _ = self
                .rhs_editor
                .read(cx)
                .scroll_manager
                .native_anchor(&rhs_snapshot, cx);
            return;
        };

        self.debug_print(cx);
        self.check_excerpt_invariants(quiesced, cx);

        let lhs_snapshot = lhs
            .editor
            .update(cx, |editor, cx| editor.display_snapshot(cx));

        let lhs_companion = lhs_snapshot
            .companion_snapshot()
            .expect("lhs display snapshot should have rhs companion while split");
        assert_eq!(
            lhs_companion.display_map_id, rhs_snapshot.display_map_id,
            "lhs display snapshot companion should point to rhs display map"
        );
        assert!(
            lhs_companion.companion_snapshot().is_none(),
            "embedded companion snapshot should not recursively contain another companion"
        );

        let rhs_companion = rhs_snapshot
            .companion_snapshot()
            .expect("rhs display snapshot should have lhs companion while split");
        assert_eq!(
            rhs_companion.display_map_id, lhs_snapshot.display_map_id,
            "rhs display snapshot companion should point to lhs display map"
        );
        assert!(
            rhs_companion.companion_snapshot().is_none(),
            "embedded companion snapshot should not recursively contain another companion"
        );

        let lhs_scroll_anchor_entity_id = lhs
            .editor
            .read(cx)
            .scroll_manager
            .scroll_anchor_entity()
            .entity_id();
        let rhs_scroll_anchor_entity_id = self
            .rhs_editor
            .read(cx)
            .scroll_manager
            .scroll_anchor_entity()
            .entity_id();
        assert_eq!(
            lhs_scroll_anchor_entity_id, rhs_scroll_anchor_entity_id,
            "split editors should share a scroll anchor entity"
        );

        let shared_scroll_anchor = self
            .rhs_editor
            .read(cx)
            .scroll_manager
            .shared_scroll_anchor(cx);
        if let Some(display_map_id) = shared_scroll_anchor.display_map_id {
            assert!(
                display_map_id == lhs_snapshot.display_map_id
                    || display_map_id == rhs_snapshot.display_map_id,
                "shared scroll anchor should be native to one side of the split"
            );
        }
        let _ = lhs
            .editor
            .read(cx)
            .scroll_manager
            .native_anchor(&lhs_snapshot, cx);
        let _ = self
            .rhs_editor
            .read(cx)
            .scroll_manager
            .native_anchor(&rhs_snapshot, cx);

        if quiesced {
            let lhs_max_row = lhs_snapshot.max_point().row();
            let rhs_max_row = rhs_snapshot.max_point().row();
            assert_eq!(lhs_max_row, rhs_max_row, "mismatch in display row count");

            let lhs_excerpt_block_rows = lhs_snapshot
                .blocks_in_range(DisplayRow(0)..lhs_max_row + 1)
                .filter(|(_, block)| {
                    matches!(
                        block,
                        Block::BufferHeader { .. } | Block::ExcerptBoundary { .. }
                    )
                })
                .map(|(row, _)| row)
                .collect::<Vec<_>>();
            let rhs_excerpt_block_rows = rhs_snapshot
                .blocks_in_range(DisplayRow(0)..rhs_max_row + 1)
                .filter(|(_, block)| {
                    matches!(
                        block,
                        Block::BufferHeader { .. } | Block::ExcerptBoundary { .. }
                    )
                })
                .map(|(row, _)| row)
                .collect::<Vec<_>>();
            assert_eq!(lhs_excerpt_block_rows, rhs_excerpt_block_rows);

            for (lhs_hunk, rhs_hunk) in lhs_snapshot.diff_hunks().zip(rhs_snapshot.diff_hunks()) {
                assert_eq!(
                    lhs_hunk.diff_base_byte_range, rhs_hunk.diff_base_byte_range,
                    "mismatch in hunks"
                );
                assert_eq!(
                    lhs_hunk.status, rhs_hunk.status,
                    "mismatch in hunk statuses"
                );

                let (lhs_point, rhs_point) =
                    if lhs_hunk.row_range.is_empty() || rhs_hunk.row_range.is_empty() {
                        use multi_buffer::ToPoint as _;

                        let lhs_end = Point::new(lhs_hunk.row_range.end.0, 0);
                        let rhs_end = Point::new(rhs_hunk.row_range.end.0, 0);

                        let lhs_excerpt_end = lhs_snapshot
                            .anchor_in_excerpt(lhs_hunk.excerpt_range.context.end)
                            .unwrap()
                            .to_point(&lhs_snapshot);
                        let lhs_exceeds = lhs_end >= lhs_excerpt_end;
                        let rhs_excerpt_end = rhs_snapshot
                            .anchor_in_excerpt(rhs_hunk.excerpt_range.context.end)
                            .unwrap()
                            .to_point(&rhs_snapshot);
                        let rhs_exceeds = rhs_end >= rhs_excerpt_end;
                        if lhs_exceeds != rhs_exceeds {
                            continue;
                        }

                        (lhs_end, rhs_end)
                    } else {
                        (
                            Point::new(lhs_hunk.row_range.start.0, 0),
                            Point::new(rhs_hunk.row_range.start.0, 0),
                        )
                    };
                let lhs_point = lhs_snapshot.point_to_display_point(lhs_point, Bias::Left);
                let rhs_point = rhs_snapshot.point_to_display_point(rhs_point, Bias::Left);
                assert_eq!(
                    lhs_point.row(),
                    rhs_point.row(),
                    "mismatch in hunk position"
                );
            }
        }
    }
    fn check_excerpt_invariants(&self, quiesced: bool, cx: &gpui::App) {
        let lhs = self.lhs.as_ref().expect("should have lhs editor");

        let rhs_snapshot = self.rhs_multibuffer.read(cx).snapshot(cx);
        let rhs_excerpts = rhs_snapshot.excerpts().collect::<Vec<_>>();
        let lhs_snapshot = lhs.multibuffer.read(cx).snapshot(cx);
        let lhs_excerpts = lhs_snapshot.excerpts().collect::<Vec<_>>();
        assert_eq!(lhs_excerpts.len(), rhs_excerpts.len());

        for (lhs_excerpt, rhs_excerpt) in lhs_excerpts.into_iter().zip(rhs_excerpts) {
            assert_eq!(
                lhs_snapshot
                    .path_for_buffer(lhs_excerpt.context.start.buffer_id)
                    .unwrap(),
                rhs_snapshot
                    .path_for_buffer(rhs_excerpt.context.start.buffer_id)
                    .unwrap(),
                "corresponding excerpts should have the same path"
            );
            let diff = self
                .rhs_multibuffer
                .read(cx)
                .diff_for(rhs_excerpt.context.start.buffer_id)
                .expect("missing diff");
            assert_eq!(
                lhs_excerpt.context.start.buffer_id,
                diff.read(cx).base_text(cx).remote_id(),
                "corresponding lhs excerpt should show diff base text"
            );

            if quiesced {
                let diff_snapshot = diff.read(cx).snapshot(cx);
                let lhs_buffer_snapshot = lhs_snapshot
                    .buffer_for_id(lhs_excerpt.context.start.buffer_id)
                    .unwrap();
                let rhs_buffer_snapshot = rhs_snapshot
                    .buffer_for_id(rhs_excerpt.context.start.buffer_id)
                    .unwrap();
                let lhs_range = lhs_excerpt.context.to_point(&lhs_buffer_snapshot);
                let rhs_range = rhs_excerpt.context.to_point(&rhs_buffer_snapshot);
                let expected_lhs_range = buffer_range_to_base_text_range(
                    &rhs_range,
                    &diff_snapshot,
                    &rhs_buffer_snapshot,
                );
                assert_eq!(
                    lhs_range, expected_lhs_range,
                    "corresponding lhs excerpt should have a matching range"
                )
            }
        }
    }
}
