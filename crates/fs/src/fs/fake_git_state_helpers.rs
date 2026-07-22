use super::*;

#[cfg(feature = "test-support")]
impl FakeFs {
    pub fn with_git_state<T, F>(&self, dot_git: &Path, emit_git_event: bool, f: F) -> Result<T>
    where
        F: FnOnce(&mut FakeGitRepositoryState) -> T,
    {
        self.with_git_state_and_paths(dot_git, emit_git_event, |state, _, _| f(state))
    }

    pub fn set_branch_name(&self, dot_git: &Path, branch: Option<impl Into<String>>) {
        self.with_git_state(dot_git, true, |state| {
            let branch = branch.map(Into::into);
            state.branches.extend(branch.clone());
            state.current_branch_name = branch
        })
        .unwrap();
    }

    pub fn set_remote_for_repo(
        &self,
        dot_git: &Path,
        name: impl Into<String>,
        url: impl Into<String>,
    ) {
        self.with_git_state(dot_git, true, |state| {
            state.remotes.insert(name.into(), url.into());
        })
        .unwrap();
    }

    pub fn insert_branches(&self, dot_git: &Path, branches: &[&str]) {
        self.with_git_state(dot_git, true, |state| {
            if let Some(first) = branches.first()
                && state.current_branch_name.is_none()
            {
                state.current_branch_name = Some(first.to_string())
            }
            state
                .branches
                .extend(branches.iter().map(ToString::to_string));
        })
        .unwrap();
    }

    pub async fn add_linked_worktree_for_repo(
        &self,
        dot_git: &Path,
        emit_git_event: bool,
        worktree: Worktree,
    ) {
        let ref_name = worktree
            .ref_name
            .as_ref()
            .expect("linked worktree must have a ref_name");
        let branch_name = ref_name
            .strip_prefix("refs/heads/")
            .unwrap_or(ref_name.as_ref());

        // Create ref in git state.
        self.with_git_state(dot_git, false, |state| {
            state
                .refs
                .insert(ref_name.to_string(), worktree.sha.to_string());
        })
        .unwrap();

        // Create .git/worktrees/<name>/ directory with HEAD, commondir, and gitdir.
        let worktrees_entry_dir = dot_git.join("worktrees").join(branch_name);
        self.create_dir(&worktrees_entry_dir).await.unwrap();

        self.write_file_internal(
            worktrees_entry_dir.join("HEAD"),
            format!("ref: {ref_name}").into_bytes(),
            false,
        )
        .unwrap();

        self.write_file_internal(
            worktrees_entry_dir.join("commondir"),
            dot_git.to_string_lossy().into_owned().into_bytes(),
            false,
        )
        .unwrap();

        let worktree_dot_git = worktree.path.join(".git");
        self.write_file_internal(
            worktrees_entry_dir.join("gitdir"),
            worktree_dot_git.to_string_lossy().into_owned().into_bytes(),
            false,
        )
        .unwrap();

        // Create the worktree checkout directory with a .git file pointing back.
        self.create_dir(&worktree.path).await.unwrap();

        self.write_file_internal(
            &worktree_dot_git,
            format!("gitdir: {}", worktrees_entry_dir.display()).into_bytes(),
            false,
        )
        .unwrap();

        if emit_git_event {
            self.with_git_state(dot_git, true, |_| {}).unwrap();
        }
    }

    pub async fn remove_worktree_for_repo(
        &self,
        dot_git: &Path,
        emit_git_event: bool,
        ref_name: &str,
    ) {
        let branch_name = ref_name.strip_prefix("refs/heads/").unwrap_or(ref_name);
        let worktrees_entry_dir = dot_git.join("worktrees").join(branch_name);

        // Read gitdir to find the worktree checkout path.
        let gitdir_content = self
            .load_internal(worktrees_entry_dir.join("gitdir"))
            .await
            .unwrap();
        let gitdir_str = String::from_utf8(gitdir_content).unwrap();
        let worktree_path = PathBuf::from(gitdir_str.trim())
            .parent()
            .map(PathBuf::from)
            .unwrap_or_default();

        // Remove the worktree checkout directory.
        self.remove_dir(
            &worktree_path,
            RemoveOptions {
                recursive: true,
                ignore_if_not_exists: true,
            },
        )
        .await
        .unwrap();

        // Remove the .git/worktrees/<name>/ directory.
        self.remove_dir(
            &worktrees_entry_dir,
            RemoveOptions {
                recursive: true,
                ignore_if_not_exists: false,
            },
        )
        .await
        .unwrap();

        if emit_git_event {
            self.with_git_state(dot_git, true, |_| {}).unwrap();
        }
    }

    pub fn set_unmerged_paths_for_repo(
        &self,
        dot_git: &Path,
        unmerged_state: &[(RepoPath, UnmergedStatus)],
    ) {
        self.with_git_state(dot_git, true, |state| {
            state.unmerged_paths.clear();
            state.unmerged_paths.extend(
                unmerged_state
                    .iter()
                    .map(|(path, content)| (path.clone(), *content)),
            );
        })
        .unwrap();
    }

    pub fn set_index_for_repo(&self, dot_git: &Path, index_state: &[(&str, String)]) {
        self.with_git_state(dot_git, true, |state| {
            state.index_contents.clear();
            state.index_contents.extend(
                index_state
                    .iter()
                    .map(|(path, content)| (repo_path(path), content.clone())),
            );
        })
        .unwrap();
    }

    pub fn set_head_for_repo(
        &self,
        dot_git: &Path,
        head_state: &[(&str, String)],
        sha: impl Into<String>,
    ) {
        self.with_git_state(dot_git, true, |state| {
            state.head_contents.clear();
            state.head_contents.extend(
                head_state
                    .iter()
                    .map(|(path, content)| (repo_path(path), content.clone())),
            );
            state.refs.insert("HEAD".into(), sha.into());
        })
        .unwrap();
    }

    pub fn set_head_and_index_for_repo(&self, dot_git: &Path, contents_by_path: &[(&str, String)]) {
        self.with_git_state(dot_git, true, |state| {
            state.head_contents.clear();
            state.head_contents.extend(
                contents_by_path
                    .iter()
                    .map(|(path, contents)| (repo_path(path), contents.clone())),
            );
            state.index_contents = state.head_contents.clone();
        })
        .unwrap();
    }

    pub fn set_merge_base_content_for_repo(
        &self,
        dot_git: &Path,
        contents_by_path: &[(&str, String)],
    ) {
        self.with_git_state(dot_git, true, |state| {
            use git::Oid;

            state.merge_base_contents.clear();
            let oids = (1..)
                .map(|n| n.to_string())
                .map(|n| Oid::from_bytes(n.repeat(20).as_bytes()).unwrap());
            for ((path, content), oid) in contents_by_path.iter().zip(oids) {
                state.merge_base_contents.insert(repo_path(path), oid);
                state.oids.insert(oid, content.clone());
            }
        })
        .unwrap();
    }

    pub fn set_blame_for_repo(&self, dot_git: &Path, blames: Vec<(RepoPath, git::blame::Blame)>) {
        self.with_git_state(dot_git, true, |state| {
            state.blames.clear();
            state.blames.extend(blames);
        })
        .unwrap();
    }

    pub fn set_graph_commits(&self, dot_git: &Path, commits: Vec<Arc<InitialGraphCommitData>>) {
        self.with_git_state(dot_git, true, |state| {
            state.graph_commits = commits;
        })
        .unwrap();
    }

    pub fn set_graph_error(&self, dot_git: &Path, error: Option<String>) {
        self.with_git_state(dot_git, true, |state| {
            state.simulated_graph_error = error;
        })
        .unwrap();
    }

    pub fn set_commit_data(
        &self,
        dot_git: &Path,
        commit_data: impl IntoIterator<Item = (CommitData, bool)>,
    ) {
        self.with_git_state(dot_git, true, |state| {
            state.commit_data = commit_data
                .into_iter()
                .map(|(data, should_fail)| {
                    (
                        data.sha,
                        if should_fail {
                            FakeCommitDataEntry::Fail(data)
                        } else {
                            FakeCommitDataEntry::Success(data)
                        },
                    )
                })
                .collect();
        })
        .unwrap();
    }

    /// Put the given git repository into a state with the given status,
    /// by mutating the head, index, and unmerged state.
    pub fn set_status_for_repo(&self, dot_git: &Path, statuses: &[(&str, FileStatus)]) {
        let workdir_path = dot_git.parent().unwrap();
        let workdir_contents = self.files_with_contents(workdir_path);
        self.with_git_state(dot_git, true, |state| {
            state.index_contents.clear();
            state.head_contents.clear();
            state.unmerged_paths.clear();
            for (path, content) in workdir_contents {
                use util::{paths::PathStyle, rel_path::RelPath};

                let repo_path = RelPath::new(path.strip_prefix(&workdir_path).unwrap(), PathStyle::local()).unwrap();
                let repo_path = RepoPath::from_rel_path(&repo_path);
                let status = statuses
                    .iter()
                    .find_map(|(p, status)| (*p == repo_path.as_unix_str()).then_some(status));
                let mut content = String::from_utf8_lossy(&content).to_string();

                let mut index_content = None;
                let mut head_content = None;
                match status {
                    None => {
                        index_content = Some(content.clone());
                        head_content = Some(content);
                    }
                    Some(FileStatus::Untracked | FileStatus::Ignored) => {}
                    Some(FileStatus::Unmerged(unmerged_status)) => {
                        state
                            .unmerged_paths
                            .insert(repo_path.clone(), *unmerged_status);
                        content.push_str(" (unmerged)");
                        index_content = Some(content.clone());
                        head_content = Some(content);
                    }
                    Some(FileStatus::Tracked(TrackedStatus {
                        index_status,
                        worktree_status,
                    })) => {
                        match worktree_status {
                            StatusCode::Modified => {
                                let mut content = content.clone();
                                content.push_str(" (modified in working copy)");
                                index_content = Some(content);
                            }
                            StatusCode::TypeChanged | StatusCode::Unmodified => {
                                index_content = Some(content.clone());
                            }
                            StatusCode::Added => {}
                            StatusCode::Deleted | StatusCode::Renamed | StatusCode::Copied => {
                                panic!("cannot create these statuses for an existing file");
                            }
                        };
                        match index_status {
                            StatusCode::Modified => {
                                let mut content = index_content.clone().expect(
                                    "file cannot be both modified in index and created in working copy",
                                );
                                content.push_str(" (modified in index)");
                                head_content = Some(content);
                            }
                            StatusCode::TypeChanged | StatusCode::Unmodified => {
                                head_content = Some(index_content.clone().expect("file cannot be both unmodified in index and created in working copy"));
                            }
                            StatusCode::Added => {}
                            StatusCode::Deleted  => {
                                head_content = Some("".into());
                            }
                            StatusCode::Renamed | StatusCode::Copied => {
                                panic!("cannot create these statuses for an existing file");
                            }
                        };
                    }
                };

                if let Some(content) = index_content {
                    state.index_contents.insert(repo_path.clone(), content);
                }
                if let Some(content) = head_content {
                    state.head_contents.insert(repo_path.clone(), content);
                }
            }
        }).unwrap();
    }

    pub fn set_error_message_for_index_write(&self, dot_git: &Path, message: Option<String>) {
        self.with_git_state(dot_git, true, |state| {
            state.simulated_index_write_error_message = message;
        })
        .unwrap();
    }

    pub fn set_create_worktree_error(&self, dot_git: &Path, message: Option<String>) {
        self.with_git_state(dot_git, true, |state| {
            state.simulated_create_worktree_error = message;
        })
        .unwrap();
    }

    /// Makes subsequent `remove_dir` calls for `path` fail with `message`.
    pub fn set_remove_dir_error(&self, path: impl AsRef<Path>, message: String) {
        self.state
            .lock()
            .remove_dir_errors
            .insert(normalize_path(path.as_ref()), message);
    }
}
