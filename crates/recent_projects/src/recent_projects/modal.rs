use super::*;

pub struct RecentProjects {
    pub picker: Entity<Picker<RecentProjectsDelegate>>,
    _subscriptions: Vec<Subscription>,
}

impl ModalView for RecentProjects {
    fn on_before_dismiss(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> workspace::DismissDecision {
        let submenu_focused = self.picker.update(cx, |picker, cx| {
            picker.delegate.actions_menu_handle.is_focused(window, cx)
        });
        workspace::DismissDecision::Dismiss(!submenu_focused)
    }
}

impl RecentProjects {
    fn new(
        delegate: RecentProjectsDelegate,
        fs: Option<Arc<dyn Fs>>,
        rem_width: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let style = delegate.style;
        let picker = cx.new(|cx| {
            Picker::list(delegate, window, cx)
                .list_measure_all()
                .initial_width(rems(rem_width))
                .show_scrollbar(true)
        });

        let picker_focus_handle = picker.focus_handle(cx);
        picker.update(cx, |picker, _| {
            picker.delegate.focus_handle = picker_focus_handle;
        });

        let mut subscriptions = vec![cx.subscribe(&picker, |_, _, _, cx| cx.emit(DismissEvent))];

        if style == ProjectPickerStyle::Popover {
            let picker_focus = picker.focus_handle(cx);
            subscriptions.push(
                cx.on_focus_out(&picker_focus, window, |this, _, window, cx| {
                    let submenu_focused = this.picker.update(cx, |picker, cx| {
                        picker.delegate.actions_menu_handle.is_focused(window, cx)
                    });
                    if !submenu_focused {
                        cx.emit(DismissEvent);
                    }
                }),
            );
        }
        // We do not want to block the UI on a potentially lengthy call to DB, so we're gonna swap
        // out workspace locations once the future runs to completion.
        let db = WorkspaceDb::global(cx);
        cx.spawn_in(window, async move |this, cx| {
            let Some(fs) = fs else { return };
            let workspaces = db
                .recent_project_workspaces(fs.as_ref())
                .await
                .log_err()
                .unwrap_or_default();
            this.update_in(cx, move |this, window, cx| {
                this.picker.update(cx, move |picker, cx| {
                    picker.delegate.set_workspaces(workspaces);
                    picker.update_matches(picker.query(cx), window, cx)
                })
            })
            .ok();
        })
        .detach();
        Self {
            picker,
            _subscriptions: subscriptions,
        }
    }

    pub fn open(
        workspace: &mut Workspace,
        create_new_window: Option<bool>,
        window_project_groups: Vec<ProjectGroupKey>,
        window: &mut Window,
        focus_handle: FocusHandle,
        cx: &mut Context<Workspace>,
    ) {
        let weak = cx.entity().downgrade();
        let open_folders = get_open_folders(workspace, cx);
        let fs = Some(workspace.app_state().fs.clone());

        let create_new_window = create_new_window.unwrap_or_else(|| default_open_in_new_window(cx));

        workspace.toggle_modal(window, cx, |window, cx| {
            let delegate = RecentProjectsDelegate::new(
                weak,
                create_new_window,
                focus_handle,
                open_folders,
                window_project_groups,
                ProjectPickerStyle::Modal,
            );

            Self::new(delegate, fs, 42., window, cx)
        })
    }

    pub fn popover(
        workspace: WeakEntity<Workspace>,
        window_project_groups: Vec<ProjectGroupKey>,
        create_new_window: Option<bool>,
        focus_handle: FocusHandle,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        let (open_folders, fs) = workspace
            .upgrade()
            .map(|workspace| {
                let workspace = workspace.read(cx);
                (
                    get_open_folders(workspace, cx),
                    Some(workspace.app_state().fs.clone()),
                )
            })
            .unwrap_or_else(|| (Vec::new(), None));

        let create_new_window = create_new_window.unwrap_or_else(|| default_open_in_new_window(cx));

        cx.new(|cx| {
            let delegate = RecentProjectsDelegate::new(
                workspace,
                create_new_window,
                focus_handle,
                open_folders,
                window_project_groups,
                ProjectPickerStyle::Popover,
            );
            let list = Self::new(delegate, fs, 20., window, cx);
            list.picker.focus_handle(cx).focus(window, cx);
            list
        })
    }

    fn handle_toggle_open_menu(
        &mut self,
        _: &ToggleActionsMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.picker.update(cx, |picker, cx| {
            let menu_handle = &picker.delegate.actions_menu_handle;
            if menu_handle.is_deployed() {
                menu_handle.hide(cx);
            } else {
                menu_handle.show(window, cx);
            }
        });
    }

    fn handle_remove_selected(
        &mut self,
        _: &RemoveSelected,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.picker.update(cx, |picker, cx| {
            let ix = picker.delegate.selected_index;

            match picker.delegate.filtered_entries.get(ix) {
                Some(ProjectPickerEntry::OpenFolder { index, .. }) => {
                    if let Some(folder) = picker.delegate.open_folders.get(*index) {
                        let worktree_id = folder.worktree_id;
                        let Some(workspace) = picker.delegate.workspace.upgrade() else {
                            return;
                        };
                        workspace.update(cx, |workspace, cx| {
                            let project = workspace.project().clone();
                            project.update(cx, |project, cx| {
                                project.remove_worktree(worktree_id, cx);
                            });
                        });
                        picker.delegate.open_folders = get_open_folders(workspace.read(cx), cx);
                        let query = picker.query(cx);
                        picker.update_matches(query, window, cx);
                    }
                }
                Some(ProjectPickerEntry::ProjectGroup(hit)) => {
                    if let Some(key) = picker
                        .delegate
                        .window_project_groups
                        .get(hit.candidate_id)
                        .cloned()
                    {
                        if picker.delegate.is_active_project_group(&key, cx) {
                            return;
                        }
                        picker.delegate.remove_project_group(key, window, cx);
                        let query = picker.query(cx);
                        picker.update_matches(query, window, cx);
                    }
                }
                Some(ProjectPickerEntry::RecentProject(_)) => {
                    picker.delegate.delete_recent_project(ix, window, cx);
                }
                _ => {}
            }
        });
    }

    fn handle_add_to_workspace(
        &mut self,
        _: &AddToWorkspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.picker.update(cx, |picker, cx| {
            let ix = picker.delegate.selected_index;

            if let Some(ProjectPickerEntry::RecentProject(hit)) =
                picker.delegate.filtered_entries.get(ix)
            {
                if let Some(workspace) = picker.delegate.workspaces.get(hit.candidate_id) {
                    if matches!(workspace.location, SerializedWorkspaceLocation::Local) {
                        let paths_to_add = workspace.paths.paths().to_vec();
                        picker
                            .delegate
                            .add_paths_to_project(paths_to_add, window, cx);
                    }
                }
            }
        });
    }
}

impl EventEmitter<DismissEvent> for RecentProjects {}

impl Focusable for RecentProjects {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.picker.focus_handle(cx)
    }
}

impl Render for RecentProjects {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .key_context("RecentProjects")
            .on_action(cx.listener(Self::handle_toggle_open_menu))
            .on_action(cx.listener(Self::handle_remove_selected))
            .on_action(cx.listener(Self::handle_add_to_workspace))
            .child(self.picker.clone())
    }
}
