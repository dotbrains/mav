use super::*;

impl Editor {
    pub(crate) fn read_metadata_from_db(
        &mut self,
        item_id: u64,
        workspace_id: WorkspaceId,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) {
        if self.buffer_kind(cx) == ItemBufferKind::Singleton
            && !self.mode.is_minimap()
            && WorkspaceSettings::get(None, cx).restore_on_startup
                != RestoreOnStartupBehavior::EmptyTab
        {
            let buffer_snapshot = OnceCell::new();

            let file_path: Option<Arc<Path>> =
                self.buffer().read(cx).as_singleton().and_then(|buffer| {
                    project::File::from_dyn(buffer.read(cx).file())
                        .map(|file| Arc::from(file.abs_path(cx)))
                });

            let db = EditorDb::global(cx);
            let (folds, needs_migration) = if let Some(ref path) = file_path {
                if let Some(folds) = db.get_file_folds(workspace_id, path).log_err()
                    && !folds.is_empty()
                {
                    (Some(folds), false)
                } else if let Some(folds) = db.get_editor_folds(item_id, workspace_id).log_err()
                    && !folds.is_empty()
                {
                    (Some(folds), true)
                } else {
                    (None, false)
                }
            } else {
                let folds = db.get_editor_folds(item_id, workspace_id).log_err();
                (folds.filter(|f| !f.is_empty()), false)
            };

            if let Some(folds) = folds {
                let snapshot = buffer_snapshot.get_or_init(|| self.buffer.read(cx).snapshot(cx));
                let (valid_folds, db_folds_for_migration) =
                    fold_persistence::resolve_persisted_folds(folds, snapshot, needs_migration);

                if !valid_folds.is_empty() {
                    self.fold_ranges(valid_folds, false, window, cx);

                    if needs_migration && let Some(ref path) = file_path {
                        let path = path.clone();
                        let db = EditorDb::global(cx);
                        cx.spawn(async move |_, _| {
                            db.save_file_folds(workspace_id, path, db_folds_for_migration)
                                .await
                                .log_err();
                        })
                        .detach();
                    }
                }
            }

            if let Some(selections) = db.get_editor_selections(item_id, workspace_id).log_err()
                && !selections.is_empty()
            {
                let snapshot = buffer_snapshot.get_or_init(|| self.buffer.read(cx).snapshot(cx));
                self.selection_history.mode = SelectionHistoryMode::Skipping;
                self.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                    s.select_ranges(selections.into_iter().map(|(start, end)| {
                        snapshot.clip_offset(MultiBufferOffset(start), Bias::Left)
                            ..snapshot.clip_offset(MultiBufferOffset(end), Bias::Right)
                    }));
                });
                self.selection_history.mode = SelectionHistoryMode::Normal;
            };
        }

        self.read_scroll_position_from_db(item_id, workspace_id, window, cx);
    }
}
