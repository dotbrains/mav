use super::*;

#[gpui::test]
async fn test_empty_nested_repository_graph_stops_loading(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        Path::new("/project"),
        json!({
            "repo_a": {
                ".git": {},
                "file_a.txt": "content",
            },
            "repo_b": {
                ".git": {},
                "file_b.txt": "content",
            },
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.run_until_parked();

    let repository = project.read_with(cx, |project, cx| {
        assert_eq!(project.repositories(cx).len(), 2);
        project
            .active_repository(cx)
            .expect("should have an active repository")
    });

    let (multi_workspace, cx) = cx.add_window_view(|window, cx| {
        workspace::MultiWorkspace::test_new(project.clone(), window, cx)
    });
    let workspace = multi_workspace.read_with(&*cx, |multi, _| multi.workspace().downgrade());
    let git_graph = cx.new_window_entity(|window, cx| {
        GitGraph::new(
            repository.read(cx).id,
            project.read(cx).git_store().clone(),
            workspace,
            None,
            window,
            cx,
        )
    });
    cx.run_until_parked();

    let (commit_count, is_loading) = git_graph.update(cx, |graph, cx| {
        graph.commit_count_and_loading_state_for_test(cx)
    });

    assert_eq!(commit_count, 0);
    assert!(!is_loading, "empty graph data should stop loading");
}

#[gpui::test]
async fn test_initial_graph_data_not_cleared_on_initial_loading(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        Path::new("/project"),
        json!({
            ".git": {},
            "file.txt": "content",
        }),
    )
    .await;

    let mut rng = StdRng::seed_from_u64(42);
    let commits = generate_random_commit_dag(&mut rng, 10, false);
    fs.set_graph_commits(Path::new("/project/.git"), commits.clone());

    let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;
    let observed_repository_events = Arc::new(Mutex::new(Vec::new()));
    project.update(cx, |project, cx| {
        let observed_repository_events = observed_repository_events.clone();
        cx.subscribe(project.git_store(), move |_, _, event, _| {
            if let GitStoreEvent::RepositoryUpdated(_, repository_event, true) = event {
                observed_repository_events
                    .lock()
                    .expect("repository event mutex should be available")
                    .push(repository_event.clone());
            }
        })
        .detach();
    });

    let repository = project.read_with(cx, |project, cx| {
        project
            .active_repository(cx)
            .expect("should have a repository")
    });

    repository.update(cx, |repo, cx| {
        repo.graph_data(LogSource::default(), LogOrder::default(), 0..usize::MAX, cx);
    });

    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.run_until_parked();

    let observed_repository_events = observed_repository_events
        .lock()
        .expect("repository event mutex should be available");
    assert!(
        observed_repository_events
            .iter()
            .any(|event| matches!(event, RepositoryEvent::HeadChanged)),
        "initial repository scan should emit HeadChanged"
    );
    let commit_count_after = repository.read_with(cx, |repo, _| {
        repo.get_graph_data(LogSource::default(), LogOrder::default())
            .map(|data| data.commit_data.len())
            .unwrap()
    });
    assert_eq!(
        commits.len(),
        commit_count_after,
        "initial_graph_data should remain populated after events emitted by initial repository scan"
    );
}

#[gpui::test]
async fn test_initial_graph_data_propagates_error(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        Path::new("/project"),
        json!({
            ".git": {},
            "file.txt": "content",
        }),
    )
    .await;

    fs.set_graph_error(
        Path::new("/project/.git"),
        Some("fatal: bad default revision 'HEAD'".to_string()),
    );

    let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;

    let repository = project.read_with(cx, |project, cx| {
        project
            .active_repository(cx)
            .expect("should have a repository")
    });

    repository.update(cx, |repo, cx| {
        repo.graph_data(LogSource::default(), LogOrder::default(), 0..usize::MAX, cx);
    });

    cx.run_until_parked();

    let error = repository.read_with(cx, |repo, _| {
        repo.get_graph_data(LogSource::default(), LogOrder::default())
            .and_then(|data| data.error.clone())
    });

    assert!(
        error.is_some(),
        "graph data should contain an error after initial_graph_data fails"
    );
    let error_message = error.unwrap();
    assert!(
        error_message.contains("bad default revision"),
        "error should contain the git error message, got: {}",
        error_message
    );
}

#[gpui::test]
async fn test_graph_data_repopulated_from_cache_after_repo_switch(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        Path::new("/project_a"),
        json!({
            ".git": {},
            "file.txt": "content",
        }),
    )
    .await;
    fs.insert_tree(
        Path::new("/project_b"),
        json!({
            ".git": {},
            "other.txt": "content",
        }),
    )
    .await;

    let mut rng = StdRng::seed_from_u64(42);
    let commits = generate_random_commit_dag(&mut rng, 10, false);
    fs.set_graph_commits(Path::new("/project_a/.git"), commits.clone());

    let project = Project::test(
        fs.clone(),
        [Path::new("/project_a"), Path::new("/project_b")],
        cx,
    )
    .await;
    cx.run_until_parked();

    let (first_repository, second_repository) = project.read_with(cx, |project, cx| {
        let mut first_repository = None;
        let mut second_repository = None;

        for repository in project.repositories(cx).values() {
            let work_directory_abs_path = &repository.read(cx).work_directory_abs_path;
            if work_directory_abs_path.as_ref() == Path::new("/project_a") {
                first_repository = Some(repository.clone());
            } else if work_directory_abs_path.as_ref() == Path::new("/project_b") {
                second_repository = Some(repository.clone());
            }
        }

        (
            first_repository.expect("should have repository for /project_a"),
            second_repository.expect("should have repository for /project_b"),
        )
    });
    first_repository.update(cx, |repository, cx| repository.set_as_active_repository(cx));
    cx.run_until_parked();

    let (multi_workspace, cx) = cx.add_window_view(|window, cx| {
        workspace::MultiWorkspace::test_new(project.clone(), window, cx)
    });

    let workspace_weak = multi_workspace.read_with(&*cx, |multi, _| multi.workspace().downgrade());
    let git_graph = cx.new_window_entity(|window, cx| {
        GitGraph::new(
            first_repository.read(cx).id,
            project.read(cx).git_store().clone(),
            workspace_weak,
            None,
            window,
            cx,
        )
    });
    cx.run_until_parked();

    // Verify initial graph data is loaded
    let initial_commit_count = git_graph.read_with(&*cx, |graph, _| graph.graph_data.commits.len());
    assert!(
        initial_commit_count > 0,
        "graph data should have been loaded, got 0 commits"
    );

    git_graph.update(cx, |graph, cx| {
        graph.set_repo_id(second_repository.read(cx).id, cx)
    });
    cx.run_until_parked();

    let commit_count_after_clear =
        git_graph.read_with(&*cx, |graph, _| graph.graph_data.commits.len());
    assert_eq!(
        commit_count_after_clear, 0,
        "graph_data should be cleared after switching away"
    );

    git_graph.update(cx, |graph, cx| {
        graph.set_repo_id(first_repository.read(cx).id, cx)
    });
    cx.run_until_parked();

    cx.draw(
        point(px(0.), px(0.)),
        gpui::size(px(1200.), px(800.)),
        |_, _| git_graph.clone().into_any_element(),
    );
    cx.run_until_parked();

    // Verify graph data is reloaded from repository cache on switch back
    let reloaded_commit_count =
        git_graph.read_with(&*cx, |graph, _| graph.graph_data.commits.len());
    assert_eq!(
        reloaded_commit_count,
        commits.len(),
        "graph data should be reloaded after switching back"
    );
}
