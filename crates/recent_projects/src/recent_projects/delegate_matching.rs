use super::*;

impl RecentProjectsDelegate {
    pub(super) fn update_delegate_matches(
        &mut self,
        query: String,
        _: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> gpui::Task<()> {
        let query = query.trim_start();
        let case = fuzzy_nucleo::Case::smart_if_uppercase_in(query);
        let is_empty_query = query.is_empty();

        let folder_matches = if self.open_folders.is_empty() {
            Vec::new()
        } else {
            let candidates: Vec<_> = self
                .open_folders
                .iter()
                .enumerate()
                .map(|(id, folder)| StringMatchCandidate::new(id, folder.name.as_ref()))
                .collect();

            match_strings(
                &candidates,
                query,
                case,
                fuzzy_nucleo::LengthPenalty::On,
                100,
            )
        };

        let project_group_candidates: Vec<_> = self
            .window_project_groups
            .iter()
            .enumerate()
            .map(|(id, key)| {
                let combined_string = key
                    .path_list()
                    .ordered_paths()
                    .map(|path| path.compact().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .concat();
                StringMatchCandidate::new(id, &combined_string)
            })
            .collect();

        let project_group_matches = match_strings(
            &project_group_candidates,
            query,
            case,
            fuzzy_nucleo::LengthPenalty::On,
            100,
        );

        // Build candidates for recent projects (not current, not sibling, not open folder)
        let recent_candidates: Vec<_> = self
            .workspaces
            .iter()
            .enumerate()
            .filter(|(_, workspace)| self.is_valid_recent_candidate(workspace, cx))
            .map(|(id, workspace)| {
                let combined_string = workspace
                    .identity_paths
                    .ordered_paths()
                    .map(|path| path.compact().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .concat();
                StringMatchCandidate::new(id, &combined_string)
            })
            .collect();

        let recent_matches = match_strings(
            &recent_candidates,
            query,
            case,
            fuzzy_nucleo::LengthPenalty::On,
            100,
        );

        let mut entries = Vec::new();

        if !self.open_folders.is_empty() {
            let matched_folders: Vec<_> = if is_empty_query {
                (0..self.open_folders.len())
                    .map(|i| (i, Vec::new()))
                    .collect()
            } else {
                folder_matches
                    .iter()
                    .map(|m| (m.candidate_id, m.positions.clone()))
                    .collect()
            };

            if !matched_folders.is_empty() {
                entries.push(ProjectPickerEntry::Header("Current Folders".into()));
                for (index, positions) in matched_folders {
                    entries.push(ProjectPickerEntry::OpenFolder { index, positions });
                }
            }
        }

        let has_projects_to_show = if is_empty_query {
            !project_group_candidates.is_empty()
        } else {
            !project_group_matches.is_empty()
        };

        if has_projects_to_show {
            entries.push(ProjectPickerEntry::Header("This Window".into()));

            if is_empty_query {
                for id in 0..self.window_project_groups.len() {
                    entries.push(ProjectPickerEntry::ProjectGroup(StringMatch {
                        candidate_id: id,
                        score: 0.0,
                        positions: Vec::new(),
                        string: Default::default(),
                    }));
                }
            } else {
                for m in project_group_matches {
                    entries.push(ProjectPickerEntry::ProjectGroup(m));
                }
            }
        }

        let has_recent_to_show = if is_empty_query {
            !recent_candidates.is_empty()
        } else {
            !recent_matches.is_empty()
        };

        if has_recent_to_show {
            entries.push(ProjectPickerEntry::Header("Recent Projects".into()));

            if is_empty_query {
                for (id, workspace) in self.workspaces.iter().enumerate() {
                    if self.is_valid_recent_candidate(workspace, cx) {
                        entries.push(ProjectPickerEntry::RecentProject(StringMatch {
                            candidate_id: id,
                            score: 0.0,
                            positions: Vec::new(),
                            string: Default::default(),
                        }));
                    }
                }
            } else {
                for m in recent_matches {
                    entries.push(ProjectPickerEntry::RecentProject(m));
                }
            }
        }

        self.filtered_entries = entries;

        if self.snap_selection_to_first_non_header_match {
            self.selected_index = self
                .filtered_entries
                .iter()
                .position(|e| !matches!(e, ProjectPickerEntry::Header(_)))
                .unwrap_or(0);
        }
        self.snap_selection_to_first_non_header_match = true;
        Task::ready(())
    }
}
