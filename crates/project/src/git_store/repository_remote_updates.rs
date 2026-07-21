use super::*;

impl Repository {
    pub(crate) fn apply_remote_update(
        &mut self,
        update: proto::UpdateRepository,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        if let Some(repository_dir_abs_path) = &update.repository_dir_abs_path {
            self.snapshot.repository_dir_abs_path =
                Path::new(repository_dir_abs_path.as_str()).into();
        }
        if let Some(common_dir_abs_path) = &update.common_dir_abs_path {
            self.snapshot.common_dir_abs_path = Path::new(common_dir_abs_path.as_str()).into();
        }

        let new_branch = update.branch_summary.as_ref().map(proto_to_branch);
        let new_head_commit = update
            .head_commit_details
            .as_ref()
            .map(proto_to_commit_details);
        if self.snapshot.branch != new_branch || self.snapshot.head_commit != new_head_commit {
            cx.emit(RepositoryEvent::HeadChanged)
        }
        self.snapshot.branch = new_branch;
        self.snapshot.head_commit = new_head_commit;

        if update.is_last_update {
            let new_branch_list: Arc<[Branch]> =
                update.branch_list.iter().map(proto_to_branch).collect();
            let new_branch_list_error = update.branch_list_error.map(SharedString::from);
            if *self.snapshot.branch_list != *new_branch_list
                || self.snapshot.branch_list_error != new_branch_list_error
            {
                cx.emit(RepositoryEvent::BranchListChanged);
            }
            self.snapshot.branch_list = new_branch_list;
            self.snapshot.branch_list_error = new_branch_list_error;
        }

        // We don't store any merge head state for downstream projects; the upstream
        // will track it and we will just get the updated conflicts
        let new_merge_heads = TreeMap::from_ordered_entries(
            update
                .current_merge_conflicts
                .into_iter()
                .filter_map(|path| Some((RepoPath::from_proto(&path).ok()?, vec![]))),
        );
        let conflicts_changed =
            self.snapshot.merge.merge_heads_by_conflicted_path != new_merge_heads;
        self.snapshot.merge.merge_heads_by_conflicted_path = new_merge_heads;
        self.snapshot.merge.message = update.merge_message.map(SharedString::from);
        let new_stash_entries = GitStash {
            entries: update
                .stash_entries
                .iter()
                .filter_map(|entry| proto_to_stash(entry).ok())
                .collect(),
        };
        if self.snapshot.stash_entries != new_stash_entries {
            cx.emit(RepositoryEvent::StashEntriesChanged)
        }
        self.snapshot.stash_entries = new_stash_entries;
        let new_linked_worktrees: Arc<[GitWorktree]> = update
            .linked_worktrees
            .iter()
            .map(proto_to_worktree)
            .collect();
        if *self.snapshot.linked_worktrees != *new_linked_worktrees {
            cx.emit(RepositoryEvent::GitWorktreeListChanged);
        }
        self.snapshot.linked_worktrees = new_linked_worktrees;
        self.snapshot.remote_upstream_url = update.remote_upstream_url;
        self.snapshot.remote_origin_url = update.remote_origin_url;

        let edits = update
            .removed_statuses
            .into_iter()
            .filter_map(|path| {
                Some(sum_tree::Edit::Remove(PathKey(
                    RelPath::from_proto(&path).log_err()?,
                )))
            })
            .chain(
                update
                    .updated_statuses
                    .into_iter()
                    .filter_map(|updated_status| {
                        Some(sum_tree::Edit::Insert(updated_status.try_into().log_err()?))
                    }),
            )
            .collect::<Vec<_>>();
        if conflicts_changed || !edits.is_empty() {
            cx.emit(RepositoryEvent::StatusesChanged);
        }
        self.snapshot.statuses_by_path.edit(edits, ());

        if update.is_last_update {
            self.snapshot.scan_id = update.scan_id;
        }
        self.clear_pending_ops(cx);
        Ok(())
    }
}
