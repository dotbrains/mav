use super::*;

#[gpui::test]
async fn test_file_history_action_uses_git_panel_and_editor_sources(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        Path::new(util::path!("/project")),
        json!({
            ".git": {},
            "tracked1.txt": "tracked 1",
            "tracked2.txt": "tracked 2",
        }),
    )
    .await;
    fs.set_status_for_repo(
        Path::new(util::path!("/project/.git")),
        &[
            ("tracked1.txt", StatusCode::Modified.worktree()),
            ("tracked2.txt", StatusCode::Modified.worktree()),
        ],
    );

    let commits = vec![Arc::new(InitialGraphCommitData {
        sha: Oid::from_bytes(&[1; 20]).unwrap(),
        parents: smallvec![],
        ref_names: vec!["HEAD".into(), "refs/heads/main".into()],
    })];
    fs.set_graph_commits(Path::new(util::path!("/project/.git")), commits);

    let project = Project::test(fs.clone(), [Path::new(util::path!("/project"))], cx).await;
    cx.run_until_parked();

    let repository = project.read_with(cx, |project, cx| {
        project
            .active_repository(cx)
            .expect("should have active repository")
    });
    let tracked1_repo_path = RepoPath::new(&"tracked1.txt").unwrap();
    let tracked2_repo_path = RepoPath::new(&"tracked2.txt").unwrap();
    let tracked1 = repository
        .read_with(cx, |repository, cx| {
            repository.repo_path_to_project_path(&tracked1_repo_path, cx)
        })
        .expect("tracked1 should resolve to project path");
    let tracked2 = repository
        .read_with(cx, |repository, cx| {
            repository.repo_path_to_project_path(&tracked2_repo_path, cx)
        })
        .expect("tracked2 should resolve to project path");

    let workspace_window = cx
        .add_window(|window, cx| workspace::MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = workspace_window
        .read_with(cx, |multi, _| multi.workspace().clone())
        .expect("workspace should exist");

    let (weak_workspace, async_window_cx) = workspace_window
        .update(cx, |multi, window, cx| {
            (multi.workspace().downgrade(), window.to_async(cx))
        })
        .expect("window should be available");
    cx.background_executor.allow_parking();
    let git_panel = cx
        .foreground_executor()
        .clone()
        .block_test(crate::git_panel::GitPanel::load(
            weak_workspace,
            async_window_cx,
        ))
        .expect("git panel should load");
    cx.background_executor.forbid_parking();

    workspace_window
        .update(cx, |multi, window, cx| {
            let workspace = multi.workspace();
            workspace.update(cx, |workspace, cx| {
                workspace.add_panel(git_panel.clone(), window, cx);
            });
        })
        .expect("workspace window should be available");
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.run_until_parked();

    workspace_window
        .update(cx, |_, window, cx| {
            git_panel.update(cx, |panel, cx| {
                panel.select_entry_by_path(tracked1.clone(), window, cx);
            });
            git_panel.update(cx, |panel, cx| {
                panel.focus_handle(cx).focus(window, cx);
            });
        })
        .expect("workspace window should be available");
    cx.run_until_parked();
    workspace_window
        .update(cx, |_, window, cx| {
            window.dispatch_action(Box::new(git::FileHistory), cx);
        })
        .expect("workspace window should be available");
    cx.run_until_parked();

    workspace.read_with(cx, |workspace, cx| {
        let graphs = workspace.items_of_type::<GitGraph>(cx).collect::<Vec<_>>();
        assert_eq!(graphs.len(), 1);
        assert_eq!(
            graphs[0].read(cx).log_source,
            LogSource::Path(tracked1_repo_path.clone())
        );
    });

    workspace_window
        .update(cx, |_, window, cx| {
            git_panel.update(cx, |panel, cx| {
                panel.select_entry_by_path(tracked1.clone(), window, cx);
            });
            git_panel.update(cx, |panel, cx| {
                panel.focus_handle(cx).focus(window, cx);
            });
        })
        .expect("workspace window should be available");
    cx.run_until_parked();
    workspace_window
        .update(cx, |_, window, cx| {
            window.dispatch_action(Box::new(git::FileHistory), cx);
        })
        .expect("workspace window should be available");
    cx.run_until_parked();

    workspace.read_with(cx, |workspace, cx| {
        let graphs = workspace.items_of_type::<GitGraph>(cx).collect::<Vec<_>>();
        assert_eq!(graphs.len(), 1);
        assert_eq!(
            graphs[0].read(cx).log_source,
            LogSource::Path(tracked1_repo_path.clone())
        );
    });

    let tracked1_buffer = project
        .update(cx, |project, cx| project.open_buffer(tracked1.clone(), cx))
        .await
        .expect("tracked1 buffer should open");
    let tracked2_buffer = project
        .update(cx, |project, cx| project.open_buffer(tracked2.clone(), cx))
        .await
        .expect("tracked2 buffer should open");
    workspace_window
        .update(cx, |multi, window, cx| {
            let workspace = multi.workspace();
            let multibuffer = cx.new(|cx| {
                let mut multibuffer = editor::MultiBuffer::new(language::Capability::ReadWrite);
                multibuffer.set_excerpts_for_buffer(
                    tracked1_buffer.clone(),
                    [Default::default()..tracked1_buffer.read(cx).max_point()],
                    0,
                    cx,
                );
                multibuffer.set_excerpts_for_buffer(
                    tracked2_buffer.clone(),
                    [Default::default()..tracked2_buffer.read(cx).max_point()],
                    0,
                    cx,
                );
                multibuffer
            });
            let editor = cx
                .new(|cx| Editor::for_multibuffer(multibuffer, Some(project.clone()), window, cx));
            workspace.update(cx, |workspace, cx| {
                workspace.add_item_to_active_pane(Box::new(editor.clone()), None, true, window, cx);
            });
            editor.update(cx, |editor, cx| {
                let snapshot = editor.buffer().read(cx).snapshot(cx);
                let second_excerpt_point = snapshot
                    .range_for_buffer(tracked2_buffer.read(cx).remote_id())
                    .expect("tracked2 excerpt should exist")
                    .start;
                let anchor = snapshot.anchor_before(second_excerpt_point);
                editor.change_selections(
                    editor::SelectionEffects::no_scroll(),
                    window,
                    cx,
                    |selections| {
                        selections.select_anchor_ranges([anchor..anchor]);
                    },
                );
                window.focus(&editor.focus_handle(cx), cx);
            });
        })
        .expect("workspace window should be available");
    cx.run_until_parked();

    workspace_window
        .update(cx, |_, window, cx| {
            window.dispatch_action(Box::new(git::FileHistory), cx);
        })
        .expect("workspace window should be available");
    cx.run_until_parked();

    workspace.read_with(cx, |workspace, cx| {
        let graphs = workspace.items_of_type::<GitGraph>(cx).collect::<Vec<_>>();
        assert_eq!(graphs.len(), 2);
        let latest = graphs
            .into_iter()
            .max_by_key(|graph| graph.entity_id())
            .expect("expected a git graph");
        assert_eq!(
            latest.read(cx).log_source,
            LogSource::Path(tracked2_repo_path)
        );
    });
}
