use super::*;

impl GitGraph {
    pub(super) fn git_task_context(
        &self,
        commit_sha: Oid,
        ref_name: Option<&str>,
        cx: &App,
    ) -> Option<TaskContext> {
        let repository_path = self
            .get_repository(cx)?
            .read(cx)
            .work_directory_abs_path
            .to_path_buf();

        let repository_name = repository_path
            .file_name()
            .and_then(|name| name.to_str())
            .map(ToString::to_string);

        let mut task_variables = TaskVariables::from_iter([
            (VariableName::GitSha, commit_sha.to_string()),
            (VariableName::GitShaShort, commit_sha.display_short()),
            (
                VariableName::GitRepositoryPath,
                repository_path.to_string_lossy().into_owned(),
            ),
        ]);

        if let Some(repository_name) = repository_name {
            task_variables.insert(VariableName::GitRepositoryName, repository_name);
        }

        if let Some(ref_name) = ref_name {
            task_variables.insert(VariableName::GitRef, ref_name.to_string());
        }

        Some(TaskContext {
            cwd: Some(repository_path),
            task_variables,
            ..TaskContext::default()
        })
    }

    pub(super) fn git_context_menu_tasks(
        &self,
        task_context: &TaskContext,
        cx: &App,
    ) -> Vec<(TaskSourceKind, ResolvedTask)> {
        let Some(workspace) = self.workspace.upgrade() else {
            return Vec::new();
        };

        let project = workspace.read(cx).project().clone();

        let task_inventory = project.read_with(cx, |project, cx| {
            project.task_store().read(cx).task_inventory().cloned()
        });

        let Some(task_inventory) = task_inventory else {
            return Vec::new();
        };

        task_inventory
            .read(cx)
            .resolve_global_tasks_with_tag(GIT_COMMAND_TASK_TAG, task_context)
    }

    pub(super) fn schedule_git_task(
        &mut self,
        task_source_kind: TaskSourceKind,
        resolved_task: ResolvedTask,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.workspace
            .update(cx, |workspace, cx| {
                workspace.schedule_resolved_task(
                    task_source_kind,
                    resolved_task,
                    false,
                    window,
                    cx,
                );
            })
            .ok();
    }

    pub(super) fn deploy_entry_context_menu(
        &mut self,
        position: Point<Pixels>,
        index: usize,
        ref_name: Option<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(commit) = self.graph_data.commits.get(index) else {
            return;
        };
        let sha = commit.data.sha;
        let sha_short = sha.display_short();
        let git_tasks = self
            .git_task_context(sha, ref_name.as_deref(), cx)
            .map(|task_context| self.git_context_menu_tasks(&task_context, cx))
            .unwrap_or_default();

        let header = match &ref_name {
            Some(ref_name) => format!("Ref {ref_name}"),
            None => format!("Commit {sha_short}"),
        };

        let focus_handle = self.focus_handle.clone();
        let git_graph = cx.entity();
        let context_menu = ContextMenu::build(window, cx, |context_menu, window, _| {
            context_menu
                .context(focus_handle)
                .header(header)
                .entry(
                    "View Commit",
                    Some(OpenCommitView.boxed_clone()),
                    window.handler_for(&git_graph, move |this, window, cx| {
                        this.open_commit_view(index, window, cx);
                    }),
                )
                .entry(
                    "Copy SHA",
                    Some(CopyCommitSha.boxed_clone()),
                    window.handler_for(&git_graph, move |this, _window, cx| {
                        this.copy_commit_sha(index, cx);
                    }),
                )
                .when_some(ref_name.clone(), |menu, ref_name| {
                    menu.entry("Copy Ref Name", None, move |_window, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(ref_name.to_string()));
                    })
                })
                .when(ref_name.is_none(), |menu| {
                    menu.map(|menu| {
                        let tag_names = commit
                            .data
                            .tag_names()
                            .into_iter()
                            .map(|tag_name| SharedString::from(tag_name.to_string()))
                            .collect::<Vec<_>>();
                        let copy_tag_label = "Copy Tag";

                        match tag_names.as_slice() {
                            [] => menu.item(
                                ContextMenuEntry::new(copy_tag_label)
                                    .action(CopyCommitTag.boxed_clone())
                                    .disabled(true),
                            ),
                            [tag_name] => {
                                let tag_name = tag_name.clone();
                                let label = format!("{copy_tag_label}: {tag_name}");
                                menu.entry(
                                    label,
                                    Some(CopyCommitTag.boxed_clone()),
                                    move |_window, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            tag_name.to_string(),
                                        ));
                                    },
                                )
                            }
                            _ => menu.submenu(copy_tag_label, move |menu, _window, _cx| {
                                let mut menu =
                                    menu.fixed_width(COMMIT_TAG_LIST_WIDTH_IN_REMS.into());

                                for tag_name in tag_names.clone() {
                                    let tag_name_to_copy = tag_name.clone();

                                    menu = menu.entry(tag_name, None, move |_window, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            tag_name_to_copy.to_string(),
                                        ));
                                    });
                                }
                                menu
                            }),
                        }
                    })
                })
                .map(|mut menu| {
                    menu = menu.separator().header("Custom Commands");

                    if git_tasks.is_empty() {
                        return menu.item(
                            ContextMenuEntry::new("Learn More")
                                .icon(IconName::ArrowUpRight)
                                .icon_color(Color::Muted)
                                .icon_position(IconPosition::End)
                                .handler(|_window, cx| {
                                    let docs_url = release_channel::docs_url(
                                        CUSTOM_GIT_COMMANDS_DOCS_SLUG,
                                        cx,
                                    );
                                    cx.open_url(&docs_url);
                                }),
                        );
                    }

                    for (task_source_kind, resolved_task) in git_tasks {
                        let label = resolved_task.display_label().to_string();

                        menu = menu.entry(
                            label,
                            None,
                            window.handler_for(&git_graph, move |this, window, cx| {
                                this.schedule_git_task(
                                    task_source_kind.clone(),
                                    resolved_task.clone(),
                                    window,
                                    cx,
                                );
                            }),
                        );
                    }

                    menu
                })
        });
        self.set_context_menu(context_menu, position, index, window, cx);
    }

    pub(super) fn set_context_menu(
        &mut self,
        context_menu: Entity<ContextMenu>,
        position: Point<Pixels>,
        entry_idx: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&context_menu.focus_handle(cx), cx);

        let subscription = cx.subscribe_in(
            &context_menu,
            window,
            |this, _, _: &DismissEvent, window, cx| {
                if this.context_menu.as_ref().is_some_and(|context_menu| {
                    context_menu
                        .menu
                        .focus_handle(cx)
                        .contains_focused(window, cx)
                }) {
                    cx.focus_self(window);
                }
                this.context_menu.take();
                cx.notify();
            },
        );
        self.context_menu = Some(GitGraphContextMenu {
            menu: context_menu,
            position,
            entry_idx,
            _subscription: subscription,
        });
        cx.notify();
    }
}
