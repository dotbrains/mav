use super::*;

impl Project {
    pub fn set_active_path(&mut self, entry: Option<ProjectPath>, cx: &mut Context<Self>) {
        let new_active_entry = entry.and_then(|project_path| {
            let worktree = self.worktree_for_id(project_path.worktree_id, cx)?;
            let entry = worktree.read(cx).entry_for_path(&project_path.path)?;
            Some(entry.id)
        });
        if new_active_entry != self.active_entry {
            self.active_entry = new_active_entry;
            self.lsp_store.update(cx, |lsp_store, _| {
                lsp_store.set_active_entry(new_active_entry);
            });
            cx.emit(Event::ActiveEntryChanged(new_active_entry));
        }
    }

    pub fn language_servers_running_disk_based_diagnostics<'a>(
        &'a self,
        cx: &'a App,
    ) -> impl Iterator<Item = LanguageServerId> + 'a {
        self.lsp_store
            .read(cx)
            .language_servers_running_disk_based_diagnostics()
    }

    pub fn diagnostic_summary(&self, include_ignored: bool, cx: &App) -> DiagnosticSummary {
        self.lsp_store
            .read(cx)
            .diagnostic_summary(include_ignored, cx)
    }

    /// Returns a summary of the diagnostics for the provided project path only.
    pub fn diagnostic_summary_for_path(&self, path: &ProjectPath, cx: &App) -> DiagnosticSummary {
        self.lsp_store
            .read(cx)
            .diagnostic_summary_for_path(path, cx)
    }

    pub fn diagnostic_summaries<'a>(
        &'a self,
        include_ignored: bool,
        cx: &'a App,
    ) -> impl Iterator<Item = (ProjectPath, LanguageServerId, DiagnosticSummary)> + 'a {
        self.lsp_store
            .read(cx)
            .diagnostic_summaries(include_ignored, cx)
    }

    pub fn active_entry(&self) -> Option<ProjectEntryId> {
        self.active_entry
    }

    pub fn entry_for_path<'a>(&'a self, path: &ProjectPath, cx: &'a App) -> Option<&'a Entry> {
        self.worktree_store.read(cx).entry_for_path(path, cx)
    }

    pub fn path_for_entry(&self, entry_id: ProjectEntryId, cx: &App) -> Option<ProjectPath> {
        let worktree = self.worktree_for_entry(entry_id, cx)?;
        let worktree = worktree.read(cx);
        let worktree_id = worktree.id();
        let path = worktree.entry_for_id(entry_id)?.path.clone();
        Some(ProjectPath { worktree_id, path })
    }

    pub fn absolute_path(&self, project_path: &ProjectPath, cx: &App) -> Option<PathBuf> {
        Some(
            self.worktree_for_id(project_path.worktree_id, cx)?
                .read(cx)
                .absolutize(&project_path.path),
        )
    }

    /// Attempts to find a `ProjectPath` corresponding to the given path. If the path
    /// is a *full path*, meaning it starts with the root name of a worktree, we'll locate
    /// it in that worktree. Otherwise, we'll attempt to find it as a relative path in
    /// the first visible worktree that has an entry for that relative path.
    ///
    /// We use this to resolve edit steps, when there's a chance an LLM may omit the workree
    /// root name from paths.
    ///
    /// # Arguments
    ///
    /// * `path` - An absolute path, or a full path that starts with a worktree root name, or a
    ///   relative path within a visible worktree.
    /// * `cx` - A reference to the `AppContext`.
    ///
    /// # Returns
    ///
    /// Returns `Some(ProjectPath)` if a matching worktree is found, otherwise `None`.
    pub fn find_project_path(&self, path: impl AsRef<Path>, cx: &App) -> Option<ProjectPath> {
        let path_style = self.path_style(cx);
        let path = path.as_ref();
        let worktree_store = self.worktree_store.read(cx);

        if is_absolute(&path.to_string_lossy(), path_style) {
            for worktree in worktree_store.visible_worktrees(cx) {
                let worktree_abs_path = worktree.read(cx).abs_path();

                if let Ok(relative_path) = path.strip_prefix(worktree_abs_path)
                    && let Ok(path) = RelPath::new(relative_path, path_style)
                {
                    return Some(ProjectPath {
                        worktree_id: worktree.read(cx).id(),
                        path: path.into_arc(),
                    });
                }
            }
        } else {
            // First pass: for each worktree, try two interpretations of the path and
            // return whichever finds an existing entry first:
            //   (a) Strip the worktree root name as a prefix.
            //   (b) Treat the path as a literal worktree-relative path.
            for worktree in worktree_store.visible_worktrees(cx) {
                let worktree = worktree.read(cx);
                if let Ok(relative_path) = path.strip_prefix(worktree.root_name().as_std_path())
                    && let Ok(rel_path) = RelPath::new(relative_path, path_style)
                    && let Some(entry) = worktree.entry_for_path(&rel_path)
                {
                    return Some(ProjectPath {
                        worktree_id: worktree.id(),
                        path: entry.path.clone(),
                    });
                }
                if let Ok(rel_path) = RelPath::new(path, path_style)
                    && let Some(entry) = worktree.entry_for_path(&rel_path)
                {
                    return Some(ProjectPath {
                        worktree_id: worktree.id(),
                        path: entry.path.clone(),
                    });
                }
            }

            // Second pass: strip the worktree root name prefix without requiring the
            // entry to exist, to allow resolving paths that don't exist yet.
            for worktree in worktree_store.visible_worktrees(cx) {
                let worktree_root_name = worktree.read(cx).root_name();
                if let Ok(relative_path) = path.strip_prefix(worktree_root_name.as_std_path())
                    && let Ok(path) = RelPath::new(relative_path, path_style)
                {
                    return Some(ProjectPath {
                        worktree_id: worktree.read(cx).id(),
                        path: path.into_arc(),
                    });
                }
            }
        }

        None
    }

    /// If there's only one visible worktree, returns the given worktree-relative path with no prefix.
    ///
    /// Otherwise, returns the full path for the project path (obtained by prefixing the worktree-relative path with the name of the worktree).
    pub fn short_full_path_for_project_path(
        &self,
        project_path: &ProjectPath,
        cx: &App,
    ) -> Option<String> {
        let path_style = self.path_style(cx);
        if self.visible_worktrees(cx).take(2).count() < 2 {
            return Some(project_path.path.display(path_style).to_string());
        }
        self.worktree_for_id(project_path.worktree_id, cx)
            .map(|worktree| {
                let worktree_name = worktree.read(cx).root_name();
                worktree_name
                    .join(&project_path.path)
                    .display(path_style)
                    .to_string()
            })
    }

    pub fn project_path_for_absolute_path(&self, abs_path: &Path, cx: &App) -> Option<ProjectPath> {
        self.worktree_store
            .read(cx)
            .project_path_for_absolute_path(abs_path, cx)
    }

    pub fn get_workspace_root(&self, project_path: &ProjectPath, cx: &App) -> Option<PathBuf> {
        Some(
            self.worktree_for_id(project_path.worktree_id, cx)?
                .read(cx)
                .abs_path()
                .to_path_buf(),
        )
    }

    pub fn blame_buffer(
        &self,
        buffer: &Entity<Buffer>,
        version: Option<clock::Global>,
        cx: &mut App,
    ) -> Task<Result<Option<Blame>>> {
        self.git_store.update(cx, |git_store, cx| {
            git_store.blame_buffer(buffer, version, cx)
        })
    }

    pub fn get_permalink_to_line(
        &self,
        buffer: &Entity<Buffer>,
        selection: Range<u32>,
        cx: &mut App,
    ) -> Task<Result<url::Url>> {
        self.git_store.update(cx, |git_store, cx| {
            git_store.get_permalink_to_line(buffer, selection, cx)
        })
    }
}
