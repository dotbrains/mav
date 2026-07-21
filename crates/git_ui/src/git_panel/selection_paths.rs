use super::*;

impl GitPanel {
    pub fn entry_by_path(&self, path: &RepoPath) -> Option<usize> {
        self.entries_indices.get(path).copied()
    }

    pub fn select_entry_by_path(
        &mut self,
        path: ProjectPath,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(git_repo) = self.active_repository.as_ref() else {
            return;
        };

        let (repo_path, section) = {
            let repo = git_repo.read(cx);
            let Some(repo_path) = repo.project_path_to_repo_path(&path, cx) else {
                return;
            };

            let section = repo
                .status_for_path(&repo_path)
                .map(|status| status.status)
                .map(|status| {
                    if repo.had_conflict_on_last_merge_head_change(&repo_path) {
                        Section::Conflict
                    } else if status.is_created() {
                        Section::New
                    } else {
                        Section::Tracked
                    }
                });

            (repo_path, section)
        };

        let mut needs_rebuild = false;
        if let (Some(section), Some(tree_state)) = (section, self.view_mode.tree_state_mut()) {
            let mut current_dir = repo_path.parent();
            while let Some(dir) = current_dir {
                let key = TreeKey {
                    section,
                    path: RepoPath::from_rel_path(dir),
                };

                if tree_state.expanded_dirs.get(&key.path) == Some(&false) {
                    tree_state.expanded_dirs.insert(key.path.clone(), true);
                    needs_rebuild = true;
                }

                current_dir = dir.parent();
            }
        }

        if needs_rebuild {
            self.update_visible_entries(window, cx);
        }

        let Some(ix) = self.entry_by_path(&repo_path) else {
            return;
        };

        self.selected_entry = Some(ix);
        self.scroll_to_selected_entry(cx);
    }
}
