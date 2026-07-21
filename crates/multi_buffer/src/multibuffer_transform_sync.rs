use super::*;

impl MultiBuffer {
    pub(super) fn sync_diff_transforms(
        snapshot: &mut MultiBufferSnapshot,
        excerpt_edits: Vec<text::Edit<ExcerptOffset>>,
        change_kind: DiffChangeKind,
    ) -> Vec<Edit<MultiBufferOffset>> {
        if excerpt_edits.is_empty() {
            return vec![];
        }

        let mut excerpts = snapshot.excerpts.cursor::<ExcerptOffset>(());
        let mut old_diff_transforms = snapshot
            .diff_transforms
            .cursor::<Dimensions<ExcerptOffset, MultiBufferOffset>>(());
        let mut new_diff_transforms = SumTree::default();
        let mut old_expanded_hunks = HashSet::default();
        let mut output_edits = Vec::new();
        let mut output_delta = 0_isize;
        let mut at_transform_boundary = true;
        let mut end_of_current_insert = None;

        let mut excerpt_edits = excerpt_edits.into_iter().peekable();
        while let Some(edit) = excerpt_edits.next() {
            excerpts.seek_forward(&edit.new.start, Bias::Right);
            if excerpts.item().is_none() && *excerpts.start() == edit.new.start {
                excerpts.prev();
            }

            // Keep any transforms that are before the edit.
            if at_transform_boundary {
                at_transform_boundary = false;
                let transforms_before_edit = old_diff_transforms.slice(&edit.old.start, Bias::Left);
                Self::append_diff_transforms(&mut new_diff_transforms, transforms_before_edit);
                if let Some(transform) = old_diff_transforms.item()
                    && old_diff_transforms.end().0 == edit.old.start
                    && old_diff_transforms.start().0 < edit.old.start
                {
                    Self::push_diff_transform(&mut new_diff_transforms, transform.clone());
                    old_diff_transforms.next();
                }
            }

            // Compute the start of the edit in output coordinates.
            let edit_start_overshoot = edit.old.start - old_diff_transforms.start().0;
            let edit_old_start = old_diff_transforms.start().1 + edit_start_overshoot;
            let edit_new_start =
                MultiBufferOffset((edit_old_start.0 as isize + output_delta) as usize);

            let changed_diff_hunks = Self::recompute_diff_transforms_for_edit(
                &edit,
                &mut excerpts,
                &mut old_diff_transforms,
                &mut new_diff_transforms,
                &mut end_of_current_insert,
                &mut old_expanded_hunks,
                snapshot,
                change_kind,
            );

            // Compute the end of the edit in output coordinates.
            let edit_old_end_overshoot = edit.old.end - old_diff_transforms.start().0;
            let edit_new_end_overshoot = edit.new.end - new_diff_transforms.summary().excerpt_len();
            let edit_old_end = old_diff_transforms.start().1 + edit_old_end_overshoot;
            let edit_new_end = new_diff_transforms.summary().output.len + edit_new_end_overshoot;
            let output_edit = Edit {
                old: edit_old_start..edit_old_end,
                new: edit_new_start..edit_new_end,
            };

            output_delta += (output_edit.new.end - output_edit.new.start) as isize;
            output_delta -= (output_edit.old.end - output_edit.old.start) as isize;
            if changed_diff_hunks || matches!(change_kind, DiffChangeKind::BufferEdited) {
                output_edits.push(output_edit);
            }

            // If this is the last edit that intersects the current diff transform,
            // then recreate the content up to the end of this transform, to prepare
            // for reusing additional slices of the old transforms.
            if excerpt_edits
                .peek()
                .is_none_or(|next_edit| next_edit.old.start >= old_diff_transforms.end().0)
            {
                let keep_next_old_transform = (old_diff_transforms.start().0 >= edit.old.end)
                    && match old_diff_transforms.item() {
                        Some(DiffTransform::BufferContent {
                            inserted_hunk_info: Some(hunk),
                            ..
                        }) => excerpts.item().is_some_and(|excerpt| {
                            if let Some(diff) = find_diff_state(&snapshot.diffs, excerpt.buffer_id)
                                && diff.main_buffer.is_some()
                            {
                                return true;
                            }
                            hunk.hunk_start_anchor
                                .is_valid(&excerpt.buffer_snapshot(&snapshot))
                        }),
                        _ => true,
                    };

                let mut excerpt_offset = edit.new.end;
                if !keep_next_old_transform {
                    excerpt_offset += old_diff_transforms.end().0 - edit.old.end;
                    old_diff_transforms.next();
                }

                old_expanded_hunks.clear();
                Self::push_buffer_content_transform(
                    snapshot,
                    &mut new_diff_transforms,
                    excerpt_offset,
                    end_of_current_insert,
                );
                at_transform_boundary = true;
            }
        }

        // Keep any transforms that are after the last edit.
        Self::append_diff_transforms(&mut new_diff_transforms, old_diff_transforms.suffix());

        // Ensure there's always at least one buffer content transform.
        if new_diff_transforms.is_empty() {
            new_diff_transforms.push(
                DiffTransform::BufferContent {
                    summary: Default::default(),
                    inserted_hunk_info: None,
                },
                (),
            );
        }

        drop(old_diff_transforms);
        drop(excerpts);
        snapshot.diff_transforms = new_diff_transforms;
        snapshot.edit_count += 1;

        #[cfg(any(test, feature = "test-support"))]
        snapshot.check_invariants();
        output_edits
    }

    fn recompute_diff_transforms_for_edit(
        edit: &Edit<ExcerptOffset>,
        excerpts: &mut Cursor<Excerpt, ExcerptOffset>,
        old_diff_transforms: &mut Cursor<
            DiffTransform,
            Dimensions<ExcerptOffset, MultiBufferOffset>,
        >,
        new_diff_transforms: &mut SumTree<DiffTransform>,
        end_of_current_insert: &mut Option<(ExcerptOffset, DiffTransformHunkInfo)>,
        old_expanded_hunks: &mut HashSet<DiffTransformHunkInfo>,
        snapshot: &MultiBufferSnapshot,
        change_kind: DiffChangeKind,
    ) -> bool {
        log::trace!(
            "recomputing diff transform for edit {:?} => {:?}",
            edit.old.start..edit.old.end,
            edit.new.start..edit.new.end
        );

        // Record which hunks were previously expanded.
        while let Some(item) = old_diff_transforms.item() {
            if let Some(hunk_info) = item.hunk_info() {
                log::trace!(
                    "previously expanded hunk at {:?}",
                    old_diff_transforms.start()
                );
                old_expanded_hunks.insert(hunk_info);
            }
            if old_diff_transforms.end().0 > edit.old.end {
                break;
            }
            old_diff_transforms.next();
        }

        // Avoid querying diff hunks if there's no possibility of hunks being expanded.
        // For inverted diffs, hunks are always shown, so we can't skip this.
        let all_diff_hunks_expanded = snapshot.all_diff_hunks_expanded;
        if old_expanded_hunks.is_empty()
            && change_kind == DiffChangeKind::BufferEdited
            && !all_diff_hunks_expanded
            && !snapshot.has_inverted_diff
        {
            return false;
        }

        // Visit each excerpt that intersects the edit.
        let mut did_expand_hunks = false;
        while let Some(excerpt) = excerpts.item() {
            // Recompute the expanded hunks in the portion of the excerpt that
            // intersects the edit.
            if let Some(diff) = find_diff_state(&snapshot.diffs, excerpt.buffer_id) {
                let buffer_snapshot = &excerpt.buffer_snapshot(&snapshot);
                let excerpt_start = *excerpts.start();
                let excerpt_end = excerpt_start + excerpt.text_summary.len;
                let excerpt_buffer_start = excerpt.range.context.start.to_offset(buffer_snapshot);
                let excerpt_buffer_end = excerpt_buffer_start + excerpt.text_summary.len;
                let edit_buffer_start =
                    excerpt_buffer_start + edit.new.start.saturating_sub(excerpt_start);
                let edit_buffer_end =
                    excerpt_buffer_start + edit.new.end.saturating_sub(excerpt_start);
                let edit_buffer_end = edit_buffer_end.min(excerpt_buffer_end);

                if let Some(main_buffer) = &diff.main_buffer {
                    for hunk in diff.hunks_intersecting_base_text_range(
                        edit_buffer_start..edit_buffer_end,
                        main_buffer,
                    ) {
                        did_expand_hunks = true;
                        let hunk_buffer_range = hunk.diff_base_byte_range.clone();
                        if hunk_buffer_range.start < excerpt_buffer_start {
                            log::trace!("skipping hunk that starts before excerpt");
                            continue;
                        }
                        let hunk_excerpt_start = excerpt_start
                            + hunk_buffer_range.start.saturating_sub(excerpt_buffer_start);
                        let hunk_excerpt_end = excerpt_end
                            .min(excerpt_start + (hunk_buffer_range.end - excerpt_buffer_start));
                        Self::push_buffer_content_transform(
                            snapshot,
                            new_diff_transforms,
                            hunk_excerpt_start,
                            *end_of_current_insert,
                        );
                        if !hunk_buffer_range.is_empty() {
                            let hunk_info = DiffTransformHunkInfo {
                                buffer_id: buffer_snapshot.remote_id(),
                                hunk_start_anchor: hunk.buffer_range.start,
                                hunk_secondary_status: hunk.secondary_status,
                                excerpt_end: excerpt.end_anchor(),
                                is_logically_deleted: true,
                            };
                            *end_of_current_insert =
                                Some((hunk_excerpt_end.min(excerpt_end), hunk_info));
                        }
                    }
                } else {
                    let edit_anchor_range = buffer_snapshot.anchor_before(edit_buffer_start)
                        ..buffer_snapshot.anchor_after(edit_buffer_end);
                    for hunk in diff.hunks_intersecting_range(edit_anchor_range, buffer_snapshot) {
                        if hunk.is_created_file() && !all_diff_hunks_expanded {
                            continue;
                        }

                        let hunk_buffer_range = hunk.buffer_range.to_offset(buffer_snapshot);
                        if hunk_buffer_range.start < excerpt_buffer_start {
                            log::trace!("skipping hunk that starts before excerpt");
                            continue;
                        }

                        let hunk_info = DiffTransformHunkInfo {
                            buffer_id: buffer_snapshot.remote_id(),
                            hunk_start_anchor: hunk.buffer_range.start,
                            hunk_secondary_status: hunk.secondary_status,
                            excerpt_end: excerpt.end_anchor(),
                            is_logically_deleted: false,
                        };

                        let hunk_excerpt_start = excerpt_start
                            + hunk_buffer_range.start.saturating_sub(excerpt_buffer_start);
                        let hunk_excerpt_end = excerpt_end
                            .min(excerpt_start + (hunk_buffer_range.end - excerpt_buffer_start));

                        Self::push_buffer_content_transform(
                            snapshot,
                            new_diff_transforms,
                            hunk_excerpt_start,
                            *end_of_current_insert,
                        );

                        // For every existing hunk, determine if it was previously expanded
                        // and if it should currently be expanded.
                        let was_previously_expanded = old_expanded_hunks.contains(&hunk_info);
                        let should_expand_hunk = match &change_kind {
                            DiffChangeKind::DiffUpdated { base_changed: true } => {
                                was_previously_expanded || all_diff_hunks_expanded
                            }
                            DiffChangeKind::ExpandOrCollapseHunks { expand } => {
                                let intersects = hunk_buffer_range.is_empty()
                                    || (hunk_buffer_range.end > edit_buffer_start);
                                if *expand {
                                    intersects || was_previously_expanded || all_diff_hunks_expanded
                                } else {
                                    !intersects
                                        && (was_previously_expanded || all_diff_hunks_expanded)
                                }
                            }
                            _ => was_previously_expanded || all_diff_hunks_expanded,
                        };

                        if should_expand_hunk {
                            did_expand_hunks = true;
                            log::trace!(
                                "expanding hunk {:?}",
                                hunk_excerpt_start..hunk_excerpt_end,
                            );

                            if !hunk.diff_base_byte_range.is_empty()
                                && hunk_buffer_range.start >= edit_buffer_start
                                && hunk_buffer_range.start <= excerpt_buffer_end
                                && snapshot.show_deleted_hunks
                            {
                                let base_text = diff.base_text();
                                let mut text_cursor =
                                    base_text.as_rope().cursor(hunk.diff_base_byte_range.start);
                                let mut base_text_summary = text_cursor
                                    .summary::<TextSummary>(hunk.diff_base_byte_range.end);

                                let mut has_trailing_newline = false;
                                if base_text_summary.last_line_chars > 0 {
                                    base_text_summary += TextSummary::newline();
                                    has_trailing_newline = true;
                                }

                                new_diff_transforms.push(
                                    DiffTransform::DeletedHunk {
                                        base_text_byte_range: hunk.diff_base_byte_range.clone(),
                                        summary: base_text_summary,
                                        buffer_id: buffer_snapshot.remote_id(),
                                        hunk_info,
                                        has_trailing_newline,
                                    },
                                    (),
                                );
                            }

                            if !hunk_buffer_range.is_empty() {
                                *end_of_current_insert =
                                    Some((hunk_excerpt_end.min(excerpt_end), hunk_info));
                            }
                        }
                    }
                }
            }

            if excerpts.end() <= edit.new.end {
                excerpts.next();
            } else {
                break;
            }
        }

        did_expand_hunks || !old_expanded_hunks.is_empty()
    }

    fn append_diff_transforms(
        new_transforms: &mut SumTree<DiffTransform>,
        subtree: SumTree<DiffTransform>,
    ) {
        if let Some(DiffTransform::BufferContent {
            inserted_hunk_info,
            summary,
        }) = subtree.first()
            && Self::extend_last_buffer_content_transform(
                new_transforms,
                *inserted_hunk_info,
                *summary,
            )
        {
            let mut cursor = subtree.cursor::<()>(());
            cursor.next();
            cursor.next();
            new_transforms.append(cursor.suffix(), ());
            return;
        }
        new_transforms.append(subtree, ());
    }

    fn push_diff_transform(new_transforms: &mut SumTree<DiffTransform>, transform: DiffTransform) {
        if let DiffTransform::BufferContent {
            inserted_hunk_info: inserted_hunk_anchor,
            summary,
        } = transform
            && Self::extend_last_buffer_content_transform(
                new_transforms,
                inserted_hunk_anchor,
                summary,
            )
        {
            return;
        }
        new_transforms.push(transform, ());
    }

    fn push_buffer_content_transform(
        old_snapshot: &MultiBufferSnapshot,
        new_transforms: &mut SumTree<DiffTransform>,
        end_offset: ExcerptOffset,
        current_inserted_hunk: Option<(ExcerptOffset, DiffTransformHunkInfo)>,
    ) {
        let inserted_region = current_inserted_hunk.map(|(insertion_end_offset, hunk_info)| {
            (end_offset.min(insertion_end_offset), Some(hunk_info))
        });
        let unchanged_region = [(end_offset, None)];

        for (end_offset, inserted_hunk_info) in inserted_region.into_iter().chain(unchanged_region)
        {
            let start_offset = new_transforms.summary().excerpt_len();
            if end_offset <= start_offset {
                continue;
            }
            let summary_to_add = old_snapshot
                .text_summary_for_excerpt_offset_range::<MBTextSummary>(start_offset..end_offset);

            if !Self::extend_last_buffer_content_transform(
                new_transforms,
                inserted_hunk_info,
                summary_to_add,
            ) {
                new_transforms.push(
                    DiffTransform::BufferContent {
                        summary: summary_to_add,
                        inserted_hunk_info,
                    },
                    (),
                )
            }
        }
    }

    fn extend_last_buffer_content_transform(
        new_transforms: &mut SumTree<DiffTransform>,
        new_inserted_hunk_info: Option<DiffTransformHunkInfo>,
        summary_to_add: MBTextSummary,
    ) -> bool {
        let mut did_extend = false;
        new_transforms.update_last(
            |last_transform| {
                if let DiffTransform::BufferContent {
                    summary,
                    inserted_hunk_info: inserted_hunk_anchor,
                } = last_transform
                    && *inserted_hunk_anchor == new_inserted_hunk_info
                {
                    *summary += summary_to_add;
                    did_extend = true;
                }
            },
            (),
        );
        did_extend
    }
}
