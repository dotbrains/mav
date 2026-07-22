use super::*;

impl Editor {
    pub fn show_diff_review_overlay(
        &mut self,
        display_range: Range<DisplayRow>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Range { start, end } = display_range.sorted();
        let buffer_snapshot = self.buffer.read(cx).snapshot(cx);
        let editor_snapshot = self.snapshot(window, cx);

        // Convert display rows to multibuffer points
        let start_point = editor_snapshot
            .display_snapshot
            .display_point_to_point(start.as_display_point(), Bias::Left);
        let end_point = editor_snapshot
            .display_snapshot
            .display_point_to_point(end.as_display_point(), Bias::Left);
        let end_multi_buffer_row = MultiBufferRow(end_point.row);

        // Create anchor range for the selected lines (start of first line to end of last line)
        let line_end = Point::new(
            end_point.row,
            buffer_snapshot.line_len(end_multi_buffer_row),
        );
        let anchor_range =
            buffer_snapshot.anchor_after(start_point)..buffer_snapshot.anchor_before(line_end);

        // Compute the hunk key for this display row
        let file_path = buffer_snapshot
            .file_at(start_point)
            .map(|file: &Arc<dyn language::File>| file.path().clone())
            .unwrap_or_else(|| Arc::from(util::rel_path::RelPath::empty()));
        let hunk_start_anchor = buffer_snapshot.anchor_before(start_point);
        let new_hunk_key = DiffHunkKey {
            file_path,
            hunk_start_anchor,
        };

        // Check if we already have an overlay for this hunk
        if let Some(existing_overlay) = self.diff_review_overlays.iter().find(|overlay| {
            Self::hunk_keys_match(&overlay.hunk_key, &new_hunk_key, &buffer_snapshot)
        }) {
            // Just focus the existing overlay's prompt editor
            let focus_handle = existing_overlay.prompt_editor.focus_handle(cx);
            window.focus(&focus_handle, cx);
            return;
        }

        // Dismiss overlays that have no comments for their hunks
        self.dismiss_overlays_without_comments(cx);

        // Get the current user's avatar URI from the project's user_store
        let user_avatar_uri = self.project.as_ref().and_then(|project| {
            let user_store = project.read(cx).user_store();
            user_store
                .read(cx)
                .current_user()
                .map(|user| user.avatar_uri.clone())
        });

        // Create anchor at the end of the last row so the block appears immediately below it
        // Use multibuffer coordinates for anchor creation
        let line_len = buffer_snapshot.line_len(end_multi_buffer_row);
        let anchor = buffer_snapshot.anchor_after(Point::new(end_multi_buffer_row.0, line_len));

        // Use the hunk key we already computed
        let hunk_key = new_hunk_key;

        // Create the prompt editor for the review input
        let prompt_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("Add a review comment...", window, cx);
            editor
        });

        // Register the Newline action on the prompt editor to submit the review
        let parent_editor = cx.entity().downgrade();
        let subscription = prompt_editor.update(cx, |prompt_editor, _cx| {
            prompt_editor.register_action({
                let parent_editor = parent_editor.clone();
                move |_: &crate::actions::Newline, window, cx| {
                    if let Some(editor) = parent_editor.upgrade() {
                        editor.update(cx, |editor, cx| {
                            editor.submit_diff_review_comment(window, cx);
                        });
                    }
                }
            })
        });

        // Calculate initial height based on existing comments for this hunk
        let initial_height = self.calculate_overlay_height(&hunk_key, true, &buffer_snapshot);

        // Create the overlay block
        let prompt_editor_for_render = prompt_editor.clone();
        let hunk_key_for_render = hunk_key.clone();
        let editor_handle = cx.entity().downgrade();
        let block = BlockProperties {
            style: BlockStyle::Sticky,
            placement: BlockPlacement::Below(anchor),
            height: Some(initial_height),
            render: Arc::new(move |cx| {
                Self::render_diff_review_overlay(
                    &prompt_editor_for_render,
                    &hunk_key_for_render,
                    &editor_handle,
                    cx,
                )
            }),
            priority: 0,
        };

        let block_ids = self.insert_blocks([block], None, cx);
        let Some(block_id) = block_ids.into_iter().next() else {
            log::error!("Failed to insert diff review overlay block");
            return;
        };

        self.diff_review_overlays.push(DiffReviewOverlay {
            anchor_range,
            block_id,
            prompt_editor: prompt_editor.clone(),
            hunk_key,
            comments_expanded: true,
            inline_edit_editors: HashMap::default(),
            inline_edit_subscriptions: HashMap::default(),
            user_avatar_uri,
            _subscription: subscription,
        });

        // Focus the prompt editor
        let focus_handle = prompt_editor.focus_handle(cx);
        window.focus(&focus_handle, cx);

        cx.notify();
    }

    /// Stores the diff review comment locally.
    /// Comments are stored per-hunk and can later be batch-submitted to the Agent panel.
    pub fn submit_diff_review_comment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Find the overlay that currently has focus
        let overlay_index = self
            .diff_review_overlays
            .iter()
            .position(|overlay| overlay.prompt_editor.focus_handle(cx).is_focused(window));
        let Some(overlay_index) = overlay_index else {
            return;
        };
        let overlay = &self.diff_review_overlays[overlay_index];

        let comment_text = overlay.prompt_editor.read(cx).text(cx).trim().to_string();
        if comment_text.is_empty() {
            return;
        }

        let anchor_range = overlay.anchor_range.clone();
        let hunk_key = overlay.hunk_key.clone();

        self.add_review_comment(hunk_key.clone(), comment_text, anchor_range, cx);

        // Clear the prompt editor but keep the overlay open
        if let Some(overlay) = self.diff_review_overlays.get(overlay_index) {
            overlay.prompt_editor.update(cx, |editor, cx| {
                editor.clear(window, cx);
            });
        }

        // Refresh the overlay to update the block height for the new comment
        self.refresh_diff_review_overlay_height(&hunk_key, window, cx);

        cx.notify();
    }

    /// Returns the prompt editor for the diff review overlay, if one is active.
    /// This is primarily used for testing.
    pub fn diff_review_prompt_editor(&self) -> Option<&Entity<Editor>> {
        self.diff_review_overlays
            .first()
            .map(|overlay| &overlay.prompt_editor)
    }

    /// Sets whether the comments section is expanded in the diff review overlay.
    /// This is primarily used for testing.
    pub fn set_diff_review_comments_expanded(&mut self, expanded: bool, cx: &mut Context<Self>) {
        for overlay in &mut self.diff_review_overlays {
            overlay.comments_expanded = expanded;
        }
        cx.notify();
    }

    /// Returns the total count of stored review comments across all hunks.
    pub(super) fn total_review_comment_count(&self) -> usize {
        self.stored_review_comments
            .iter()
            .map(|(_, v)| v.len())
            .sum()
    }

    /// Adds a new review comment to a specific hunk.
    pub(super) fn add_review_comment(
        &mut self,
        hunk_key: DiffHunkKey,
        comment: String,
        anchor_range: Range<Anchor>,
        cx: &mut Context<Self>,
    ) -> usize {
        let id = self.next_review_comment_id;
        self.next_review_comment_id += 1;

        let stored_comment = StoredReviewComment::new(id, comment, anchor_range);

        let snapshot = self.buffer.read(cx).snapshot(cx);
        let key_point = hunk_key.hunk_start_anchor.to_point(&snapshot);

        // Find existing entry for this hunk or add a new one
        if let Some((_, comments)) = self.stored_review_comments.iter_mut().find(|(k, _)| {
            k.file_path == hunk_key.file_path
                && k.hunk_start_anchor.to_point(&snapshot) == key_point
        }) {
            comments.push(stored_comment);
        } else {
            self.stored_review_comments
                .push((hunk_key, vec![stored_comment]));
        }

        cx.emit(EditorEvent::ReviewCommentsChanged {
            total_count: self.total_review_comment_count(),
        });
        cx.notify();
        id
    }
}
