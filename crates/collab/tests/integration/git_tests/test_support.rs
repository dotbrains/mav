use super::*;

pub(super) fn collect_diff_stats<C: gpui::AppContext>(
    panel: &gpui::Entity<GitPanel>,
    cx: &C,
) -> HashMap<RepoPath, DiffStat> {
    panel.read_with(cx, |panel, cx| {
        let Some(repo) = panel.active_repository() else {
            return HashMap::default();
        };
        let snapshot = repo.read(cx).snapshot();
        let mut stats = HashMap::default();
        for entry in snapshot.statuses_by_path.iter() {
            if let Some(diff_stat) = entry.diff_stat {
                stats.insert(entry.repo_path.clone(), diff_stat);
            }
        }
        stats
    })
}

pub(super) async fn load_commit_data_batch(
    repository: &gpui::Entity<Repository>,
    shas: &[Oid],
    executor: &BackgroundExecutor,
    cx: &mut TestAppContext,
) -> HashMap<Oid, CommitData> {
    let states = cx.update(|cx| {
        shas.iter()
            .map(|sha| {
                (
                    *sha,
                    repository.update(cx, |repository, cx| {
                        repository.fetch_commit_data(*sha, true, cx).clone()
                    }),
                )
            })
            .collect::<Vec<_>>()
    });

    executor.run_until_parked();

    let mut commit_data = HashMap::default();
    for (sha, state) in states {
        let data = match state {
            CommitDataState::Loaded(data) => data.as_ref().clone(),
            CommitDataState::Loading(Some(shared)) => shared.await.unwrap().as_ref().clone(),
            CommitDataState::Loading(None) => {
                panic!("fetch_commit_data(..., true) should return an await-result state")
            }
        };
        commit_data.insert(sha, data);
    }

    commit_data
}

pub(super) fn branch_list_snapshot(
    project: &gpui::Entity<project::Project>,
    cx: &mut TestAppContext,
) -> (Option<String>, Vec<String>) {
    project.read_with(cx, |project, cx| {
        let repos = project.repositories(cx);
        assert_eq!(repos.len(), 1, "project should have exactly 1 repository");
        let repo = repos.values().next().unwrap();
        let snapshot = repo.read(cx).snapshot();
        (
            snapshot
                .branch
                .as_ref()
                .map(|branch| branch.name().to_string()),
            snapshot
                .branch_list
                .iter()
                .map(|branch| branch.ref_name.to_string())
                .collect(),
        )
    })
}

pub(super) fn build_git_graph(
    project: &Entity<project::Project>,
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
) -> Entity<GitGraph> {
    let (repository_id, git_store) = project.read_with(cx, |project, cx| {
        let repository = project
            .active_repository(cx)
            .expect("project should have an active repository");
        (repository.read(cx).id, project.git_store().clone())
    });
    let workspace = workspace.downgrade();

    cx.new_window_entity(|window, cx| {
        GitGraph::new(repository_id, git_store, workspace, None, window, cx)
    })
}

pub(super) fn render_git_graph(graph: &Entity<GitGraph>, cx: &mut VisualTestContext) {
    cx.draw(point(px(0.), px(0.)), size(px(1200.), px(800.)), |_, _| {
        graph.clone().into_any_element()
    });
    cx.run_until_parked();
}

pub(super) fn assert_initial_graph_commits_eq(
    actual: &[Arc<InitialGraphCommitData>],
    expected: &[Arc<InitialGraphCommitData>],
) {
    assert_eq!(actual.len(), expected.len(), "commit count should match");
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        assert_eq!(
            actual.sha, expected.sha,
            "sha should match at index {index}"
        );
        assert_eq!(
            actual.parents, expected.parents,
            "parents should match at index {index}"
        );
        assert_eq!(
            actual.ref_names, expected.ref_names,
            "ref names should match at index {index}"
        );
    }
}

pub(super) fn assert_remote_cache_matches_local_cache(
    local_repository: &gpui::Entity<Repository>,
    remote_repository: &gpui::Entity<Repository>,
    cx_local: &mut TestAppContext,
    cx_remote: &mut TestAppContext,
) {
    let local_cache = cx_local.update(|cx| {
        local_repository.update(cx, |repository, _| repository.loaded_commit_data_for_test())
    });
    let remote_cache = cx_remote.update(|cx| {
        remote_repository.update(cx, |repository, _| repository.loaded_commit_data_for_test())
    });

    for (sha, remote_commit_data) in &remote_cache {
        let local_commit_data = local_cache
            .get(sha)
            .unwrap_or_else(|| panic!("local cache missing commit data for {sha}"));
        assert_eq!(
            local_commit_data.sha, remote_commit_data.sha,
            "local and remote cache should agree on sha for {sha}"
        );
        assert_eq!(
            local_commit_data.parents, remote_commit_data.parents,
            "local and remote cache should agree on parents for {sha}"
        );
        assert_eq!(
            local_commit_data.author_name, remote_commit_data.author_name,
            "local and remote cache should agree on author_name for {sha}"
        );
        assert_eq!(
            local_commit_data.author_email, remote_commit_data.author_email,
            "local and remote cache should agree on author_email for {sha}"
        );
        assert_eq!(
            local_commit_data.commit_timestamp, remote_commit_data.commit_timestamp,
            "local and remote cache should agree on commit_timestamp for {sha}"
        );
        assert_eq!(
            local_commit_data.subject, remote_commit_data.subject,
            "local and remote cache should agree on subject for {sha}"
        );
        assert_eq!(
            local_commit_data.message, remote_commit_data.message,
            "local and remote cache should agree on message for {sha}"
        );
    }
}
