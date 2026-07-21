use super::*;

impl ProjectPanel {
    fn new(
        workspace: &mut Workspace,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Entity<Self> {
        let project = workspace.project().clone();
        let git_store = project.read(cx).git_store().clone();
        let path_style = project.read(cx).path_style(cx);
        let project_panel = cx.new(|cx| {
            let focus_handle = cx.focus_handle();
            cx.on_focus(&focus_handle, window, Self::focus_in).detach();

            cx.subscribe_in(
                &git_store,
                window,
                |this, _, event, window, cx| match event {
                    GitStoreEvent::RepositoryUpdated(_, RepositoryEvent::StatusesChanged, _)
                    | GitStoreEvent::RepositoryAdded
                    | GitStoreEvent::RepositoryRemoved(_) => {
                        this.update_visible_entries(None, false, false, window, cx);
                        cx.notify();
                    }
                    _ => {}
                },
            )
            .detach();

            cx.subscribe_in(
                &project,
                window,
                |this, project, event, window, cx| match event {
                    project::Event::ActiveEntryChanged(Some(entry_id)) => {
                        if ProjectPanelSettings::get_global(cx).auto_reveal_entries {
                            this.reveal_entry(project.clone(), *entry_id, true, window, cx)
                                .ok();
                        }
                    }
                    project::Event::ActiveEntryChanged(None) => {
                        let is_active_item_file_diff_view = this
                            .workspace
                            .upgrade()
                            .and_then(|ws| ws.read(cx).active_item(cx))
                            .map(|item| {
                                item.act_as_type(TypeId::of::<FileDiffView>(), cx).is_some()
                            })
                            .unwrap_or(false);
                        if !is_active_item_file_diff_view {
                            this.marked_entries.clear();
                        }
                    }
                    project::Event::RevealInProjectPanel(entry_id) => {
                        if let Some(()) = this
                            .reveal_entry(project.clone(), *entry_id, false, window, cx)
                            .log_err()
                        {
                            cx.emit(PanelEvent::Activate);
                        }
                    }
                    project::Event::ActivateProjectPanel => {
                        cx.emit(PanelEvent::Activate);
                    }
                    project::Event::DiskBasedDiagnosticsFinished { .. }
                    | project::Event::DiagnosticsUpdated { .. } => {
                        if ProjectPanelSettings::get_global(cx).show_diagnostics
                            != ShowDiagnostics::Off
                        {
                            this.diagnostic_summary_update = cx.spawn(async move |this, cx| {
                                cx.background_executor()
                                    .timer(Duration::from_millis(30))
                                    .await;
                                this.update(cx, |this, cx| {
                                    this.update_diagnostics(cx);
                                    cx.notify();
                                })
                                .log_err();
                            });
                        }
                    }
                    project::Event::WorktreeRemoved(id) => {
                        this.state.expanded_dir_ids.remove(id);
                        this.update_visible_entries(None, false, false, window, cx);
                        cx.notify();
                    }
                    project::Event::WorktreeUpdatedEntries(_, _)
                    | project::Event::WorktreeAdded(_)
                    | project::Event::WorktreeOrderChanged => {
                        this.update_visible_entries(None, false, false, window, cx);
                        cx.notify();
                    }
                    project::Event::ExpandedAllForEntry(worktree_id, entry_id) => {
                        this.synchronously_expand_all_directories(
                            *worktree_id,
                            *entry_id,
                            window,
                            cx,
                        );
                    }
                    _ => {}
                },
            )
            .detach();

            let trash_action = [TypeId::of::<Trash>()];
            let is_remote = project.read(cx).is_remote();

            // Make sure the trash option is never displayed anywhere on remote
            // hosts since they may not support trashing. May want to dynamically
            // detect this in the future.
            if is_remote {
                CommandPaletteFilter::update_global(cx, |filter, _cx| {
                    filter.hide_action_types(&trash_action);
                });
            }

            let filename_editor = cx.new(|cx| Editor::single_line(window, cx));

            cx.subscribe_in(
                &filename_editor,
                window,
                |project_panel, _, editor_event, window, cx| match editor_event {
                    EditorEvent::BufferEdited => {
                        project_panel.populate_validation_error(cx);
                        project_panel.autoscroll(cx);
                    }
                    EditorEvent::SelectionsChanged { .. } => {
                        project_panel.autoscroll(cx);
                    }
                    EditorEvent::Blurred => {
                        if project_panel
                            .state
                            .edit_state
                            .as_ref()
                            .is_some_and(|state| state.processing_filename.is_none())
                        {
                            match project_panel.confirm_edit(false, window, cx) {
                                Some(task) => {
                                    task.detach_and_notify_err(
                                        project_panel.workspace.clone(),
                                        window,
                                        cx,
                                    );
                                }
                                None => {
                                    project_panel.discard_edit_state(window, cx);
                                }
                            }
                        }
                    }
                    _ => {}
                },
            )
            .detach();

            cx.observe_global::<FileIcons>(|_, cx| {
                cx.notify();
            })
            .detach();

            let mut project_panel_settings = *ProjectPanelSettings::get_global(cx);
            cx.observe_global_in::<SettingsStore>(window, move |this, window, cx| {
                let new_settings = *ProjectPanelSettings::get_global(cx);
                if project_panel_settings != new_settings {
                    if project_panel_settings.hide_gitignore != new_settings.hide_gitignore {
                        this.update_visible_entries(None, false, false, window, cx);
                    }
                    if project_panel_settings.hide_root != new_settings.hide_root {
                        this.update_visible_entries(None, false, false, window, cx);
                    }
                    if project_panel_settings.hide_hidden != new_settings.hide_hidden {
                        this.update_visible_entries(None, false, false, window, cx);
                    }
                    if project_panel_settings.sort_mode != new_settings.sort_mode {
                        this.update_visible_entries(None, false, false, window, cx);
                    }
                    if project_panel_settings.sort_order != new_settings.sort_order {
                        this.update_visible_entries(None, false, false, window, cx);
                    }
                    if project_panel_settings.sticky_scroll && !new_settings.sticky_scroll {
                        this.sticky_items_count = 0;
                    }
                    project_panel_settings = new_settings;
                    this.update_diagnostics(cx);
                    cx.notify();
                }
            })
            .detach();

            let scroll_handle = UniformListScrollHandle::new();
            let weak_project_panel = cx.weak_entity();
            let mut this = Self {
                project: project.clone(),
                hover_scroll_task: None,
                fs: workspace.app_state().fs.clone(),
                focus_handle,
                rendered_entries_len: 0,
                folded_directory_drag_target: None,
                drag_target_entry: None,
                marked_entries: Default::default(),
                selection: None,
                context_menu: None,
                filename_editor,
                clipboard: None,
                _dragged_entry_destination: None,
                workspace: workspace.weak_handle(),
                diagnostics: Default::default(),
                diagnostic_counts: Default::default(),
                diagnostic_summary_update: Task::ready(()),
                scroll_handle,
                mouse_down: false,
                hover_expand_task: None,
                previous_drag_position: None,
                sticky_items_count: 0,
                last_reported_update: Instant::now(),
                state: State {
                    max_width_item_index: None,
                    edit_state: None,
                    temporarily_unfolded_pending_state: None,
                    last_worktree_root_id: Default::default(),
                    visible_entries: Default::default(),
                    ancestors: Default::default(),
                    expanded_dir_ids: Default::default(),
                    unfolded_dir_ids: Default::default(),
                },
                update_visible_entries_task: Default::default(),
                undo_manager: UndoManager::new(workspace.weak_handle(), weak_project_panel, &cx),
            };
            this.update_visible_entries(None, false, false, window, cx);

            this
        });

        cx.subscribe_in(&project_panel, window, {
            let project_panel = project_panel.downgrade();
            move |workspace, _, event, window, cx| match event {
                &Event::OpenedEntry {
                    entry_id,
                    focus_opened_item,
                    allow_preview,
                } => {
                    if let Some(worktree) = project.read(cx).worktree_for_entry(entry_id, cx)
                        && let Some(entry) = worktree.read(cx).entry_for_id(entry_id) {
                            let file_path = entry.path.clone();
                            let worktree_id = worktree.read(cx).id();
                            let entry_id = entry.id;
                            let is_via_ssh = project.read(cx).is_via_remote_server();

                            workspace
                                .open_path_preview_in_tabbed_pane(
                                    ProjectPath {
                                        worktree_id,
                                        path: file_path.clone(),
                                    },
                                    None,
                                    focus_opened_item,
                                    allow_preview,
                                    true,
                                    window, cx,
                                )
                                .detach_and_prompt_err("Failed to open file", window, cx, move |e, _, _| {
                                    match e.error_code() {
                                        ErrorCode::Disconnected => if is_via_ssh {
                                            Some("Disconnected from SSH host".to_string())
                                        } else {
                                            Some("Disconnected from remote project".to_string())
                                        },
                                        ErrorCode::UnsharedItem => Some(format!(
                                            "{} is not shared by the host. This could be because it has been marked as `private`",
                                            file_path.display(path_style)
                                        )),
                                        // See note in worktree.rs where this error originates. Returning Some in this case prevents
                                        // the error popup from saying "Try Again", which is a red herring in this case
                                        ErrorCode::Internal if e.to_string().contains("File is too large to load") => Some(e.to_string()),
                                        _ => None,
                                    }
                                });

                            if let Some(project_panel) = project_panel.upgrade() {
                                // Always select and mark the entry, regardless of whether it is opened or not.
                                project_panel.update(cx, |project_panel, _| {
                                    let entry = SelectedEntry { worktree_id, entry_id };
                                    project_panel.marked_entries.clear();
                                    project_panel.marked_entries.push(entry);
                                    project_panel.selection = Some(entry);
                                });
                                if !focus_opened_item {
                                    let focus_handle = project_panel.read(cx).focus_handle.clone();
                                    window.focus(&focus_handle, cx);
                                }
                            }
                        }
                }
                &Event::SplitEntry {
                    entry_id,
                    allow_preview,
                    split_direction,
                } => {
                    if let Some(worktree) = project.read(cx).worktree_for_entry(entry_id, cx)
                        && let Some(entry) = worktree.read(cx).entry_for_id(entry_id) {
                            workspace
                                .split_path_preview(
                                    ProjectPath {
                                        worktree_id: worktree.read(cx).id(),
                                        path: entry.path.clone(),
                                    },
                                    allow_preview,
                                    split_direction,
                                    window, cx,
                                )
                                .detach_and_log_err(cx);
                        }
                }

                _ => {}
            }
        })
        .detach();

        project_panel
    }

    pub async fn load(
        workspace: WeakEntity<Workspace>,
        mut cx: AsyncWindowContext,
    ) -> Result<Entity<Self>> {
        workspace.update_in(&mut cx, |workspace, window, cx| {
            ProjectPanel::new(workspace, window, cx)
        })
    }
}
