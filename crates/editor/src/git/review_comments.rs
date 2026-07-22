use super::*;

impl Editor {
    pub(super) fn dismiss_all_diff_review_overlays(&mut self, cx: &mut Context<Self>) {
        if self.diff_review_overlays.is_empty() {
            return;
        }
        let block_ids: HashSet<_> = self
            .diff_review_overlays
            .drain(..)
            .map(|overlay| overlay.block_id)
            .collect();
        self.remove_blocks(block_ids, None, cx);
        cx.notify();
    }
    /// Action handler for SubmitDiffReviewComment.
    pub(super) fn submit_diff_review_comment_action(
        &mut self,
        _: &SubmitDiffReviewComment,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.submit_diff_review_comment(window, cx);
    }

    /// Returns comments for a specific hunk, ordered by creation time.
    pub(super) fn comments_for_hunk<'a>(
        &'a self,
        key: &DiffHunkKey,
        snapshot: &MultiBufferSnapshot,
    ) -> &'a [StoredReviewComment] {
        let key_point = key.hunk_start_anchor.to_point(snapshot);
        self.stored_review_comments
            .iter()
            .find(|(k, _)| {
                k.file_path == key.file_path && k.hunk_start_anchor.to_point(snapshot) == key_point
            })
            .map(|(_, comments)| comments.as_slice())
            .unwrap_or(&[])
    }

    /// Returns the count of comments for a specific hunk.
    pub(super) fn hunk_comment_count(
        &self,
        key: &DiffHunkKey,
        snapshot: &MultiBufferSnapshot,
    ) -> usize {
        let key_point = key.hunk_start_anchor.to_point(snapshot);
        self.stored_review_comments
            .iter()
            .find(|(k, _)| {
                k.file_path == key.file_path && k.hunk_start_anchor.to_point(snapshot) == key_point
            })
            .map(|(_, v)| v.len())
            .unwrap_or(0)
    }

    /// Removes a review comment by ID from any hunk.
    pub(super) fn remove_review_comment(&mut self, id: usize, cx: &mut Context<Self>) -> bool {
        for (_, comments) in self.stored_review_comments.iter_mut() {
            if let Some(index) = comments.iter().position(|c| c.id == id) {
                comments.remove(index);
                cx.emit(EditorEvent::ReviewCommentsChanged {
                    total_count: self.total_review_comment_count(),
                });
                cx.notify();
                return true;
            }
        }
        false
    }

    /// Updates a review comment's text by ID.
    pub(super) fn update_review_comment(
        &mut self,
        id: usize,
        new_comment: String,
        cx: &mut Context<Self>,
    ) -> bool {
        for (_, comments) in self.stored_review_comments.iter_mut() {
            if let Some(comment) = comments.iter_mut().find(|c| c.id == id) {
                comment.comment = new_comment;
                comment.is_editing = false;
                cx.emit(EditorEvent::ReviewCommentsChanged {
                    total_count: self.total_review_comment_count(),
                });
                cx.notify();
                return true;
            }
        }
        false
    }

    /// Sets a comment's editing state.
    pub(super) fn set_comment_editing(
        &mut self,
        id: usize,
        is_editing: bool,
        cx: &mut Context<Self>,
    ) {
        for (_, comments) in self.stored_review_comments.iter_mut() {
            if let Some(comment) = comments.iter_mut().find(|c| c.id == id) {
                comment.is_editing = is_editing;
                cx.notify();
                return;
            }
        }
    }

    /// Removes review comments whose anchors are no longer valid or whose
    /// associated diff hunks no longer exist.
    ///
    /// This should be called when the buffer changes to prevent orphaned comments
    /// from accumulating.
    pub(super) fn cleanup_orphaned_review_comments(&mut self, cx: &mut Context<Self>) {
        let snapshot = self.buffer.read(cx).snapshot(cx);
        let original_count = self.total_review_comment_count();

        // Remove comments with invalid hunk anchors
        self.stored_review_comments
            .retain(|(hunk_key, _)| hunk_key.hunk_start_anchor.is_valid(&snapshot));

        // Also clean up individual comments with invalid anchor ranges
        for (_, comments) in &mut self.stored_review_comments {
            comments.retain(|comment| {
                comment.range.start.is_valid(&snapshot) && comment.range.end.is_valid(&snapshot)
            });
        }

        // Remove empty hunk entries
        self.stored_review_comments
            .retain(|(_, comments)| !comments.is_empty());

        let new_count = self.total_review_comment_count();
        if new_count != original_count {
            cx.emit(EditorEvent::ReviewCommentsChanged {
                total_count: new_count,
            });
            cx.notify();
        }
    }

    /// Toggles the expanded state of the comments section in the overlay.
    pub(super) fn toggle_review_comments_expanded(
        &mut self,
        _: &ToggleReviewCommentsExpanded,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Find the overlay that currently has focus, or use the first one
        let overlay_info = self.diff_review_overlays.iter_mut().find_map(|overlay| {
            if overlay.prompt_editor.focus_handle(cx).is_focused(window) {
                overlay.comments_expanded = !overlay.comments_expanded;
                Some(overlay.hunk_key.clone())
            } else {
                None
            }
        });

        // If no focused overlay found, toggle the first one
        let hunk_key = overlay_info.or_else(|| {
            self.diff_review_overlays.first_mut().map(|overlay| {
                overlay.comments_expanded = !overlay.comments_expanded;
                overlay.hunk_key.clone()
            })
        });

        if let Some(hunk_key) = hunk_key {
            self.refresh_diff_review_overlay_height(&hunk_key, window, cx);
            cx.notify();
        }
    }

    /// Handles the EditReviewComment action - sets a comment into editing mode.
    pub(super) fn edit_review_comment(
        &mut self,
        action: &EditReviewComment,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let comment_id = action.id;

        // Set the comment to editing mode
        self.set_comment_editing(comment_id, true, cx);

        // Find the overlay that contains this comment and create an inline editor if needed
        // First, find which hunk this comment belongs to
        let hunk_key = self
            .stored_review_comments
            .iter()
            .find_map(|(key, comments)| {
                if comments.iter().any(|c| c.id == comment_id) {
                    Some(key.clone())
                } else {
                    None
                }
            });

        let snapshot = self.buffer.read(cx).snapshot(cx);
        if let Some(hunk_key) = hunk_key {
            if let Some(overlay) = self
                .diff_review_overlays
                .iter_mut()
                .find(|overlay| Self::hunk_keys_match(&overlay.hunk_key, &hunk_key, &snapshot))
            {
                if let std::collections::hash_map::Entry::Vacant(entry) =
                    overlay.inline_edit_editors.entry(comment_id)
                {
                    // Find the comment text
                    let comment_text = self
                        .stored_review_comments
                        .iter()
                        .flat_map(|(_, comments)| comments)
                        .find(|c| c.id == comment_id)
                        .map(|c| c.comment.clone())
                        .unwrap_or_default();

                    // Create inline editor
                    let parent_editor = cx.entity().downgrade();
                    let inline_editor = cx.new(|cx| {
                        let mut editor = Editor::single_line(window, cx);
                        editor.set_text(&*comment_text, window, cx);
                        // Select all text for easy replacement
                        editor.select_all(&crate::actions::SelectAll, window, cx);
                        editor
                    });

                    // Register the Newline action to confirm the edit
                    let subscription = inline_editor.update(cx, |inline_editor, _cx| {
                        inline_editor.register_action({
                            let parent_editor = parent_editor.clone();
                            move |_: &crate::actions::Newline, window, cx| {
                                if let Some(editor) = parent_editor.upgrade() {
                                    editor.update(cx, |editor, cx| {
                                        editor.confirm_edit_review_comment(comment_id, window, cx);
                                    });
                                }
                            }
                        })
                    });

                    // Store the subscription to keep the action handler alive
                    overlay
                        .inline_edit_subscriptions
                        .insert(comment_id, subscription);

                    // Focus the inline editor
                    let focus_handle = inline_editor.focus_handle(cx);
                    window.focus(&focus_handle, cx);

                    entry.insert(inline_editor);
                }
            }
        }

        cx.notify();
    }

    /// Confirms an inline edit of a review comment.
    pub(super) fn confirm_edit_review_comment(
        &mut self,
        comment_id: usize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Get the new text from the inline editor
        // Find the overlay containing this comment's inline editor
        let snapshot = self.buffer.read(cx).snapshot(cx);
        let hunk_key = self
            .stored_review_comments
            .iter()
            .find_map(|(key, comments)| {
                if comments.iter().any(|c| c.id == comment_id) {
                    Some(key.clone())
                } else {
                    None
                }
            });

        let new_text = hunk_key
            .as_ref()
            .and_then(|hunk_key| {
                self.diff_review_overlays
                    .iter()
                    .find(|overlay| Self::hunk_keys_match(&overlay.hunk_key, hunk_key, &snapshot))
            })
            .as_ref()
            .and_then(|overlay| overlay.inline_edit_editors.get(&comment_id))
            .map(|editor| editor.read(cx).text(cx).trim().to_string());

        if let Some(new_text) = new_text {
            if !new_text.is_empty() {
                self.update_review_comment(comment_id, new_text, cx);
            }
        }

        // Remove the inline editor and its subscription
        if let Some(hunk_key) = hunk_key {
            if let Some(overlay) = self
                .diff_review_overlays
                .iter_mut()
                .find(|overlay| Self::hunk_keys_match(&overlay.hunk_key, &hunk_key, &snapshot))
            {
                overlay.inline_edit_editors.remove(&comment_id);
                overlay.inline_edit_subscriptions.remove(&comment_id);
            }
        }

        // Clear editing state
        self.set_comment_editing(comment_id, false, cx);
    }

    /// Cancels an inline edit of a review comment.
    pub(super) fn cancel_edit_review_comment(
        &mut self,
        comment_id: usize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Find which hunk this comment belongs to
        let hunk_key = self
            .stored_review_comments
            .iter()
            .find_map(|(key, comments)| {
                if comments.iter().any(|c| c.id == comment_id) {
                    Some(key.clone())
                } else {
                    None
                }
            });

        // Remove the inline editor and its subscription
        if let Some(hunk_key) = hunk_key {
            let snapshot = self.buffer.read(cx).snapshot(cx);
            if let Some(overlay) = self
                .diff_review_overlays
                .iter_mut()
                .find(|overlay| Self::hunk_keys_match(&overlay.hunk_key, &hunk_key, &snapshot))
            {
                overlay.inline_edit_editors.remove(&comment_id);
                overlay.inline_edit_subscriptions.remove(&comment_id);
            }
        }

        // Clear editing state
        self.set_comment_editing(comment_id, false, cx);
    }

    /// Action handler for ConfirmEditReviewComment.
    pub(super) fn confirm_edit_review_comment_action(
        &mut self,
        action: &ConfirmEditReviewComment,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.confirm_edit_review_comment(action.id, window, cx);
    }

    /// Action handler for CancelEditReviewComment.
    pub(super) fn cancel_edit_review_comment_action(
        &mut self,
        action: &CancelEditReviewComment,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cancel_edit_review_comment(action.id, window, cx);
    }

    /// Handles the DeleteReviewComment action - removes a comment.
    pub(super) fn delete_review_comment(
        &mut self,
        action: &DeleteReviewComment,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Get the hunk key before removing the comment
        // Find the hunk key from the comment itself
        let comment_id = action.id;
        let hunk_key = self
            .stored_review_comments
            .iter()
            .find_map(|(key, comments)| {
                if comments.iter().any(|c| c.id == comment_id) {
                    Some(key.clone())
                } else {
                    None
                }
            });

        // Also get it from the overlay for refresh purposes
        let overlay_hunk_key = self
            .diff_review_overlays
            .first()
            .map(|o| o.hunk_key.clone());

        self.remove_review_comment(action.id, cx);

        // Refresh the overlay height after removing a comment
        if let Some(hunk_key) = hunk_key.or(overlay_hunk_key) {
            self.refresh_diff_review_overlay_height(&hunk_key, window, cx);
        }
    }
}
