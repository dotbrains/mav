use super::*;

impl LocalLspStore {
    pub(super) fn initialize_buffer(
        &mut self,
        buffer_handle: &Entity<Buffer>,
        cx: &mut Context<LspStore>,
    ) {
        let buffer = buffer_handle.read(cx);

        let file = buffer.file().cloned();

        let Some(file) = File::from_dyn(file.as_ref()) else {
            return;
        };
        if !file.is_local() {
            return;
        }
        let path = ProjectPath::from_file(file, cx);
        let worktree_id = file.worktree_id(cx);
        let language = buffer.language().cloned();

        if let Some(diagnostics) = self.diagnostics.get(&worktree_id) {
            for (server_id, diagnostics) in
                diagnostics.get(file.path()).cloned().unwrap_or_default()
            {
                self.update_buffer_diagnostics(
                    buffer_handle,
                    server_id,
                    None,
                    None,
                    None,
                    Vec::new(),
                    diagnostics,
                    cx,
                )
                .log_err();
            }
        }
        let Some(language) = language else {
            return;
        };
        let Some(snapshot) = self
            .worktree_store
            .read(cx)
            .worktree_for_id(worktree_id, cx)
            .map(|worktree| worktree.read(cx).snapshot())
        else {
            return;
        };
        let delegate: Arc<dyn ManifestDelegate> = Arc::new(ManifestQueryDelegate::new(snapshot));

        for server_id in
            self.lsp_tree
                .get(path, language.name(), language.manifest(), &delegate, cx)
        {
            let server = self
                .language_servers
                .get(&server_id)
                .and_then(|server_state| {
                    if let LanguageServerState::Running { server, .. } = server_state {
                        Some(server.clone())
                    } else {
                        None
                    }
                });
            let server = match server {
                Some(server) => server,
                None => continue,
            };

            buffer_handle.update(cx, |buffer, cx| {
                buffer.set_completion_triggers(
                    server.server_id(),
                    server
                        .capabilities()
                        .completion_provider
                        .as_ref()
                        .and_then(|provider| {
                            provider
                                .trigger_characters
                                .as_ref()
                                .map(|characters| characters.iter().cloned().collect())
                        })
                        .unwrap_or_default(),
                    cx,
                );
            });
        }
    }

    pub(crate) fn reset_buffer(&mut self, buffer: &Entity<Buffer>, old_file: &File, cx: &mut App) {
        buffer.update(cx, |buffer, cx| {
            let Some(language) = buffer.language() else {
                return;
            };
            let path = ProjectPath {
                worktree_id: old_file.worktree_id(cx),
                path: old_file.path.clone(),
            };
            for server_id in self.language_server_ids_for_project_path(path, language, cx) {
                buffer.update_diagnostics(server_id, DiagnosticSet::new([], buffer), cx);
                buffer.set_completion_triggers(server_id, Default::default(), cx);
            }
        });
    }

    pub(super) fn update_buffer_diagnostics(
        &mut self,
        buffer: &Entity<Buffer>,
        server_id: LanguageServerId,
        registration_id: Option<Option<SharedString>>,
        result_id: Option<SharedString>,
        version: Option<i32>,
        new_diagnostics: Vec<DiagnosticEntry<Unclipped<PointUtf16>>>,
        reused_diagnostics: Vec<DiagnosticEntry<Unclipped<PointUtf16>>>,
        cx: &mut Context<LspStore>,
    ) -> Result<()> {
        fn compare_diagnostics(a: &Diagnostic, b: &Diagnostic) -> Ordering {
            Ordering::Equal
                .then_with(|| b.is_primary.cmp(&a.is_primary))
                .then_with(|| a.is_disk_based.cmp(&b.is_disk_based))
                .then_with(|| a.severity.cmp(&b.severity))
                .then_with(|| a.message.cmp(&b.message))
        }

        let mut diagnostics = Vec::with_capacity(new_diagnostics.len() + reused_diagnostics.len());
        diagnostics.extend(new_diagnostics.into_iter().map(|d| (true, d)));
        diagnostics.extend(reused_diagnostics.into_iter().map(|d| (false, d)));

        diagnostics.sort_unstable_by(|(_, a), (_, b)| {
            Ordering::Equal
                .then_with(|| a.range.start.cmp(&b.range.start))
                .then_with(|| b.range.end.cmp(&a.range.end))
                .then_with(|| compare_diagnostics(&a.diagnostic, &b.diagnostic))
        });

        let snapshot = self.buffer_snapshot_for_lsp_version(buffer, server_id, version, cx)?;

        let edits_since_save = std::cell::LazyCell::new(|| {
            let saved_version = buffer.read(cx).saved_version();
            Patch::new(snapshot.edits_since::<PointUtf16>(saved_version).collect())
        });

        let mut sanitized_diagnostics = Vec::with_capacity(diagnostics.len());

        for (new_diagnostic, entry) in diagnostics {
            let start;
            let end;
            if new_diagnostic && entry.diagnostic.is_disk_based {
                // Some diagnostics are based on files on disk instead of buffers'
                // current contents. Adjust these diagnostics' ranges to reflect
                // any unsaved edits.
                // Do not alter the reused ones though, as their coordinates were stored as anchors
                // and were properly adjusted on reuse.
                start = Unclipped((*edits_since_save).old_to_new(entry.range.start.0));
                end = Unclipped((*edits_since_save).old_to_new(entry.range.end.0));
            } else {
                start = entry.range.start;
                end = entry.range.end;
            }

            let mut range = snapshot.clip_point_utf16(start, Bias::Left)
                ..snapshot.clip_point_utf16(end, Bias::Right);

            // Expand empty ranges by one codepoint
            if range.start == range.end {
                // This will be go to the next boundary when being clipped
                range.end.column += 1;
                range.end = snapshot.clip_point_utf16(Unclipped(range.end), Bias::Right);
                if range.start == range.end && range.end.column > 0 {
                    range.start.column -= 1;
                    range.start = snapshot.clip_point_utf16(Unclipped(range.start), Bias::Left);
                }
            }

            sanitized_diagnostics.push(DiagnosticEntry {
                range,
                diagnostic: entry.diagnostic,
            });
        }
        drop(edits_since_save);

        let set = DiagnosticSet::new(sanitized_diagnostics, &snapshot);
        buffer.update(cx, |buffer, cx| {
            if let Some(registration_id) = registration_id {
                if let Some(abs_path) = File::from_dyn(buffer.file()).map(|f| f.abs_path(cx)) {
                    self.buffer_pull_diagnostics_result_ids
                        .entry(server_id)
                        .or_default()
                        .entry(registration_id)
                        .or_default()
                        .insert(abs_path, result_id);
                }
            }

            buffer.update_diagnostics(server_id, set, cx)
        });

        Ok(())
    }

    pub(super) fn register_language_server_for_invisible_worktree(
        &mut self,
        worktree: &Entity<Worktree>,
        language_server_id: LanguageServerId,
        cx: &mut App,
    ) {
        let worktree = worktree.read(cx);
        let worktree_id = worktree.id();
        debug_assert!(!worktree.is_visible());
        let Some(mut origin_seed) = self
            .language_server_ids
            .iter()
            .find_map(|(seed, state)| (state.id == language_server_id).then(|| seed.clone()))
        else {
            return;
        };
        origin_seed.worktree_id = worktree_id;
        self.language_server_ids
            .entry(origin_seed)
            .or_insert_with(|| UnifiedLanguageServer {
                id: language_server_id,
                project_roots: Default::default(),
            });
    }
}
