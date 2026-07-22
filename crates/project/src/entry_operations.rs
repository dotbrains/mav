use super::*;

impl Project {
    #[inline]
    pub fn worktree_for_root_name(&self, root_name: &str, cx: &App) -> Option<Entity<Worktree>> {
        self.visible_worktrees(cx)
            .find(|tree| tree.read(cx).root_name() == root_name)
    }

    fn emit_group_key_changed_if_needed(&mut self, cx: &mut Context<Self>) {
        let new_worktree_paths = self.worktree_paths(cx);
        if new_worktree_paths != self.last_worktree_paths {
            let old_worktree_paths =
                std::mem::replace(&mut self.last_worktree_paths, new_worktree_paths);
            cx.emit(Event::WorktreePathsChanged { old_worktree_paths });
        }
    }

    #[inline]
    pub fn worktree_root_names<'a>(&'a self, cx: &'a App) -> impl Iterator<Item = &'a str> {
        self.visible_worktrees(cx)
            .map(|tree| tree.read(cx).root_name().as_unix_str())
    }

    #[inline]
    pub fn worktree_for_id(&self, id: WorktreeId, cx: &App) -> Option<Entity<Worktree>> {
        self.worktree_store.read(cx).worktree_for_id(id, cx)
    }

    pub fn worktree_for_entry(
        &self,
        entry_id: ProjectEntryId,
        cx: &App,
    ) -> Option<Entity<Worktree>> {
        self.worktree_store
            .read(cx)
            .worktree_for_entry(entry_id, cx)
    }

    #[inline]
    pub fn worktree_id_for_entry(&self, entry_id: ProjectEntryId, cx: &App) -> Option<WorktreeId> {
        self.worktree_for_entry(entry_id, cx)
            .map(|worktree| worktree.read(cx).id())
    }

    /// Checks if the entry is the root of a worktree.
    #[inline]
    pub fn entry_is_worktree_root(&self, entry_id: ProjectEntryId, cx: &App) -> bool {
        self.worktree_for_entry(entry_id, cx)
            .map(|worktree| {
                worktree
                    .read(cx)
                    .root_entry()
                    .is_some_and(|e| e.id == entry_id)
            })
            .unwrap_or(false)
    }

    #[inline]
    pub fn project_path_git_status(
        &self,
        project_path: &ProjectPath,
        cx: &App,
    ) -> Option<FileStatus> {
        self.git_store
            .read(cx)
            .project_path_git_status(project_path, cx)
    }

    #[inline]
    pub fn visibility_for_paths(
        &self,
        paths: &[PathBuf],
        exclude_sub_dirs: bool,
        cx: &App,
    ) -> Option<bool> {
        paths
            .iter()
            .map(|path| self.visibility_for_path(path, exclude_sub_dirs, cx))
            .max()
            .flatten()
    }

    pub fn visibility_for_path(
        &self,
        path: &Path,
        exclude_sub_dirs: bool,
        cx: &App,
    ) -> Option<bool> {
        let path = SanitizedPath::new(path).as_path();
        let path_style = self.path_style(cx);
        self.worktrees(cx)
            .filter_map(|worktree| {
                let worktree = worktree.read(cx);
                let abs_path = worktree.abs_path();
                let relative_path = path_style.strip_prefix(path, abs_path.as_ref());
                let is_dir = relative_path
                    .as_ref()
                    .and_then(|p| worktree.entry_for_path(p))
                    .is_some_and(|e| e.is_dir());
                // Don't exclude the worktree root itself, only actual subdirectories
                let is_subdir = relative_path
                    .as_ref()
                    .is_some_and(|p| !p.as_ref().as_unix_str().is_empty());
                let contains =
                    relative_path.is_some() && (!exclude_sub_dirs || !is_dir || !is_subdir);
                contains.then(|| worktree.is_visible())
            })
            .max()
    }

    pub fn create_entry(
        &mut self,
        project_path: impl Into<ProjectPath>,
        is_directory: bool,
        cx: &mut Context<Self>,
    ) -> Task<Result<CreatedEntry>> {
        let project_path = project_path.into();
        let Some(worktree) = self.worktree_for_id(project_path.worktree_id, cx) else {
            return Task::ready(Err(anyhow!(format!(
                "No worktree for path {project_path:?}"
            ))));
        };
        worktree.update(cx, |worktree, cx| {
            worktree.create_entry(project_path.path, is_directory, None, cx)
        })
    }

    #[inline]
    pub fn copy_entry(
        &mut self,
        entry_id: ProjectEntryId,
        new_project_path: ProjectPath,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Entry>>> {
        self.worktree_store.update(cx, |worktree_store, cx| {
            worktree_store.copy_entry(entry_id, new_project_path, cx)
        })
    }

    /// Renames the project entry with given `entry_id`.
    ///
    /// `new_path` is a relative path to worktree root.
    /// If root entry is renamed then its new root name is used instead.
    pub fn rename_entry(
        &mut self,
        entry_id: ProjectEntryId,
        new_path: ProjectPath,
        cx: &mut Context<Self>,
    ) -> Task<Result<CreatedEntry>> {
        let worktree_store = self.worktree_store.clone();
        let Some((worktree, old_path, is_dir)) = worktree_store
            .read(cx)
            .worktree_and_entry_for_id(entry_id, cx)
            .map(|(worktree, entry)| (worktree, entry.path.clone(), entry.is_dir()))
        else {
            return Task::ready(Err(anyhow!(format!("No worktree for entry {entry_id:?}"))));
        };

        let worktree_id = worktree.read(cx).id();
        let is_root_entry = self.entry_is_worktree_root(entry_id, cx);

        let lsp_store = self.lsp_store().downgrade();
        cx.spawn(async move |project, cx| {
            let (old_abs_path, new_abs_path) = {
                let root_path = worktree.read_with(cx, |this, _| this.abs_path());
                let new_abs_path = if is_root_entry {
                    root_path
                        .parent()
                        .unwrap()
                        .join(new_path.path.as_std_path())
                } else {
                    root_path.join(&new_path.path.as_std_path())
                };
                (root_path.join(old_path.as_std_path()), new_abs_path)
            };
            let transaction = LspStore::will_rename_entry(
                lsp_store.clone(),
                worktree_id,
                &old_abs_path,
                &new_abs_path,
                is_dir,
                cx.clone(),
            )
            .await;

            let entry = worktree_store
                .update(cx, |worktree_store, cx| {
                    worktree_store.rename_entry(entry_id, new_path.clone(), cx)
                })
                .await?;

            project
                .update(cx, |_, cx| {
                    cx.emit(Event::EntryRenamed(
                        transaction,
                        new_path.clone(),
                        new_abs_path.clone(),
                    ));
                })
                .ok();

            lsp_store
                .read_with(cx, |this, _| {
                    this.did_rename_entry(worktree_id, &old_abs_path, &new_abs_path, is_dir);
                })
                .ok();
            Ok(entry)
        })
    }

    #[inline]
    pub fn delete_file(
        &mut self,
        path: ProjectPath,
        trash: bool,
        cx: &mut Context<Self>,
    ) -> Option<Task<Result<Option<TrashedEntry>>>> {
        let entry = self.entry_for_path(&path, cx)?;
        self.delete_entry(entry.id, trash, cx)
    }

    #[inline]
    pub fn delete_entry(
        &mut self,
        entry_id: ProjectEntryId,
        trash: bool,
        cx: &mut Context<Self>,
    ) -> Option<Task<Result<Option<TrashedEntry>>>> {
        let worktree = self.worktree_for_entry(entry_id, cx)?;
        cx.emit(Event::DeletedEntry(worktree.read(cx).id(), entry_id));
        worktree.update(cx, |worktree, cx| {
            worktree.delete_entry(entry_id, trash, cx)
        })
    }

    #[inline]
    pub fn restore_entry(
        &self,
        worktree_id: WorktreeId,
        trash_entry: TrashedEntry,
        cx: &mut Context<'_, Self>,
    ) -> Task<Result<ProjectPath>> {
        let Some(worktree) = self.worktree_for_id(worktree_id, cx) else {
            return Task::ready(Err(anyhow!("No worktree for id {worktree_id:?}")));
        };

        cx.spawn(async move |_, cx| {
            Worktree::restore_entry(trash_entry, worktree, cx)
                .await
                .map(|rel_path_buf| ProjectPath {
                    worktree_id: worktree_id,
                    path: Arc::from(rel_path_buf.as_rel_path()),
                })
        })
    }

    #[inline]
    pub fn expand_entry(
        &mut self,
        worktree_id: WorktreeId,
        entry_id: ProjectEntryId,
        cx: &mut Context<Self>,
    ) -> Option<Task<Result<()>>> {
        let worktree = self.worktree_for_id(worktree_id, cx)?;
        worktree.update(cx, |worktree, cx| worktree.expand_entry(entry_id, cx))
    }

    pub fn expand_all_for_entry(
        &mut self,
        worktree_id: WorktreeId,
        entry_id: ProjectEntryId,
        cx: &mut Context<Self>,
    ) -> Option<Task<Result<()>>> {
        let worktree = self.worktree_for_id(worktree_id, cx)?;
        let task = worktree.update(cx, |worktree, cx| {
            worktree.expand_all_for_entry(entry_id, cx)
        });
        Some(cx.spawn(async move |this, cx| {
            task.context("no task")?.await?;
            this.update(cx, |_, cx| {
                cx.emit(Event::ExpandedAllForEntry(worktree_id, entry_id));
            })?;
            Ok(())
        }))
    }
}
