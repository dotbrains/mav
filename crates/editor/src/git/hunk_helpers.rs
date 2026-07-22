use super::*;

impl Editor {
    pub(super) fn hunk_after_position(
        &mut self,
        snapshot: &EditorSnapshot,
        position: Point,
        wrap_around: bool,
    ) -> Option<MultiBufferDiffHunk> {
        let result = snapshot
            .buffer_snapshot()
            .diff_hunks_in_range(position..snapshot.buffer_snapshot().max_point())
            .find(|hunk| hunk.row_range.start.0 > position.row);
        if wrap_around {
            result.or_else(|| {
                snapshot
                    .buffer_snapshot()
                    .diff_hunks_in_range(Point::zero()..position)
                    .find(|hunk| hunk.row_range.end.0 < position.row)
            })
        } else {
            result
        }
    }

    pub(super) fn hunk_before_position(
        &mut self,
        snapshot: &EditorSnapshot,
        position: Point,
        wrap_around: bool,
    ) -> Option<MultiBufferRow> {
        let result = snapshot.buffer_snapshot().diff_hunk_before(position);

        if wrap_around {
            result.or_else(|| snapshot.buffer_snapshot().diff_hunk_before(Point::MAX))
        } else {
            result
        }
    }
}
