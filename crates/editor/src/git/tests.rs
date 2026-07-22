use super::*;

#[cfg(test)]
impl Editor {
    /// Returns the line range for the first diff review overlay, if one is active.
    /// Returns (start_row, end_row) as physical line numbers in the underlying file.
    pub(super) fn diff_review_line_range(&self, cx: &App) -> Option<(u32, u32)> {
        let overlay = self.diff_review_overlays.first()?;
        let snapshot = self.buffer.read(cx).snapshot(cx);
        let start_point = overlay.anchor_range.start.to_point(&snapshot);
        let end_point = overlay.anchor_range.end.to_point(&snapshot);
        let start_row = snapshot
            .point_to_buffer_point(start_point)
            .map(|(_, p)| p.row)
            .unwrap_or(start_point.row);
        let end_row = snapshot
            .point_to_buffer_point(end_point)
            .map(|(_, p)| p.row)
            .unwrap_or(end_point.row);
        Some((start_row, end_row))
    }
    /// Takes all stored comments from all hunks, clearing the storage.
    /// Returns a Vec of (hunk_key, comments) pairs.
    pub(super) fn take_all_review_comments(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Vec<(DiffHunkKey, Vec<StoredReviewComment>)> {
        // Dismiss all overlays when taking comments (e.g., when sending to agent)
        self.dismiss_all_diff_review_overlays(cx);
        let comments = std::mem::take(&mut self.stored_review_comments);
        // Reset the ID counter since all comments have been taken
        self.next_review_comment_id = 0;
        cx.emit(EditorEvent::ReviewCommentsChanged { total_count: 0 });
        cx.notify();
        comments
    }
}
