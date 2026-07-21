use super::*;

#[gpui::test]
fn test_serialized_state_roundtrip(_cx: &mut TestAppContext) {
    use persistence::SerializedGitGraphState;

    let path = RepoPath::new(&"src/main.rs").unwrap();
    let sha = Oid::from_bytes(&[0xab; 20]).unwrap();

    let state = SerializedGitGraphState {
        log_source_type: Some(persistence::LOG_SOURCE_PATH),
        log_source_value: Some("src/main.rs".to_string()),
        log_order: Some(persistence::LOG_ORDER_TOPO),
        selected_sha: Some(sha.to_string()),
        search_query: Some("fix bug".to_string()),
        search_case_sensitive: Some(true),
    };

    assert_eq!(
        persistence::deserialize_log_source(&state),
        LogSource::Path(path)
    );
    assert!(matches!(
        persistence::deserialize_log_order(&state),
        LogOrder::TopoOrder
    ));
    assert_eq!(
        state.selected_sha.as_deref(),
        Some(sha.to_string()).as_deref()
    );
    assert_eq!(state.search_query.as_deref(), Some("fix bug"));
    assert_eq!(state.search_case_sensitive, Some(true));

    let all_state = SerializedGitGraphState {
        log_source_type: Some(persistence::LOG_SOURCE_ALL),
        log_source_value: None,
        log_order: Some(persistence::LOG_ORDER_DATE),
        selected_sha: None,
        search_query: None,
        search_case_sensitive: None,
    };
    assert_eq!(
        persistence::deserialize_log_source(&all_state),
        LogSource::All
    );
    assert!(matches!(
        persistence::deserialize_log_order(&all_state),
        LogOrder::DateOrder
    ));

    let branch_state = SerializedGitGraphState {
        log_source_type: Some(persistence::LOG_SOURCE_BRANCH),
        log_source_value: Some("refs/heads/main".to_string()),
        ..Default::default()
    };
    assert_eq!(
        persistence::deserialize_log_source(&branch_state),
        LogSource::Branch("refs/heads/main".into())
    );

    let sha_state = SerializedGitGraphState {
        log_source_type: Some(persistence::LOG_SOURCE_SHA),
        log_source_value: Some(sha.to_string()),
        ..Default::default()
    };
    assert_eq!(
        persistence::deserialize_log_source(&sha_state),
        LogSource::Sha(sha)
    );

    let empty_state = SerializedGitGraphState::default();
    assert_eq!(
        persistence::deserialize_log_source(&empty_state),
        LogSource::All
    );
    assert!(matches!(
        persistence::deserialize_log_order(&empty_state),
        LogOrder::DateOrder
    ));
}

#[gpui::test]
async fn test_git_graph_state_persists_across_serialization_roundtrip(cx: &mut TestAppContext) {
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

    let mut rng = StdRng::seed_from_u64(99);
    let commits = generate_random_commit_dag(&mut rng, 20, false);
    fs.set_graph_commits(Path::new("/project/.git"), commits.clone());

    let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;
    cx.run_until_parked();

    let repository = project.read_with(cx, |project, cx| {
        project
            .active_repository(cx)
            .expect("should have a repository")
    });

    let (multi_workspace, cx) = cx.add_window_view(|window, cx| {
        workspace::MultiWorkspace::test_new(project.clone(), window, cx)
    });
    let workspace_weak = multi_workspace.read_with(&*cx, |multi, _| multi.workspace().downgrade());

    let git_graph = cx.new_window_entity(|window, cx| {
        GitGraph::new(
            repository.read(cx).id,
            project.read(cx).git_store().clone(),
            workspace_weak.clone(),
            None,
            window,
            cx,
        )
    });
    cx.run_until_parked();

    cx.draw(
        point(px(0.), px(0.)),
        gpui::size(px(1200.), px(800.)),
        |_, _| git_graph.clone().into_any_element(),
    );
    cx.run_until_parked();

    let commit_count = git_graph.read_with(&*cx, |graph, _| graph.graph_data.commits.len());
    assert!(commit_count > 0, "graph should have loaded commits, got 0");

    let target_sha = commits[5].sha;
    git_graph.update(cx, |graph, _| {
        graph.selected_entry_idx = Some(5);
    });

    let selected_sha = git_graph.read_with(&*cx, |graph, _| {
        graph
            .selected_entry_idx
            .and_then(|idx| graph.graph_data.commits.get(idx))
            .map(|c| c.data.sha.to_string())
    });
    assert_eq!(selected_sha, Some(target_sha.to_string()));

    let item_id = workspace::ItemId::from(999_u64);
    let workspace_db = cx.read(|cx| workspace::WorkspaceDb::global(cx));
    let workspace_id = workspace_db
        .next_id()
        .await
        .expect("should create workspace id");
    let db = cx.read(|cx| persistence::GitGraphsDb::global(cx));
    db.save_git_graph(
        item_id,
        workspace_id,
        "/project".to_string(),
        Some(persistence::LOG_SOURCE_ALL),
        None,
        Some(persistence::LOG_ORDER_DATE),
        selected_sha.clone(),
        Some("some query".to_string()),
        Some(true),
    )
    .await
    .expect("save should succeed");

    let restored_graph = cx
        .update(|window, cx| {
            <GitGraph as workspace::SerializableItem>::deserialize(
                project.clone(),
                workspace_weak,
                workspace_id,
                item_id,
                window,
                cx,
            )
        })
        .await
        .expect("deserialization should succeed");
    cx.run_until_parked();

    cx.draw(
        point(px(0.), px(0.)),
        gpui::size(px(1200.), px(800.)),
        |_, _| restored_graph.clone().into_any_element(),
    );
    cx.run_until_parked();

    let restored_commit_count =
        restored_graph.read_with(&*cx, |graph, _| graph.graph_data.commits.len());
    assert_eq!(
        restored_commit_count, commit_count,
        "restored graph should have the same number of commits"
    );

    restored_graph.read_with(&*cx, |graph, _| {
        assert_eq!(
            graph.log_source,
            LogSource::All,
            "log_source should be restored"
        );

        let restored_selected_sha = graph
            .selected_entry_idx
            .and_then(|idx| graph.graph_data.commits.get(idx))
            .map(|c| c.data.sha.to_string());
        assert_eq!(
            restored_selected_sha, selected_sha,
            "selected commit should be restored via pending_select_sha"
        );

        assert_eq!(
            graph.search_state.case_sensitive, true,
            "search case sensitivity should be restored"
        );
    });

    restored_graph.read_with(&*cx, |graph, cx| {
        let editor_text = graph.search_state.editor.read(cx).text(cx);
        assert_eq!(
            editor_text, "some query",
            "search query text should be restored in editor"
        );
    });
}
