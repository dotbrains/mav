use super::*;

impl ProjectPanel {
    pub(super) fn update_diagnostics(&mut self, cx: &mut Context<Self>) {
        let mut diagnostics: HashMap<(WorktreeId, Arc<RelPath>), DiagnosticSeverity> =
            Default::default();
        let show_diagnostics_setting = ProjectPanelSettings::get_global(cx).show_diagnostics;

        if show_diagnostics_setting != ShowDiagnostics::Off {
            self.project
                .read(cx)
                .diagnostic_summaries(false, cx)
                .filter_map(|(path, _, diagnostic_summary)| {
                    if diagnostic_summary.error_count > 0 {
                        Some((path, DiagnosticSeverity::ERROR))
                    } else if show_diagnostics_setting == ShowDiagnostics::All
                        && diagnostic_summary.warning_count > 0
                    {
                        Some((path, DiagnosticSeverity::WARNING))
                    } else {
                        None
                    }
                })
                .for_each(|(project_path, diagnostic_severity)| {
                    let ancestors = project_path.path.ancestors().collect::<Vec<_>>();
                    for path in ancestors.into_iter().rev() {
                        Self::update_strongest_diagnostic_severity(
                            &mut diagnostics,
                            &project_path,
                            path.into(),
                            diagnostic_severity,
                        );
                    }
                });
        }
        self.diagnostics = diagnostics;

        let diagnostic_badges = ProjectPanelSettings::get_global(cx).diagnostic_badges;
        self.diagnostic_counts =
            if diagnostic_badges && show_diagnostics_setting != ShowDiagnostics::Off {
                self.project.read(cx).diagnostic_summaries(false, cx).fold(
                    HashMap::default(),
                    |mut counts, (project_path, _, summary)| {
                        let entry = counts
                            .entry((project_path.worktree_id, project_path.path))
                            .or_default();
                        entry.error_count += summary.error_count;
                        if show_diagnostics_setting == ShowDiagnostics::All {
                            entry.warning_count += summary.warning_count;
                        }
                        counts
                    },
                )
            } else {
                Default::default()
            };
    }

    pub(super) fn update_strongest_diagnostic_severity(
        diagnostics: &mut HashMap<(WorktreeId, Arc<RelPath>), DiagnosticSeverity>,
        project_path: &ProjectPath,
        path_buffer: Arc<RelPath>,
        diagnostic_severity: DiagnosticSeverity,
    ) {
        diagnostics
            .entry((project_path.worktree_id, path_buffer))
            .and_modify(|strongest_diagnostic_severity| {
                *strongest_diagnostic_severity =
                    cmp::min(*strongest_diagnostic_severity, diagnostic_severity);
            })
            .or_insert(diagnostic_severity);
    }

    pub(super) fn focus_in(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus_handle.contains_focused(window, cx) {
            cx.emit(Event::Focus);
        }
    }

    pub(super) fn deploy_context_menu(
        &mut self,
        position: Point<Pixels>,
        entry_id: ProjectEntryId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let project = self.project.read(cx);

        let worktree_id = if let Some(id) = project.worktree_id_for_entry(entry_id, cx) {
            id
        } else {
            return;
        };

        self.selection = Some(SelectedEntry {
            worktree_id,
            entry_id,
        });

        if let Some((worktree, entry)) = self.selected_sub_entry(cx) {
            let auto_fold_dirs = ProjectPanelSettings::get_global(cx).auto_fold_dirs;
            let worktree = worktree.read(cx);
            let is_root = Some(entry) == worktree.root_entry();
            let is_dir = entry.is_dir();
            let is_foldable = auto_fold_dirs && self.is_foldable(entry, worktree);
            let is_unfoldable = auto_fold_dirs && self.is_unfoldable(entry, worktree);
            let is_read_only = project.is_read_only(cx);
            let is_remote = project.is_remote();
            let is_collab = project.is_via_collab();
            let is_local = project.is_local() || project.is_via_wsl_with_host_interop(cx);
            let is_markdown = !is_dir && MarkdownPreviewView::is_markdown_path(&*entry.path);

            let settings = ProjectPanelSettings::get_global(cx);
            let visible_worktrees_count = project.visible_worktrees(cx).count();
            let should_hide_rename = is_root
                && (cfg!(target_os = "windows")
                    || (settings.hide_root && visible_worktrees_count == 1));
            let should_show_compare = !is_dir && self.file_abs_paths_to_diff(cx).is_some();

            let (has_git_repo, has_history) = {
                let project_path = project::ProjectPath {
                    worktree_id,
                    path: entry.path.clone(),
                };
                let git_store = project.git_store().read(cx);
                let has_git_repo = git_store
                    .repository_and_path_for_project_path(&project_path, cx)
                    .is_some();
                let has_history = has_git_repo
                    && !git_store
                        .project_path_git_status(&project_path, cx)
                        .is_some_and(|status| status.is_created());
                (has_git_repo, has_history)
            };

            let has_pasteable_content = self.has_pasteable_content(cx);
            let context_menu = ContextMenu::build(window, cx, |menu, _, cx| {
                menu.context(self.focus_handle.clone()).map(|menu| {
                    if is_read_only {
                        menu.when(is_markdown, |menu| {
                            menu.action("Open Markdown Preview", Box::new(OpenMarkdownPreview))
                        })
                        .when(is_dir, |menu| {
                            menu.action("Search Inside", Box::new(NewSearchInDirectory))
                        })
                    } else {
                        menu.action("New File", Box::new(NewFile))
                            .action("New Folder", Box::new(NewDirectory))
                            .separator()
                            .when(is_local, |menu| {
                                menu.action(
                                    ui::utils::reveal_in_file_manager_label(is_remote),
                                    Box::new(RevealInFileManager),
                                )
                            })
                            .when(is_local, |menu| {
                                menu.action("Open in Default App", Box::new(OpenWithSystem))
                            })
                            .action("Open in Terminal", Box::new(OpenInTerminal))
                            .when(is_markdown, |menu| {
                                menu.action("Open Markdown Preview", Box::new(OpenMarkdownPreview))
                            })
                            .when(is_dir, |menu| {
                                menu.separator()
                                    .action("Find in Folder…", Box::new(NewSearchInDirectory))
                            })
                            .when(is_unfoldable, |menu| {
                                menu.action("Unfold Directory", Box::new(UnfoldDirectory))
                            })
                            .when(is_foldable, |menu| {
                                menu.action("Fold Directory", Box::new(FoldDirectory))
                            })
                            .when(should_show_compare, |menu| {
                                menu.separator()
                                    .action("Compare Marked Files", Box::new(CompareMarkedFiles))
                            })
                            .separator()
                            .action("Cut", Box::new(Cut))
                            .action("Copy", Box::new(Copy))
                            .action("Duplicate", Box::new(Duplicate))
                            .action_disabled_when(!has_pasteable_content, "Paste", Box::new(Paste))
                            .when(cx.has_flag::<ProjectPanelUndoRedoFeatureFlag>(), |menu| {
                                menu.action_disabled_when(
                                    !self.undo_manager.can_undo(),
                                    "Undo",
                                    Box::new(Undo),
                                )
                                .action_disabled_when(
                                    !self.undo_manager.can_redo(),
                                    "Redo",
                                    Box::new(Redo),
                                )
                            })
                            .when(is_remote, |menu| {
                                menu.separator()
                                    .action("Download...", Box::new(DownloadFromRemote))
                            })
                            .separator()
                            .action("Copy Path", Box::new(mav_actions::workspace::CopyPath))
                            .action(
                                "Copy Relative Path",
                                Box::new(mav_actions::workspace::CopyRelativePath),
                            )
                            .when(has_git_repo, |menu| {
                                menu.separator()
                                    .when(!is_dir && self.has_git_changes(entry_id), |menu| {
                                        menu.action(
                                            "Restore File",
                                            Box::new(git::RestoreFile { skip_prompt: false }),
                                        )
                                    })
                                    .action("Add to .gitignore", Box::new(git::AddToGitignore))
                                    .action(
                                        "Add to .git/info/exclude",
                                        Box::new(git::AddToGitInfoExclude),
                                    )
                                    .when(has_history, |menu| {
                                        menu.action("View History", Box::new(git::FileHistory))
                                    })
                            })
                            .when(!should_hide_rename, |menu| {
                                menu.separator().action("Rename", Box::new(Rename))
                            })
                            .when(!is_root && !is_remote, |menu| {
                                menu.action("Trash", Box::new(Trash { skip_prompt: false }))
                            })
                            .when(!is_root, |menu| {
                                menu.action("Delete", Box::new(Delete { skip_prompt: false }))
                            })
                            .when(!is_collab && is_root, |menu| {
                                menu.separator()
                                    .action(
                                        "Add Folders to Project…",
                                        Box::new(workspace::AddFolderToProject),
                                    )
                                    .action("Remove from Project", Box::new(RemoveFromProject))
                            })
                            .when(is_dir && !is_root, |menu| {
                                menu.separator()
                                    .action("Expand All", Box::new(ExpandSelectedEntryAndChildren))
                                    .action(
                                        "Collapse All",
                                        Box::new(CollapseSelectedEntryAndChildren),
                                    )
                            })
                            .when(is_dir && is_root, |menu| {
                                menu.separator()
                                    .action("Expand All", Box::new(ExpandAllEntries))
                                    .action("Collapse All", Box::new(CollapseAllEntries))
                            })
                    }
                })
            });

            window.focus(&context_menu.focus_handle(cx), cx);
            let subscription = cx.subscribe(&context_menu, |this, _, _: &DismissEvent, cx| {
                this.context_menu.take();
                cx.notify();
            });
            self.context_menu = Some((context_menu, position, subscription));
        }

        cx.notify();
    }

    pub(super) fn has_git_changes(&self, entry_id: ProjectEntryId) -> bool {
        for visible in &self.state.visible_entries {
            if let Some(git_entry) = visible.entries.iter().find(|e| e.id == entry_id) {
                let total_modified =
                    git_entry.git_summary.index.modified + git_entry.git_summary.worktree.modified;
                let total_deleted =
                    git_entry.git_summary.index.deleted + git_entry.git_summary.worktree.deleted;
                return total_modified > 0 || total_deleted > 0;
            }
        }
        false
    }
}
