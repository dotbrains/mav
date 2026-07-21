use super::*;

impl ProjectPanel {
    pub(super) fn confirm(&mut self, _: &Confirm, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(task) = self.confirm_edit(true, window, cx) {
            task.detach_and_notify_err(self.workspace.clone(), window, cx);
        }
    }

    pub(super) fn populate_validation_error(&mut self, cx: &mut Context<Self>) {
        let edit_state = match self.state.edit_state.as_mut() {
            Some(state) => state,
            None => return,
        };
        let filename = self.filename_editor.read(cx).text(cx);
        if !filename.is_empty() {
            if filename.is_empty() {
                edit_state.validation_state =
                    ValidationState::Error("File or directory name cannot be empty.".to_string());
                cx.notify();
                return;
            }

            let trimmed_filename = filename.trim();
            if trimmed_filename != filename {
                edit_state.validation_state = ValidationState::Warning(
                    "File or directory name contains leading or trailing whitespace.".to_string(),
                );
                cx.notify();
                return;
            }
            let trimmed_filename = trimmed_filename.trim_start_matches('/');

            let Ok(filename) = RelPath::unix(trimmed_filename) else {
                edit_state.validation_state = ValidationState::Warning(
                    "File or directory name contains leading or trailing whitespace.".to_string(),
                );
                cx.notify();
                return;
            };

            if let Some(worktree) = self
                .project
                .read(cx)
                .worktree_for_id(edit_state.worktree_id, cx)
                && let Some(entry) = worktree.read(cx).entry_for_id(edit_state.entry_id)
            {
                let mut already_exists = false;
                if edit_state.is_new_entry() {
                    let new_path = entry.path.join(filename);
                    if worktree.read(cx).entry_for_path(&new_path).is_some() {
                        already_exists = true;
                    }
                } else {
                    let new_path = if let Some(parent) = entry.path.clone().parent() {
                        parent.join(&filename)
                    } else {
                        filename.into()
                    };
                    if let Some(existing) = worktree.read(cx).entry_for_path(&new_path)
                        && existing.id != entry.id
                    {
                        already_exists = true;
                    }
                };
                if already_exists {
                    edit_state.validation_state = ValidationState::Error(format!(
                        "File or directory '{}' already exists at location. Please choose a different name.",
                        filename.as_unix_str()
                    ));
                    cx.notify();
                    return;
                }
            }
        }
        edit_state.validation_state = ValidationState::None;
        cx.notify();
    }

    pub(super) fn confirm_edit(
        &mut self,
        refocus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Task<Result<()>>> {
        let edit_state = self.state.edit_state.as_mut()?;
        let worktree_id = edit_state.worktree_id;
        let is_new_entry = edit_state.is_new_entry();
        let mut filename = self.filename_editor.read(cx).text(cx);
        let path_style = self.project.read(cx).path_style(cx);
        if path_style.is_windows() {
            // on windows, trailing dots are ignored in paths
            // this can cause project panel to create a new entry with a trailing dot
            // while the actual one without the dot gets populated by the file watcher
            while let Some(trimmed) = filename.strip_suffix('.') {
                filename = trimmed.to_string();
            }
        }
        if filename.trim().is_empty() {
            return None;
        }

        let filename_indicates_dir = if path_style.is_windows() {
            filename.ends_with('/') || filename.ends_with('\\')
        } else {
            filename.ends_with('/')
        };
        let filename = if path_style.is_windows() {
            filename.trim_start_matches(&['/', '\\'])
        } else {
            filename.trim_start_matches('/')
        };
        let filename = RelPath::new(filename.as_ref(), path_style).ok()?.into_arc();

        edit_state.is_dir =
            edit_state.is_dir || (edit_state.is_new_entry() && filename_indicates_dir);
        let is_dir = edit_state.is_dir;
        let worktree = self.project.read(cx).worktree_for_id(worktree_id, cx)?;
        let entry = worktree.read(cx).entry_for_id(edit_state.entry_id)?.clone();

        let edit_task;
        let edited_entry_id;
        let edited_entry;
        let new_project_path: ProjectPath;
        if is_new_entry {
            self.selection = Some(SelectedEntry {
                worktree_id,
                entry_id: NEW_ENTRY_ID,
            });
            let new_path = entry.path.join(&filename);
            if worktree.read(cx).entry_for_path(&new_path).is_some() {
                return None;
            }

            edited_entry = None;
            edited_entry_id = NEW_ENTRY_ID;
            new_project_path = (worktree_id, new_path).into();
            edit_task = self.project.update(cx, |project, cx| {
                project.create_entry(new_project_path.clone(), is_dir, cx)
            });
        } else {
            let new_path = if let Some(parent) = entry.path.parent() {
                parent.join(&filename)
            } else {
                filename.clone()
            };
            if let Some(existing) = worktree.read(cx).entry_for_path(&new_path) {
                if existing.id == entry.id && refocus {
                    window.focus(&self.focus_handle, cx);
                }
                return None;
            }
            edited_entry_id = entry.id;
            edited_entry = Some(entry);
            new_project_path = (worktree_id, new_path).into();
            edit_task = self.project.update(cx, |project, cx| {
                project.rename_entry(edited_entry_id, new_project_path.clone(), cx)
            })
        };

        if refocus {
            window.focus(&self.focus_handle, cx);
        }
        edit_state.processing_filename = Some(filename);
        cx.notify();

        Some(cx.spawn_in(window, async move |project_panel, cx| {
            let new_entry = edit_task.await;
            project_panel.update(cx, |project_panel, cx| {
                project_panel.state.edit_state = None;

                // Record the operation if the edit was applied
                if new_entry.is_ok() {
                    let operation = if let Some(old_entry) = edited_entry {
                        Change::Renamed((worktree_id, old_entry.path).into(), new_project_path)
                    } else {
                        Change::Created(new_project_path)
                    };

                    project_panel.undo_manager.record([operation]).log_err();
                }

                cx.notify();
            })?;

            match new_entry {
                Err(e) => {
                    project_panel
                        .update_in(cx, |project_panel, window, cx| {
                            project_panel.marked_entries.clear();
                            project_panel.update_visible_entries(None, false, false, window, cx);
                        })
                        .ok();
                    Err(e)?;
                }
                Ok(CreatedEntry::Included(new_entry)) => {
                    project_panel.update_in(cx, |project_panel, window, cx| {
                        if let Some(selection) = &mut project_panel.selection
                            && selection.entry_id == edited_entry_id
                        {
                            selection.worktree_id = worktree_id;
                            selection.entry_id = new_entry.id;
                            project_panel.marked_entries.clear();
                            project_panel.expand_to_selection(cx);
                        }
                        project_panel.update_visible_entries(None, false, false, window, cx);
                        if is_new_entry && !is_dir {
                            let settings = ProjectPanelSettings::get_global(cx);
                            if settings.auto_open.should_open_on_create() {
                                project_panel.open_entry(new_entry.id, true, false, cx);
                            }
                        }
                        cx.notify();
                    })?;
                }
                Ok(CreatedEntry::Excluded { abs_path }) => {
                    if let Some(open_task) = project_panel
                        .update_in(cx, |project_panel, window, cx| {
                            project_panel.marked_entries.clear();
                            project_panel.update_visible_entries(None, false, false, window, cx);

                            if is_dir {
                                project_panel.project.update(cx, |_, cx| {
                                    cx.emit(project::Event::Toast {
                                        notification_id: "excluded-directory".into(),
                                        message: format!(
                                            concat!(
                                                "Created an excluded directory at {:?}.\n",
                                                "Alter `file_scan_exclusions` in the settings ",
                                                "to show it in the panel"
                                            ),
                                            abs_path
                                        ),
                                        link: None,
                                    })
                                });
                                None
                            } else {
                                project_panel
                                    .workspace
                                    .update(cx, |workspace, cx| {
                                        workspace.open_abs_path(
                                            abs_path,
                                            OpenOptions {
                                                visible: Some(OpenVisible::All),
                                                ..Default::default()
                                            },
                                            window,
                                            cx,
                                        )
                                    })
                                    .ok()
                            }
                        })
                        .ok()
                        .flatten()
                    {
                        let _ = open_task.await?;
                    }
                }
            }
            Ok(())
        }))
    }

    pub(super) fn discard_edit_state(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(edit_state) = self.state.edit_state.take() {
            self.state.temporarily_unfolded_pending_state = edit_state
                .temporarily_unfolded
                .and_then(|temporarily_unfolded_entry_id| {
                    let previously_focused_leaf_entry = edit_state.previously_focused?;
                    let folded_ancestors =
                        self.state.ancestors.get(&temporarily_unfolded_entry_id)?;
                    Some(TemporaryUnfoldedPendingState {
                        previously_focused_leaf_entry,
                        temporarily_unfolded_active_entry_id: folded_ancestors
                            .active_ancestor()
                            .unwrap_or(temporarily_unfolded_entry_id),
                    })
                });
            let previously_focused = edit_state
                .previously_focused
                .map(|entry| (entry.worktree_id, entry.entry_id));
            self.update_visible_entries(
                previously_focused,
                false,
                previously_focused.is_some(),
                window,
                cx,
            );
        }
    }

    pub(super) fn cancel(&mut self, _: &menu::Cancel, window: &mut Window, cx: &mut Context<Self>) {
        if cx.stop_active_drag(window) {
            self.drag_target_entry.take();
            self.hover_expand_task.take();
            return;
        }
        self.marked_entries.clear();
        cx.notify();
        self.discard_edit_state(window, cx);
        window.focus(&self.focus_handle, cx);
    }

    pub(super) fn open_entry(
        &mut self,
        entry_id: ProjectEntryId,
        focus_opened_item: bool,
        allow_preview: bool,

        cx: &mut Context<Self>,
    ) {
        cx.emit(Event::OpenedEntry {
            entry_id,
            focus_opened_item,
            allow_preview,
        });
    }

    pub(super) fn split_entry(
        &mut self,
        entry_id: ProjectEntryId,
        allow_preview: bool,
        split_direction: Option<SplitDirection>,

        cx: &mut Context<Self>,
    ) {
        cx.emit(Event::SplitEntry {
            entry_id,
            allow_preview,
            split_direction,
        });
    }

    pub(super) fn new_file(&mut self, _: &NewFile, window: &mut Window, cx: &mut Context<Self>) {
        self.add_entry(false, window, cx)
    }

    pub(super) fn new_directory(
        &mut self,
        _: &NewDirectory,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.add_entry(true, window, cx)
    }

    pub(super) fn add_entry(&mut self, is_dir: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some((worktree_id, entry_id)) = self
            .selection
            .map(|entry| (entry.worktree_id, entry.entry_id))
            .or_else(|| {
                let entry_id = self.state.last_worktree_root_id?;
                let worktree_id = self
                    .project
                    .read(cx)
                    .worktree_for_entry(entry_id, cx)?
                    .read(cx)
                    .id();

                self.selection = Some(SelectedEntry {
                    worktree_id,
                    entry_id,
                });

                Some((worktree_id, entry_id))
            })
        else {
            return;
        };

        let directory_id;
        let new_entry_id = self.resolve_entry(entry_id);
        if let Some(worktree) = self.project.read(cx).worktree_for_id(worktree_id, cx) {
            let worktree = worktree.read(cx);
            let expanded_dir_ids = match self.state.expanded_dir_ids.entry(worktree_id) {
                hash_map::Entry::Occupied(entry) => entry.into_mut(),
                hash_map::Entry::Vacant(entry) => {
                    let Some(root_entry_id) = worktree.root_entry().map(|entry| entry.id) else {
                        return;
                    };
                    entry.insert(vec![root_entry_id])
                }
            };

            if let Some(mut entry) = worktree.entry_for_id(new_entry_id) {
                loop {
                    if entry.is_dir() {
                        if let Err(ix) = expanded_dir_ids.binary_search(&entry.id) {
                            expanded_dir_ids.insert(ix, entry.id);
                        }
                        directory_id = entry.id;
                        break;
                    } else {
                        if let Some(parent_path) = entry.path.parent()
                            && let Some(parent_entry) = worktree.entry_for_path(parent_path)
                        {
                            entry = parent_entry;
                            continue;
                        }
                        return;
                    }
                }
            } else {
                return;
            };
        } else {
            return;
        };

        self.marked_entries.clear();
        self.state.edit_state = Some(EditState {
            worktree_id,
            entry_id: directory_id,
            leaf_entry_id: None,
            is_dir,
            processing_filename: None,
            previously_focused: self.selection,
            depth: 0,
            validation_state: ValidationState::None,
            temporarily_unfolded: (new_entry_id != entry_id).then_some(new_entry_id),
        });
        self.filename_editor.update(cx, |editor, cx| {
            editor.clear(window, cx);
        });
        self.update_visible_entries(Some((worktree_id, NEW_ENTRY_ID)), true, true, window, cx);
        cx.notify();
    }

    pub(super) fn unflatten_entry_id(&self, leaf_entry_id: ProjectEntryId) -> ProjectEntryId {
        if let Some(ancestors) = self.state.ancestors.get(&leaf_entry_id) {
            ancestors
                .ancestors
                .get(ancestors.current_ancestor_depth)
                .copied()
                .unwrap_or(leaf_entry_id)
        } else {
            leaf_entry_id
        }
    }
}
