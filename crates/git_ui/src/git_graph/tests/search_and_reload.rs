use super::*;

#[gpui::test]
async fn test_git_graph_search_matches_commit_hash_prefix(cx: &mut TestAppContext) {
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

    let first_sha = Oid::from_bytes(&[1; 20]).unwrap();
    let target_sha = Oid::from_bytes(&[2; 20]).unwrap();
    let third_sha = Oid::from_bytes(&[3; 20]).unwrap();
    let commits = vec![
        Arc::new(InitialGraphCommitData {
            sha: first_sha,
            parents: smallvec![target_sha],
            ref_names: vec!["HEAD".into(), "refs/heads/main".into()],
        }),
        Arc::new(InitialGraphCommitData {
            sha: target_sha,
            parents: smallvec![third_sha],
            ref_names: vec![],
        }),
        Arc::new(InitialGraphCommitData {
            sha: third_sha,
            parents: smallvec![],
            ref_names: vec![],
        }),
    ];
    fs.set_graph_commits(Path::new("/project/.git"), commits);
    fs.set_commit_data(
        Path::new("/project/.git"),
        [
            (
                CommitData {
                    sha: first_sha,
                    parents: smallvec![target_sha],
                    author_name: "Author".into(),
                    author_email: "author@example.com".into(),
                    commit_timestamp: 1,
                    subject: "Add feature".into(),
                    message: "Add feature".into(),
                },
                false,
            ),
            (
                CommitData {
                    sha: target_sha,
                    parents: smallvec![third_sha],
                    author_name: "Author".into(),
                    author_email: "author@example.com".into(),
                    commit_timestamp: 2,
                    subject: "Fix branch loading".into(),
                    message: "Fix branch loading".into(),
                },
                false,
            ),
            (
                CommitData {
                    sha: third_sha,
                    parents: smallvec![],
                    author_name: "Author".into(),
                    author_email: "author@example.com".into(),
                    commit_timestamp: 3,
                    subject: "Update docs".into(),
                    message: "Update docs".into(),
                },
                false,
            ),
        ],
    );

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
            workspace_weak,
            None,
            window,
            cx,
        )
    });
    cx.run_until_parked();

    git_graph.update(cx, |graph, cx| {
        graph.search_for_test("0202020".into(), cx);
    });
    cx.run_until_parked();

    git_graph.read_with(&*cx, |graph, _| {
        assert_eq!(graph.search_matches_for_test(), vec![target_sha]);
        let selected_sha = graph
            .selected_entry_idx
            .and_then(|idx| graph.graph_data.commits.get(idx))
            .map(|commit| commit.data.sha);
        assert_eq!(selected_sha, Some(target_sha));
    });

    git_graph.update(cx, |graph, cx| {
        graph.search_for_test("docs".into(), cx);
    });
    cx.run_until_parked();

    git_graph.read_with(&*cx, |graph, _| {
        assert_eq!(graph.search_matches_for_test(), vec![third_sha]);
    });
}

#[gpui::test]
async fn test_graph_data_reloaded_after_stash_change(cx: &mut TestAppContext) {
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

    let initial_head = Oid::from_bytes(&[1; 20]).unwrap();
    let initial_stash = Oid::from_bytes(&[2; 20]).unwrap();
    let updated_head = Oid::from_bytes(&[3; 20]).unwrap();
    let updated_stash = Oid::from_bytes(&[4; 20]).unwrap();

    fs.set_graph_commits(
        Path::new("/project/.git"),
        vec![
            Arc::new(InitialGraphCommitData {
                sha: initial_head,
                parents: smallvec![initial_stash],
                ref_names: vec!["HEAD".into(), "refs/heads/main".into()],
            }),
            Arc::new(InitialGraphCommitData {
                sha: initial_stash,
                parents: smallvec![],
                ref_names: vec!["refs/stash".into()],
            }),
        ],
    );
    fs.with_git_state(Path::new("/project/.git"), true, |state| {
        state.stash_entries = git::stash::GitStash {
            entries: vec![git::stash::StashEntry {
                index: 0,
                oid: initial_stash,
                message: "initial stash".to_string(),
                branch: Some("main".to_string()),
                timestamp: 1,
            }]
            .into(),
        };
    })
    .unwrap();

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
            workspace_weak,
            None,
            window,
            cx,
        )
    });
    cx.run_until_parked();

    let initial_shas = git_graph.read_with(&*cx, |graph, _| {
        graph
            .graph_data
            .commits
            .iter()
            .map(|commit| commit.data.sha)
            .collect::<Vec<_>>()
    });
    assert_eq!(initial_shas, vec![initial_head, initial_stash]);

    fs.set_graph_commits(
        Path::new("/project/.git"),
        vec![
            Arc::new(InitialGraphCommitData {
                sha: updated_head,
                parents: smallvec![updated_stash],
                ref_names: vec!["HEAD".into(), "refs/heads/main".into()],
            }),
            Arc::new(InitialGraphCommitData {
                sha: updated_stash,
                parents: smallvec![],
                ref_names: vec!["refs/stash".into()],
            }),
        ],
    );
    fs.with_git_state(Path::new("/project/.git"), true, |state| {
        state.stash_entries = git::stash::GitStash {
            entries: vec![git::stash::StashEntry {
                index: 0,
                oid: updated_stash,
                message: "updated stash".to_string(),
                branch: Some("main".to_string()),
                timestamp: 1,
            }]
            .into(),
        };
    })
    .unwrap();

    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.run_until_parked();

    cx.draw(
        point(px(0.), px(0.)),
        gpui::size(px(1200.), px(800.)),
        |_, _| git_graph.clone().into_any_element(),
    );
    cx.run_until_parked();

    let reloaded_shas = git_graph.read_with(&*cx, |graph, _| {
        graph
            .graph_data
            .commits
            .iter()
            .map(|commit| commit.data.sha)
            .collect::<Vec<_>>()
    });
    assert_eq!(reloaded_shas, vec![updated_head, updated_stash]);
}

#[gpui::test]
async fn test_git_graph_row_at_position_rounding(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        Path::new("/project"),
        serde_json::json!({
            ".git": {},
            "file.txt": "content",
        }),
    )
    .await;

    let mut rng = StdRng::seed_from_u64(42);
    let commits = generate_random_commit_dag(&mut rng, 10, false);
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
            workspace_weak,
            None,
            window,
            cx,
        )
    });
    cx.run_until_parked();

    git_graph.update_in(cx, |graph, window, cx| {
        assert!(
            graph.graph_data.commits.len() >= 10,
            "graph should load dummy commits"
        );

        let row_height = GitGraph::row_height(window, cx);
        let origin_y = px(100.0);
        graph.graph_canvas_bounds.set(Some(Bounds {
            origin: point(px(0.0), origin_y),
            size: gpui::size(px(100.0), row_height * 50.0),
        }));

        // Scroll down by half a row so the row under a position near the
        // top of the canvas is row 1 rather than row 0.
        let scroll_offset = row_height * 0.75;
        graph.table_interaction_state.update(cx, |state, _| {
            state.set_scroll_offset(point(px(0.0), -scroll_offset))
        });
        let pos_y = origin_y + row_height * 0.5;
        let absolute_calc_row = graph.row_at_position(pos_y, window, cx);

        assert_eq!(
            absolute_calc_row,
            Some(1),
            "Row calculation should yield absolute row exactly"
        );
    });
}
