use super::*;

impl LastEvent {
    pub fn finalize(
        &self,
        license_detection_watchers: &HashMap<WorktreeId, Rc<LicenseDetectionWatcher>>,
        cx: &App,
    ) -> Option<StoredEvent> {
        let path = buffer_path_with_id_fallback(self.new_file.as_ref(), &self.new_snapshot, cx);
        let old_path = buffer_path_with_id_fallback(self.old_file.as_ref(), &self.old_snapshot, cx);

        let in_open_source_repo =
            [self.new_file.as_ref(), self.old_file.as_ref()]
                .iter()
                .all(|file| {
                    file.is_some_and(|file| {
                        license_detection_watchers
                            .get(&file.worktree_id(cx))
                            .is_some_and(|watcher| watcher.is_project_open_source())
                    })
                });

        let (diff, old_range, new_range) = compute_diff_between_snapshots_in_range(
            &self.old_snapshot,
            &self.new_snapshot,
            &self.total_edit_range,
        )?;

        if path == old_path && diff.is_empty() {
            None
        } else {
            Some(StoredEvent {
                event: Arc::new(zeta_prompt::Event::BufferChange {
                    old_path,
                    path,
                    diff,
                    old_range,
                    new_range: new_range.clone(),
                    in_open_source_repo,
                    predicted: self.predicted,
                }),
                old_snapshot: self.old_snapshot.clone(),
                new_snapshot_version: self.new_snapshot.version.clone(),
                total_edit_range: self.new_snapshot.anchor_before(new_range.start)
                    ..self.new_snapshot.anchor_before(new_range.end),
                file_context: self.file_context.clone(),
            })
        }
    }

    pub fn split_by_pause(&self) -> (LastEvent, Option<LastEvent>) {
        let Some(boundary_snapshot) = self.snapshot_after_last_editing_pause.as_ref() else {
            return (self.clone(), None);
        };

        let Some(after) = self.suffix_after(boundary_snapshot) else {
            return (self.clone(), None);
        };

        let total_edit_range_before_pause = self
            .total_edit_range_at_last_pause_boundary
            .clone()
            .unwrap_or_else(|| self.total_edit_range.clone());

        let before = LastEvent {
            new_snapshot: boundary_snapshot.clone(),
            latest_edit_range: total_edit_range_before_pause.clone(),
            total_edit_range: total_edit_range_before_pause,
            total_edit_range_at_last_pause_boundary: None,
            snapshot_after_last_editing_pause: None,
            ..self.clone()
        };

        (before, Some(after))
    }

    /// The portion of this event that happened after `boundary_snapshot`, or
    /// None if the buffer hasn't changed since.
    pub fn suffix_after(&self, boundary_snapshot: &TextBufferSnapshot) -> Option<LastEvent> {
        let total_edit_range =
            compute_total_edit_range_between_snapshots(boundary_snapshot, &self.new_snapshot)?;
        Some(LastEvent {
            old_snapshot: boundary_snapshot.clone(),
            latest_edit_range: total_edit_range.clone(),
            total_edit_range,
            total_edit_range_at_last_pause_boundary: None,
            snapshot_after_last_editing_pause: None,
            ..self.clone()
        })
    }
}
