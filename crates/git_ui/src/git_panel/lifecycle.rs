use super::*;

impl GitPanel {
    // Only the test-support constructors call this thin wrapper now; production
    // registration goes through `new_with_serialized_panel` directly. Gate it to
    // the same cfg as `new_test` so the non-test lib build doesn't see it as dead.
    #[cfg(any(test, feature = "test-support"))]
    fn new(
        workspace: &mut Workspace,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Entity<Self> {
        Self::new_with_serialized_panel(workspace, None, window, cx)
    }

    pub(super) fn new_with_serialized_panel(
        workspace: &mut Workspace,
        serialized_panel: Option<SerializedGitPanel>,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Entity<Self> {
        let project = workspace.project().clone();
        let app_state = workspace.app_state().clone();
        let fs = app_state.fs.clone();
        let git_store = project.read(cx).git_store().clone();
        let active_repository = project.read(cx).active_repository(cx);
        let signoff_enabled = serialized_panel
            .as_ref()
            .is_some_and(|panel| panel.signoff_enabled);
        let active_work_directory_abs_path = active_repository.as_ref().map(|repository| {
            repository
                .read(cx)
                .work_directory_abs_path
                .to_string_lossy()
                .into_owned()
        });
        let active_draft = serialized_panel.as_ref().and_then(|panel| {
            let path = active_work_directory_abs_path.as_ref()?;
            panel.commit_messages.get(path)
        });
        // Seed the placeholder editor with the restored draft when the active
        // repository already matches the serialized one, so the message is
        // present immediately on restart instead of only after the commit
        // buffer finishes loading in `reopen_commit_buffer`. Sourced from the
        // serialized draft rather than a live buffer snapshot and scoped to the
        // matching repository, so it neither replays cleared text nor leaks a
        // draft across repositories. `reopen_commit_buffer` still performs the
        // one-shot restore into the loaded buffer; applying the same draft
        // there is idempotent.
        let amend_pending = active_draft.is_some_and(|draft| draft.amend_pending);
        let original_commit_message = active_draft.and_then(|draft| draft.original_message.clone());
        let initial_commit_message = active_draft
            .and_then(|draft| draft.message.clone())
            .unwrap_or_default();
        let pending_commit_message_restores = serialized_panel
            .map(|panel| panel.commit_messages)
            .unwrap_or_default();

        cx.new(|cx| {
            let focus_handle = cx.focus_handle();
            cx.on_focus(&focus_handle, window, Self::focus_in).detach();

            let mut was_sort_by = GitPanelSettings::get_global(cx).sort_by;
            let mut was_group_by = GitPanelSettings::get_global(cx).group_by;
            let mut was_tree_view = GitPanelSettings::get_global(cx).tree_view;
            let mut was_file_icons = GitPanelSettings::get_global(cx).file_icons;
            let mut was_folder_icons = GitPanelSettings::get_global(cx).folder_icons;
            let mut was_diff_stats = GitPanelSettings::get_global(cx).diff_stats;
            cx.observe_global_in::<SettingsStore>(window, move |this, window, cx| {
                let settings = GitPanelSettings::get_global(cx);
                let sort_by = settings.sort_by;
                let group_by = settings.group_by;
                let tree_view = settings.tree_view;
                let file_icons = settings.file_icons;
                let folder_icons = settings.folder_icons;
                let diff_stats = settings.diff_stats;
                if tree_view != was_tree_view {
                    match (&mut this.view_mode, tree_view) {
                        (GitPanelViewMode::Tree(state), false) => {
                            this.tree_expanded_dirs = state.expanded_dirs.clone();
                            this.view_mode = GitPanelViewMode::Flat;
                        }
                        (GitPanelViewMode::Flat, true) => {
                            this.view_mode = GitPanelViewMode::Tree(TreeViewState {
                                expanded_dirs: this.tree_expanded_dirs.clone(),
                                ..Default::default()
                            });
                        }
                        _ => {}
                    }
                }

                let mut update_entries = false;
                if sort_by != was_sort_by || group_by != was_group_by || tree_view != was_tree_view
                {
                    this.bulk_staging.take();
                    update_entries = true;
                }
                if (diff_stats != was_diff_stats) || update_entries {
                    this.update_visible_entries(window, cx);
                }
                if file_icons != was_file_icons || folder_icons != was_folder_icons {
                    cx.notify();
                }
                was_sort_by = sort_by;
                was_group_by = group_by;
                was_tree_view = tree_view;
                was_file_icons = file_icons;
                was_folder_icons = folder_icons;
                was_diff_stats = diff_stats;
            })
            .detach();

            cx.observe_global::<FileIcons>(|_, cx| {
                cx.notify();
            })
            .detach();

            // just to let us render a placeholder editor.
            // Once the active git repo is set, this buffer will be replaced.
            let temporary_buffer = cx.new(|cx| Buffer::local(initial_commit_message, cx));
            let commit_editor = cx.new(|cx| {
                commit_message_editor(temporary_buffer, None, project.clone(), true, window, cx)
            });

            let scroll_handle = UniformListScrollHandle::new();

            let mut was_ai_enabled = AgentSettings::get_global(cx).enabled(cx);
            let _settings_subscription = cx.observe_global::<SettingsStore>(move |_, cx| {
                let is_ai_enabled = AgentSettings::get_global(cx).enabled(cx);
                if was_ai_enabled != is_ai_enabled {
                    was_ai_enabled = is_ai_enabled;
                    cx.notify();
                }
            });

            let registry = LanguageModelRegistry::global(cx);
            cx.subscribe(&registry, |_, _, event, cx| match event {
                LanguageModelEvent::CommitMessageModelChanged
                | LanguageModelEvent::DefaultModelChanged
                | LanguageModelEvent::ProviderStateChanged(_)
                | LanguageModelEvent::AddedProvider(_)
                | LanguageModelEvent::RemovedProvider(_)
                | LanguageModelEvent::ProvidersChanged => {
                    cx.notify();
                }
                _ => {}
            })
            .detach();

            cx.subscribe_in(
                &git_store,
                window,
                move |this, _git_store, event, window, cx| match event {
                    GitStoreEvent::RepositoryUpdated(
                        _,
                        RepositoryEvent::StatusesChanged | RepositoryEvent::HeadChanged,
                        true,
                    )
                    | GitStoreEvent::RepositoryAdded
                    | GitStoreEvent::RepositoryRemoved(_)
                    | GitStoreEvent::GlobalConfigurationUpdated
                    | GitStoreEvent::ActiveRepositoryChanged(_) => {
                        this.schedule_update(window, cx);
                    }
                    GitStoreEvent::IndexWriteError(error) => {
                        this.workspace
                            .update(cx, |workspace, cx| {
                                workspace.show_error(format!("{error}"), cx);
                            })
                            .ok();
                    }
                    GitStoreEvent::RepositoryUpdated(_, _, _) => {}
                    GitStoreEvent::JobsUpdated | GitStoreEvent::ConflictsUpdated => {}
                },
            )
            .detach();

            let mut this = Self {
                active_repository,
                commit_editor,
                commit_editor_expanded: false,
                conflicted_count: 0,
                conflicted_staged_count: 0,
                add_coauthors: true,
                generate_commit_message_task: None,
                entries: Vec::new(),
                view_mode: GitPanelViewMode::from_settings(cx),
                tree_expanded_dirs: HashMap::default(),
                entries_indices: HashMap::default(),
                focus_handle: cx.focus_handle(),
                fs,
                new_count: 0,
                new_staged_count: 0,
                changes_count: 0,
                diff_stat_total: DiffStat::default(),
                pending_commit: None,
                pending_remote_operation: None,
                amend_pending,
                original_commit_message,
                pending_commit_message_restores,
                signoff_enabled,
                pending_serialization: Task::ready(()),
                single_staged_entry: None,
                single_tracked_entry: None,
                project,
                scroll_handle,
                max_width_item_index: None,
                selected_entry: None,
                marked_entries: Vec::new(),
                tracked_count: 0,
                tracked_staged_count: 0,
                update_visible_entries_task: Task::ready(()),
                reopen_commit_buffer_task: Task::ready(()),
                show_placeholders: false,
                local_committer: None,
                local_committer_task: None,
                commit_template: None,
                context_menu: None,
                workspace: workspace.weak_handle(),
                modal_open: false,
                entry_count: 0,
                bulk_staging: None,
                stash_entries: Default::default(),
                active_tab: GitPanelTab::Changes,
                commit_history_scroll_handle: UniformListScrollHandle::new(),
                commit_history_shas: None,
                focused_history_entry: None,
                history_keyboard_nav: false,
                _commit_message_buffer_subscription: None,
                _repo_subscriptions: Vec::new(),
                _settings_subscription,
                git_access: GitAccess::Yes,
            };

            this.schedule_update(window, cx);
            this
        })
    }
}
