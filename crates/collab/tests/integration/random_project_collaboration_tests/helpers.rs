use super::*;

pub(super) fn generate_git_operation(rng: &mut StdRng, client: &TestClient) -> GitOperation {
    fn generate_file_paths(
        repo_path: &Path,
        rng: &mut StdRng,
        client: &TestClient,
    ) -> Vec<RelPathBuf> {
        let mut paths = client
            .fs()
            .files()
            .into_iter()
            .filter(|path| path.starts_with(repo_path))
            .collect::<Vec<_>>();

        let count = rng.random_range(0..=paths.len());
        paths.shuffle(rng);
        paths.truncate(count);

        paths
            .iter()
            .map(|path| {
                RelPath::new(path.strip_prefix(repo_path).unwrap(), PathStyle::local())
                    .unwrap()
                    .to_rel_path_buf()
            })
            .collect::<Vec<_>>()
    }

    let repo_path = client.fs().directories(false).choose(rng).unwrap().clone();

    match rng.random_range(0..100_u32) {
        0..=25 => {
            let file_paths = generate_file_paths(&repo_path, rng, client);

            let contents = file_paths
                .into_iter()
                .map(|path| (path, distr::Alphanumeric.sample_string(rng, 16)))
                .collect();

            GitOperation::WriteGitIndex {
                repo_path,
                contents,
            }
        }
        26..=63 => {
            let new_branch =
                (rng.random_range(0..10) > 3).then(|| distr::Alphanumeric.sample_string(rng, 8));

            GitOperation::WriteGitBranch {
                repo_path,
                new_branch,
            }
        }
        64..=100 => {
            let file_paths = generate_file_paths(&repo_path, rng, client);
            let statuses = file_paths
                .into_iter()
                .map(|path| (path, gen_status(rng)))
                .collect::<Vec<_>>();
            GitOperation::WriteGitStatuses {
                repo_path,
                statuses,
            }
        }
        _ => unreachable!(),
    }
}

pub(super) fn buffer_for_full_path(
    client: &TestClient,
    project: &Entity<Project>,
    full_path: &RelPath,
    cx: &TestAppContext,
) -> Option<Entity<language::Buffer>> {
    client
        .buffers_for_project(project)
        .iter()
        .find(|buffer| {
            buffer.read_with(cx, |buffer, cx| {
                let file = buffer.file().unwrap();
                let Some(worktree) = project.read(cx).worktree_for_id(file.worktree_id(cx), cx)
                else {
                    return false;
                };
                worktree.read(cx).root_name().join(&file.path()).as_ref() == full_path
            })
        })
        .cloned()
}

pub(super) fn project_for_root_name(
    client: &TestClient,
    root_name: &str,
    cx: &TestAppContext,
) -> Option<Entity<Project>> {
    if let Some(ix) = project_ix_for_root_name(client.local_projects().deref(), root_name, cx) {
        return Some(client.local_projects()[ix].clone());
    }
    if let Some(ix) = project_ix_for_root_name(client.dev_server_projects().deref(), root_name, cx)
    {
        return Some(client.dev_server_projects()[ix].clone());
    }
    None
}

pub(super) fn project_ix_for_root_name(
    projects: &[Entity<Project>],
    root_name: &str,
    cx: &TestAppContext,
) -> Option<usize> {
    projects.iter().position(|project| {
        project.read_with(cx, |project, cx| {
            let worktree = project.visible_worktrees(cx).next().unwrap();
            worktree.read(cx).root_name() == root_name
        })
    })
}

pub(super) fn root_name_for_project(project: &Entity<Project>, cx: &TestAppContext) -> String {
    project.read_with(cx, |project, cx| {
        project
            .visible_worktrees(cx)
            .next()
            .unwrap()
            .read(cx)
            .root_name_str()
            .to_string()
    })
}

pub(super) fn project_path_for_full_path(
    project: &Entity<Project>,
    full_path: &RelPath,
    cx: &TestAppContext,
) -> Option<ProjectPath> {
    let mut components = full_path.components();
    let root_name = components.next().unwrap();
    let path = components.rest().into();
    let worktree_id = project.read_with(cx, |project, cx| {
        project.worktrees(cx).find_map(|worktree| {
            let worktree = worktree.read(cx);
            if worktree.root_name_str() == root_name {
                Some(worktree.id())
            } else {
                None
            }
        })
    })?;
    Some(ProjectPath { worktree_id, path })
}

pub(super) async fn ensure_project_shared(
    project: &Entity<Project>,
    client: &TestClient,
    cx: &mut TestAppContext,
) {
    let first_root_name = root_name_for_project(project, cx);
    let active_call = cx.read(ActiveCall::global);
    if active_call.read_with(cx, |call, _| call.room().is_some())
        && project.read_with(cx, |project, _| project.is_local() && !project.is_shared())
    {
        match active_call
            .update(cx, |call, cx| call.share_project(project.clone(), cx))
            .await
        {
            Ok(project_id) => {
                log::info!(
                    "{}: shared project {} with id {}",
                    client.username,
                    first_root_name,
                    project_id
                );
            }
            Err(error) => {
                log::error!(
                    "{}: error sharing project {}: {:?}",
                    client.username,
                    first_root_name,
                    error
                );
            }
        }
    }
}

pub(super) fn choose_random_project(
    client: &TestClient,
    rng: &mut StdRng,
) -> Option<Entity<Project>> {
    client
        .local_projects()
        .deref()
        .iter()
        .chain(client.dev_server_projects().iter())
        .choose(rng)
        .cloned()
}

pub(super) fn gen_file_name(rng: &mut StdRng) -> String {
    let mut name = String::new();
    for _ in 0..10 {
        let letter = rng.random_range('a'..='z');
        name.push(letter);
    }
    name
}

pub(super) fn gen_status(rng: &mut StdRng) -> FileStatus {
    fn gen_tracked_status(rng: &mut StdRng) -> TrackedStatus {
        match rng.random_range(0..3) {
            0 => TrackedStatus {
                index_status: StatusCode::Unmodified,
                worktree_status: StatusCode::Unmodified,
            },
            1 => TrackedStatus {
                index_status: StatusCode::Modified,
                worktree_status: StatusCode::Modified,
            },
            2 => TrackedStatus {
                index_status: StatusCode::Added,
                worktree_status: StatusCode::Modified,
            },
            3 => TrackedStatus {
                index_status: StatusCode::Added,
                worktree_status: StatusCode::Unmodified,
            },
            _ => unreachable!(),
        }
    }

    fn gen_unmerged_status_code(rng: &mut StdRng) -> UnmergedStatusCode {
        match rng.random_range(0..3) {
            0 => UnmergedStatusCode::Updated,
            1 => UnmergedStatusCode::Added,
            2 => UnmergedStatusCode::Deleted,
            _ => unreachable!(),
        }
    }

    match rng.random_range(0..2) {
        0 => FileStatus::Unmerged(UnmergedStatus {
            first_head: gen_unmerged_status_code(rng),
            second_head: gen_unmerged_status_code(rng),
        }),
        1 => FileStatus::Tracked(gen_tracked_status(rng)),
        _ => unreachable!(),
    }
}
