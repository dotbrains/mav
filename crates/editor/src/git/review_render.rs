use super::*;

impl Editor {
    pub(super) fn calculate_overlay_height(
        &self,
        hunk_key: &DiffHunkKey,
        comments_expanded: bool,
        snapshot: &MultiBufferSnapshot,
    ) -> u32 {
        let comment_count = self.hunk_comment_count(hunk_key, snapshot);
        let base_height: u32 = 2; // Input row with avatar and buttons
        if comment_count == 0 {
            base_height
        } else if comments_expanded {
            // Header (1 line) + 2 lines per comment
            base_height + 1 + (comment_count as u32 * 2)
        } else {
            // Just header when collapsed
            base_height + 1
        }
    }
}
