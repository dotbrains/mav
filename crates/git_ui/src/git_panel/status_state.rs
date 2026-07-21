use super::*;

impl GitPanel {
    pub(super) fn header_state(&self, header_type: Section) -> ToggleState {
        let (staged_count, count) = match header_type {
            Section::New => (self.new_staged_count, self.new_count),
            Section::Tracked => (self.tracked_staged_count, self.tracked_count),
            Section::Conflict => (self.conflicted_staged_count, self.conflicted_count),
        };
        if staged_count == 0 {
            ToggleState::Unselected
        } else if count == staged_count {
            ToggleState::Selected
        } else {
            ToggleState::Indeterminate
        }
    }

    pub(super) fn update_counts(&mut self, repo: &Repository) {
        self.show_placeholders = false;
        self.conflicted_count = 0;
        self.conflicted_staged_count = 0;
        self.new_count = 0;
        self.tracked_count = 0;
        self.new_staged_count = 0;
        self.tracked_staged_count = 0;
        self.entry_count = 0;
        self.diff_stat_total = DiffStat::default();

        for status_entry in self.entries.iter().filter_map(|entry| entry.status_entry()) {
            self.entry_count += 1;
            if let Some(diff_stat) = status_entry.diff_stat {
                self.diff_stat_total.added =
                    self.diff_stat_total.added.saturating_add(diff_stat.added);
                self.diff_stat_total.deleted = self
                    .diff_stat_total
                    .deleted
                    .saturating_add(diff_stat.deleted);
            }

            let is_staging_or_staged = GitPanel::stage_status_for_entry(status_entry, repo)
                .as_bool()
                .unwrap_or(true);

            if repo.had_conflict_on_last_merge_head_change(&status_entry.repo_path) {
                self.conflicted_count += 1;
                if is_staging_or_staged {
                    self.conflicted_staged_count += 1;
                }
            } else if status_entry.status.is_created() {
                self.new_count += 1;
                if is_staging_or_staged {
                    self.new_staged_count += 1;
                }
            } else {
                self.tracked_count += 1;
                if is_staging_or_staged {
                    self.tracked_staged_count += 1;
                }
            }
        }
    }

    pub(crate) fn has_staged_changes(&self) -> bool {
        self.tracked_staged_count > 0
            || self.new_staged_count > 0
            || self.conflicted_staged_count > 0
    }

    pub(crate) fn has_unstaged_changes(&self) -> bool {
        self.tracked_count > self.tracked_staged_count
            || self.new_count > self.new_staged_count
            || self.conflicted_count > self.conflicted_staged_count
    }

    pub(super) fn has_tracked_changes(&self) -> bool {
        self.tracked_count > 0
    }

    pub fn has_unstaged_conflicts(&self) -> bool {
        self.conflicted_count > 0 && self.conflicted_count != self.conflicted_staged_count
    }
}
