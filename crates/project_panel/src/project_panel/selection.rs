use super::*;

impl ProjectPanel {
    pub(super) fn index_for_selection(
        &self,
        selection: SelectedEntry,
    ) -> Option<(usize, usize, usize)> {
        self.index_for_entry(selection.entry_id, selection.worktree_id)
    }

    pub(super) fn disjoint_effective_entries(&self, cx: &App) -> BTreeSet<SelectedEntry> {
        self.disjoint_entries(self.effective_entries(), cx)
    }

    pub(super) fn disjoint_entries(
        &self,
        entries: BTreeSet<SelectedEntry>,
        cx: &App,
    ) -> BTreeSet<SelectedEntry> {
        let mut sanitized_entries = BTreeSet::new();
        if entries.is_empty() {
            return sanitized_entries;
        }

        let project = self.project.read(cx);
        let entries_by_worktree: HashMap<WorktreeId, Vec<SelectedEntry>> = entries
            .into_iter()
            .filter(|entry| !project.entry_is_worktree_root(entry.entry_id, cx))
            .fold(HashMap::default(), |mut map, entry| {
                map.entry(entry.worktree_id).or_default().push(entry);
                map
            });

        for (worktree_id, worktree_entries) in entries_by_worktree {
            if let Some(worktree) = project.worktree_for_id(worktree_id, cx) {
                let worktree = worktree.read(cx);
                let dir_paths = worktree_entries
                    .iter()
                    .filter_map(|entry| {
                        worktree.entry_for_id(entry.entry_id).and_then(|entry| {
                            if entry.is_dir() {
                                Some(entry.path.as_ref())
                            } else {
                                None
                            }
                        })
                    })
                    .collect::<BTreeSet<_>>();

                sanitized_entries.extend(worktree_entries.into_iter().filter(|entry| {
                    let Some(entry_info) = worktree.entry_for_id(entry.entry_id) else {
                        return false;
                    };
                    let entry_path = entry_info.path.as_ref();
                    let inside_selected_dir = dir_paths.iter().any(|&dir_path| {
                        entry_path != dir_path && entry_path.starts_with(dir_path)
                    });
                    !inside_selected_dir
                }));
            }
        }

        sanitized_entries
    }

    pub(super) fn effective_entries(&self) -> BTreeSet<SelectedEntry> {
        if let Some(selection) = self.selection {
            let selection = SelectedEntry {
                entry_id: self.resolve_entry(selection.entry_id),
                worktree_id: selection.worktree_id,
            };

            // Default to using just the selected item when nothing is marked.
            if self.marked_entries.is_empty() {
                return BTreeSet::from([selection]);
            }

            // Allow operating on the selected item even when something else is marked,
            // making it easier to perform one-off actions without clearing a mark.
            if self.marked_entries.len() == 1 && !self.marked_entries.contains(&selection) {
                return BTreeSet::from([selection]);
            }
        }

        // Return only marked entries since we've already handled special cases where
        // only selection should take precedence. At this point, marked entries may or
        // may not include the current selection, which is intentional.
        self.marked_entries
            .iter()
            .map(|entry| SelectedEntry {
                entry_id: self.resolve_entry(entry.entry_id),
                worktree_id: entry.worktree_id,
            })
            .collect::<BTreeSet<_>>()
    }

    /// Finds the currently selected subentry for a given leaf entry id. If a given entry
    /// has no ancestors, the project entry ID that's passed in is returned as-is.
    pub(super) fn resolve_entry(&self, id: ProjectEntryId) -> ProjectEntryId {
        self.state
            .ancestors
            .get(&id)
            .and_then(|ancestors| ancestors.active_ancestor())
            .unwrap_or(id)
    }

    pub fn selected_entry<'a>(&self, cx: &'a App) -> Option<(&'a Worktree, &'a project::Entry)> {
        let (worktree, entry) = self.selected_entry_handle(cx)?;
        Some((worktree.read(cx), entry))
    }

    pub fn selected_entry_project_path(&self, cx: &App) -> Option<ProjectPath> {
        let (worktree, entry) = self.selected_sub_entry(cx)?;
        Some(ProjectPath {
            worktree_id: worktree.read(cx).id(),
            path: entry.path.clone(),
        })
    }

    /// Compared to selected_entry, this function resolves to the currently
    /// selected subentry if dir auto-folding is enabled.
    pub(super) fn selected_sub_entry<'a>(
        &self,
        cx: &'a App,
    ) -> Option<(Entity<Worktree>, &'a project::Entry)> {
        let (worktree, mut entry) = self.selected_entry_handle(cx)?;

        let resolved_id = self.resolve_entry(entry.id);
        if resolved_id != entry.id {
            let worktree = worktree.read(cx);
            entry = worktree.entry_for_id(resolved_id)?;
        }
        Some((worktree, entry))
    }

    pub(super) fn reveal_in_file_manager_path(&self, cx: &App) -> Option<PathBuf> {
        if let Some((worktree, entry)) = self.selected_sub_entry(cx) {
            return Some(worktree.read(cx).absolutize(&entry.path));
        }

        let root_entry_id = self.state.last_worktree_root_id?;
        let project = self.project.read(cx);
        let worktree = project.worktree_for_entry(root_entry_id, cx)?;
        let worktree = worktree.read(cx);
        let root_entry = worktree.entry_for_id(root_entry_id)?;
        Some(worktree.absolutize(&root_entry.path))
    }

    pub fn selected_entry_handle<'a>(
        &self,
        cx: &'a App,
    ) -> Option<(Entity<Worktree>, &'a project::Entry)> {
        let selection = self.selection?;
        let project = self.project.read(cx);
        let worktree = project.worktree_for_id(selection.worktree_id, cx)?;
        let entry = worktree.read(cx).entry_for_id(selection.entry_id)?;
        Some((worktree, entry))
    }

    pub(super) fn expand_to_selection(&mut self, cx: &mut Context<Self>) -> Option<()> {
        let (worktree, entry) = self.selected_entry(cx)?;
        let expanded_dir_ids = self
            .state
            .expanded_dir_ids
            .entry(worktree.id())
            .or_default();

        for path in entry.path.ancestors() {
            let Some(entry) = worktree.entry_for_path(path) else {
                continue;
            };
            if entry.is_dir()
                && let Err(idx) = expanded_dir_ids.binary_search(&entry.id)
            {
                expanded_dir_ids.insert(idx, entry.id);
            }
        }

        Some(())
    }

    pub(super) fn create_new_git_entry(
        parent_entry: &Entry,
        git_summary: GitSummary,
        new_entry_kind: EntryKind,
    ) -> GitEntry {
        GitEntry {
            entry: Entry {
                id: NEW_ENTRY_ID,
                kind: new_entry_kind,
                path: parent_entry.path.join(RelPath::unix("\0").unwrap()),
                inode: 0,
                mtime: parent_entry.mtime,
                size: parent_entry.size,
                is_ignored: parent_entry.is_ignored,
                is_hidden: parent_entry.is_hidden,
                is_external: false,
                is_private: false,
                is_always_included: parent_entry.is_always_included,
                canonical_path: parent_entry.canonical_path.clone(),
                char_bag: parent_entry.char_bag,
                is_fifo: parent_entry.is_fifo,
            },
            git_summary,
        }
    }
}
