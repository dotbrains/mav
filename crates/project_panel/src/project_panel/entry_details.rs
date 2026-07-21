use super::*;

impl ProjectPanel {
    pub(super) fn details_for_entry(
        &self,
        entry: &Entry,
        worktree_id: WorktreeId,
        root_name: &RelPath,
        entries_paths: &HashSet<Arc<RelPath>>,
        git_status: GitSummary,
        sticky: Option<StickyDetails>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> EntryDetails {
        let (show_file_icons, show_folder_icons) = {
            let settings = ProjectPanelSettings::get_global(cx);
            (settings.file_icons, settings.folder_icons)
        };

        let expanded_entry_ids = self
            .state
            .expanded_dir_ids
            .get(&worktree_id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let is_expanded = expanded_entry_ids.binary_search(&entry.id).is_ok();

        let icon = match entry.kind {
            EntryKind::File => {
                if show_file_icons {
                    FileIcons::get_icon(entry.path.as_std_path(), cx)
                } else {
                    None
                }
            }
            _ => {
                if show_folder_icons {
                    FileIcons::get_folder_icon(is_expanded, entry.path.as_std_path(), cx)
                } else {
                    FileIcons::get_chevron_icon(is_expanded, cx)
                }
            }
        };

        let path_style = self.project.read(cx).path_style(cx);
        let (depth, difference) =
            ProjectPanel::calculate_depth_and_difference(entry, entries_paths);

        let filename = if difference > 1 {
            entry
                .path
                .last_n_components(difference)
                .map_or(String::new(), |suffix| {
                    suffix.display(path_style).to_string()
                })
        } else {
            entry
                .path
                .file_name()
                .map(|name| name.to_string())
                .unwrap_or_else(|| root_name.as_unix_str().to_string())
        };

        let selection = SelectedEntry {
            worktree_id,
            entry_id: entry.id,
        };
        let is_marked = self.marked_entries.contains(&selection);
        let is_selected = self.selection == Some(selection);

        let diagnostic_severity = self
            .diagnostics
            .get(&(worktree_id, entry.path.clone()))
            .cloned();

        let diagnostic_count = self
            .diagnostic_counts
            .get(&(worktree_id, entry.path.clone()))
            .copied();

        let filename_text_color =
            entry_git_aware_label_color(git_status, entry.is_ignored, is_marked);

        let is_cut = self
            .clipboard
            .as_ref()
            .is_some_and(|e| e.is_cut() && e.items().contains(&selection));

        EntryDetails {
            filename,
            icon,
            path: entry.path.clone(),
            depth,
            kind: entry.kind,
            is_ignored: entry.is_ignored,
            is_expanded,
            is_selected,
            is_marked,
            is_editing: false,
            is_processing: false,
            is_cut,
            sticky,
            filename_text_color,
            diagnostic_severity,
            diagnostic_count,
            git_status,
            is_private: entry.is_private,
            worktree_id,
            canonical_path: entry.canonical_path.clone(),
        }
    }
}
