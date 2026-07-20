use super::*;

impl Workspace {
    pub fn open_workspace_for_paths(
        &mut self,
        // replace_current_window: bool,
        mut open_mode: OpenMode,
        paths: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<Entity<Workspace>>> {
        let requesting_window = window.window_handle().downcast::<MultiWorkspace>();
        let is_remote = self.project.read(cx).is_via_collab();
        let has_worktree = self.project.read(cx).worktrees(cx).next().is_some();
        let has_dirty_items = self.items(cx).any(|item| item.is_dirty(cx));

        let workspace_is_empty = !is_remote && !has_worktree && !has_dirty_items;
        if workspace_is_empty {
            open_mode = OpenMode::Activate;
        }

        let app_state = self.app_state.clone();

        cx.spawn(async move |_, cx| {
            let OpenResult { workspace, .. } = cx
                .update(|cx| {
                    open_paths(
                        &paths,
                        app_state,
                        OpenOptions {
                            requesting_window,
                            open_mode,
                            workspace_matching: if open_mode == OpenMode::NewWindow {
                                WorkspaceMatching::None
                            } else {
                                WorkspaceMatching::default()
                            },
                            ..Default::default()
                        },
                        cx,
                    )
                })
                .await?;
            Ok(workspace)
        })
    }

    #[allow(clippy::type_complexity)]
    pub fn open_paths(
        &mut self,
        mut abs_paths: Vec<PathBuf>,
        options: OpenOptions,
        pane: Option<WeakEntity<Pane>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Vec<Option<anyhow::Result<Box<dyn ItemHandle>>>>> {
        let fs = self.app_state.fs.clone();

        let caller_ordered_abs_paths = abs_paths.clone();

        // Sort the paths to ensure we add worktrees for parents before their children.
        abs_paths.sort_unstable();
        cx.spawn_in(window, async move |this, cx| {
            let mut tasks = Vec::with_capacity(abs_paths.len());

            for abs_path in &abs_paths {
                let visible = match options.visible.as_ref().unwrap_or(&OpenVisible::None) {
                    OpenVisible::All => Some(true),
                    OpenVisible::None => Some(false),
                    OpenVisible::OnlyFiles => match fs.metadata(abs_path).await.log_err() {
                        Some(Some(metadata)) => Some(!metadata.is_dir),
                        Some(None) => Some(true),
                        None => None,
                    },
                    OpenVisible::OnlyDirectories => match fs.metadata(abs_path).await.log_err() {
                        Some(Some(metadata)) => Some(metadata.is_dir),
                        Some(None) => Some(false),
                        None => None,
                    },
                };
                let project_path = match visible {
                    Some(visible) => match this
                        .update(cx, |this, cx| {
                            Workspace::project_path_for_path(
                                this.project.clone(),
                                abs_path,
                                visible,
                                cx,
                            )
                        })
                        .log_err()
                    {
                        Some(project_path) => project_path.await.log_err(),
                        None => None,
                    },
                    None => None,
                };

                let this = this.clone();
                let abs_path: Arc<Path> = SanitizedPath::new(&abs_path).as_path().into();
                let fs = fs.clone();
                let pane = pane.clone();
                let task = cx.spawn(async move |cx| {
                    let (worktree, project_path) = project_path?;
                    let (entry_is_directory, worktree_is_local) =
                        worktree.read_with(cx, |worktree, _| {
                            let entry = if project_path.path.as_unix_str().is_empty() {
                                worktree.root_entry()
                            } else {
                                worktree.entry_for_path(&project_path.path)
                            };
                            (entry.map(|entry| entry.is_dir()), worktree.is_local())
                        });
                    let is_directory = match entry_is_directory {
                        Some(is_directory) => is_directory,
                        None if worktree_is_local => fs.is_dir(&abs_path).await,
                        None => false,
                    };

                    if is_directory {
                        // Opening a directory should not race to update the active entry.
                        // We'll select/reveal a deterministic final entry after all paths finish opening.
                        None
                    } else {
                        Some(
                            this.update_in(cx, |this, window, cx| {
                                this.open_path_in_tabbed_pane(
                                    project_path,
                                    pane,
                                    options.focus.unwrap_or(true),
                                    window,
                                    cx,
                                )
                            })
                            .ok()?
                            .await,
                        )
                    }
                });
                tasks.push(task);
            }

            let results = futures::future::join_all(tasks).await;

            // Determine the winner using the fake/abstract FS metadata, not `Path::is_dir`.
            let mut winner: Option<(PathBuf, bool)> = None;
            for abs_path in caller_ordered_abs_paths.into_iter().rev() {
                if let Some(Some(metadata)) = fs.metadata(&abs_path).await.log_err() {
                    if !metadata.is_dir {
                        winner = Some((abs_path, false));
                        break;
                    }
                    if winner.is_none() {
                        winner = Some((abs_path, true));
                    }
                } else if winner.is_none() {
                    winner = Some((abs_path, false));
                }
            }

            // Compute the winner entry id on the foreground thread and emit once, after all
            // paths finish opening. This avoids races between concurrently-opening paths
            // (directories in particular) and makes the resulting project panel selection
            // deterministic.
            if let Some((winner_abs_path, winner_is_dir)) = winner {
                'emit_winner: {
                    let winner_abs_path: Arc<Path> =
                        SanitizedPath::new(&winner_abs_path).as_path().into();

                    let visible = match options.visible.as_ref().unwrap_or(&OpenVisible::None) {
                        OpenVisible::All => true,
                        OpenVisible::None => false,
                        OpenVisible::OnlyFiles => !winner_is_dir,
                        OpenVisible::OnlyDirectories => winner_is_dir,
                    };

                    let Some(worktree_task) = this
                        .update(cx, |workspace, cx| {
                            workspace.project.update(cx, |project, cx| {
                                project.find_or_create_worktree(
                                    winner_abs_path.as_ref(),
                                    visible,
                                    cx,
                                )
                            })
                        })
                        .ok()
                    else {
                        break 'emit_winner;
                    };

                    let Ok((worktree, _)) = worktree_task.await else {
                        break 'emit_winner;
                    };

                    let Ok(Some(entry_id)) = this.update(cx, |_, cx| {
                        let worktree = worktree.read(cx);
                        let worktree_abs_path = worktree.abs_path();
                        let entry = if winner_abs_path.as_ref() == worktree_abs_path.as_ref() {
                            worktree.root_entry()
                        } else {
                            winner_abs_path
                                .strip_prefix(worktree_abs_path.as_ref())
                                .ok()
                                .and_then(|relative_path| {
                                    let relative_path =
                                        RelPath::new(relative_path, PathStyle::local())
                                            .log_err()?;
                                    worktree.entry_for_path(&relative_path)
                                })
                        }?;
                        Some(entry.id)
                    }) else {
                        break 'emit_winner;
                    };

                    this.update(cx, |workspace, cx| {
                        workspace.project.update(cx, |_, cx| {
                            cx.emit(project::Event::ActiveEntryChanged(Some(entry_id)));
                        });
                    })
                    .ok();
                }
            }

            results
        })
    }

    pub fn open_resolved_path(
        &mut self,
        path: ResolvedPath,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<Box<dyn ItemHandle>>> {
        match path {
            ResolvedPath::ProjectPath { project_path, .. } => {
                self.open_path_in_tabbed_pane(project_path, None, true, window, cx)
            }
            ResolvedPath::AbsPath { path, .. } => self.open_abs_path(
                PathBuf::from(path),
                OpenOptions {
                    visible: Some(OpenVisible::None),
                    ..Default::default()
                },
                window,
                cx,
            ),
        }
    }

    pub fn absolute_path_of_worktree(
        &self,
        worktree_id: WorktreeId,
        cx: &mut Context<Self>,
    ) -> Option<PathBuf> {
        self.project
            .read(cx)
            .worktree_for_id(worktree_id, cx)
            // TODO: use `abs_path` or `root_dir`
            .map(|wt| wt.read(cx).abs_path().as_ref().to_path_buf())
    }

    pub fn add_folder_to_project(
        &mut self,
        _: &AddFolderToProject,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let project = self.project.read(cx);
        if project.is_via_collab() {
            self.show_error("You cannot add folders to someone else's project", cx);
            return;
        }
        let paths = self.prompt_for_open_path(
            PathPromptOptions {
                files: false,
                directories: true,
                multiple: true,
                prompt: None,
            },
            DirectoryLister::Project(self.project.clone()),
            window,
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if let Some(paths) = paths.await.log_err().flatten() {
                let results = this
                    .update_in(cx, |this, window, cx| {
                        this.open_paths(
                            paths,
                            OpenOptions {
                                visible: Some(OpenVisible::All),
                                ..Default::default()
                            },
                            None,
                            window,
                            cx,
                        )
                    })?
                    .await;
                for result in results.into_iter().flatten() {
                    result.log_err();
                }
            }
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    pub fn project_path_for_path(
        project: Entity<Project>,
        abs_path: &Path,
        visible: bool,
        cx: &mut App,
    ) -> Task<Result<(Entity<Worktree>, ProjectPath)>> {
        let entry = project.update(cx, |project, cx| {
            project.find_or_create_worktree(abs_path, visible, cx)
        });
        cx.spawn(async move |cx| {
            let (worktree, path) = entry.await?;
            let worktree_id = worktree.read_with(cx, |t, _| t.id());
            Ok((worktree, ProjectPath { worktree_id, path }))
        })
    }
}
