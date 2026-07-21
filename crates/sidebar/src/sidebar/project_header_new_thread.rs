use super::*;

impl Sidebar {
    pub(super) fn render_new_thread_button(
        &self,
        ix: usize,
        id_prefix: &str,
        key: &ProjectGroupKey,
        group_name: &SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focus_handle = self.focus_handle.clone();

        let menu_handle = self
            .project_header_new_thread_menu_handles
            .get(&ix)
            .cloned()
            .unwrap_or_default();
        let is_menu_open = menu_handle.is_deployed();

        let button = IconButton::new(
            SharedString::from(format!("{id_prefix}project-header-new-thread-{ix}")),
            IconName::Plus,
        )
        .selected_style(ButtonStyle::Tinted(TintColor::Accent))
        .icon_size(IconSize::Small)
        .when(!is_menu_open, |this| this.visible_on_hover(group_name));

        let open_workspaces = self
            .multi_workspace
            .upgrade()
            .and_then(|mw| mw.read(cx).workspaces_for_project_group(key, cx))
            .unwrap_or_default();

        if open_workspaces.is_empty() {
            let key = key.clone();
            return button
                .tooltip(move |_, cx| {
                    Tooltip::for_action_in("Start New Agent Thread", &NewThread, &focus_handle, cx)
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.set_group_expanded(&key, true, cx);
                    this.selection = None;
                    if let Some(workspace) = this.workspace_for_group(&key, cx) {
                        this.create_new_entry(&workspace, window, cx);
                    } else {
                        this.open_workspace_and_create_entry(
                            &key,
                            NewEntryTarget::LastCreatedKind,
                            window,
                            cx,
                        );
                    }
                }))
                .into_any_element();
        }

        let this = cx.weak_entity();
        let key = key.clone();

        PopoverMenu::new(SharedString::from(format!(
            "{id_prefix}project-header-new-thread-menu-{ix}"
        )))
        .with_handle(menu_handle)
        .trigger_with_tooltip(button, move |_, cx| {
            Tooltip::for_action_in("Start New Agent Thread", &NewThread, &focus_handle, cx)
        })
        .anchor(gpui::Anchor::TopLeft)
        .on_open(Rc::new({
            let this = this.clone();
            move |_window, cx| {
                this.update(cx, |_sidebar, cx| cx.notify()).ok();
            }
        }))
        .menu(move |window, cx| {
            let this = this.clone();
            let key = key.clone();
            let open_workspaces = open_workspaces.clone();
            let active_workspace = this
                .read_with(cx, |sidebar, cx| {
                    sidebar
                        .multi_workspace
                        .upgrade()
                        .map(|mw| mw.read(cx).workspace().clone())
                })
                .ok()
                .flatten();
            let workspace_labels: Vec<_> = open_workspaces
                .iter()
                .map(|workspace| workspace_menu_worktree_labels(workspace, cx))
                .collect();

            Some(ContextMenu::build(
                window,
                cx,
                move |mut menu, _window, cx| {
                    menu = menu.header("New Thread In…");

                    for (workspace, labels) in open_workspaces
                        .iter()
                        .cloned()
                        .zip(workspace_labels.iter().cloned())
                    {
                        let is_active_workspace = active_workspace.as_ref() == Some(&workspace);
                        menu = menu.custom_entry(
                            move |_window, _cx| {
                                h_flex()
                                    .w_full()
                                    .gap_2()
                                    .justify_between()
                                    .child(h_flex().min_w_0().gap_1().children(
                                        labels.iter().enumerate().map(|(label_ix, label)| {
                                            h_flex()
                                                .gap_1()
                                                .when(label_ix > 0, |this| {
                                                    this.child(Label::new("•").alpha(0.25))
                                                })
                                                .child(label.render())
                                                .into_any_element()
                                        }),
                                    ))
                                    .when(is_active_workspace, |this| {
                                        this.child(
                                            Icon::new(IconName::Check)
                                                .size(IconSize::Small)
                                                .color(Color::Accent),
                                        )
                                    })
                                    .into_any_element()
                            },
                            {
                                let this = this.clone();
                                let key = key.clone();
                                let workspace = workspace.clone();
                                move |window, cx| {
                                    this.update(cx, |sidebar, cx| {
                                        sidebar.set_group_expanded(&key, true, cx);
                                        sidebar.selection = None;
                                        sidebar.create_new_entry(&workspace, window, cx);
                                    })
                                    .ok();
                                }
                            },
                        );
                    }

                    let base_workspace = active_workspace
                        .as_ref()
                        .filter(|workspace| open_workspaces.contains(workspace))
                        .cloned()
                        .or_else(|| open_workspaces.first().cloned());

                    // Only offer worktree creation when the base project can
                    // actually create one; otherwise the submenu would expand to
                    // nothing. Mirrors the picker's `creation_blocked_reason`.
                    let creation_blocked = base_workspace.as_ref().is_none_or(|base_workspace| {
                        let project = base_workspace.read(cx).project().read(cx);
                        project.is_via_collab() || project.repositories(cx).is_empty()
                    });

                    if let Some(base_workspace) = base_workspace.filter(|_| !creation_blocked) {
                        menu = menu.separator().submenu("Create New Worktree…", {
                            let this = this.clone();
                            move |mut submenu, _window, submenu_cx| {
                                let project = base_workspace.read(submenu_cx).project().clone();
                                let project_ref = project.read(submenu_cx);
                                let has_multiple_repositories =
                                    project_ref.repositories(submenu_cx).len() > 1;
                                let current_branch =
                                    project_ref.active_repository(submenu_cx).and_then(|repo| {
                                        repo.read(submenu_cx)
                                            .branch
                                            .as_ref()
                                            .map(|branch| branch.name().to_string())
                                    });
                                let default_branch = this
                                    .read_with(submenu_cx, |sidebar, _| {
                                        match sidebar.worktree_default_branches.get(&key) {
                                            Some(DefaultBranchCache::Resolved(branch)) => {
                                                branch.clone()
                                            }
                                            _ => None,
                                        }
                                    })
                                    .ok()
                                    .flatten();

                                let targets = worktree_create_targets(
                                    has_multiple_repositories,
                                    default_branch,
                                    current_branch.as_deref(),
                                );
                                for target in targets {
                                    let label = format!(
                                        "Based on {}",
                                        target.branch_label(
                                            has_multiple_repositories,
                                            current_branch.as_deref(),
                                        )
                                    );
                                    let branch_target = target.branch_target();
                                    let workspace = base_workspace.clone();
                                    submenu = submenu.entry(label, None, move |window, cx| {
                                        create_worktree_in_workspace(
                                            &workspace,
                                            branch_target.clone(),
                                            window,
                                            cx,
                                        );
                                    });
                                }

                                submenu
                            }
                        });
                    }

                    menu
                },
            ))
        })
        .anchor(gpui::Anchor::TopRight)
        .offset(gpui::Point {
            x: px(0.),
            y: px(1.),
        })
        .into_any_element()
    }

    // Warms `worktree_default_branches` for every project group with at least one
    // open workspace. The git query runs off the menu path so the submenu can read
    // the result synchronously when it opens. Worktrees of a repository share the
    // same default branch, so any workspace in the group yields the same answer.
    pub(super) fn prefetch_worktree_default_branches(&mut self, cx: &mut Context<Self>) {
        let Some(multi_workspace) = self.multi_workspace.upgrade() else {
            return;
        };
        let keys: Vec<ProjectGroupKey> = self
            .contents
            .entries
            .iter()
            .filter_map(|entry| match entry {
                ListEntry::ProjectHeader { key, .. } => Some(key.clone()),
                _ => None,
            })
            .collect();
        for key in keys {
            if self.worktree_default_branches.contains_key(&key) {
                continue;
            }
            let Some(base) = multi_workspace
                .read(cx)
                .workspaces_for_project_group(&key, cx)
                .and_then(|workspaces| workspaces.first().cloned())
            else {
                continue;
            };
            self.prefetch_worktree_default_branch(&key, &base, cx);
        }
    }

    fn prefetch_worktree_default_branch(
        &mut self,
        key: &ProjectGroupKey,
        workspace: &Entity<Workspace>,
        cx: &mut Context<Self>,
    ) {
        // Presence of the key means the group is already pending or resolved. The
        // no-repository case is deliberately not inserted so it retries on a
        // later rebuild once the repository has finished loading.
        if self.worktree_default_branches.contains_key(key) {
            return;
        }
        let Some(repository) = workspace.read(cx).project().read(cx).active_repository(cx) else {
            return;
        };
        let request = repository.update(cx, |repository, _| repository.default_branch(true));
        self.worktree_default_branches
            .insert(key.clone(), DefaultBranchCache::Pending);
        let key = key.clone();
        cx.spawn(async move |this, cx| {
            let default_branch = request.await.ok().and_then(Result::ok).flatten();
            let parsed = default_branch.as_deref().and_then(RemoteBranchName::parse);
            this.update(cx, |sidebar, cx| {
                sidebar
                    .worktree_default_branches
                    .insert(key, DefaultBranchCache::Resolved(parsed));
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}
