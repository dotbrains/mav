use super::*;

#[gpui::test]
async fn test_visible_list(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root1",
        json!({
            ".dockerignore": "",
            ".git": {
                "HEAD": "",
            },
            "a": {
                "0": { "q": "", "r": "", "s": "" },
                "1": { "t": "", "u": "" },
                "2": { "v": "", "w": "", "x": "", "y": "" },
            },
            "b": {
                "3": { "Q": "" },
                "4": { "R": "", "S": "", "T": "", "U": "" },
            },
            "C": {
                "5": {},
                "6": { "V": "", "W": "" },
                "7": { "X": "" },
                "8": { "Y": {}, "Z": "" }
            }
        }),
    )
    .await;
    fs.insert_tree(
        "/root2",
        json!({
            "d": {
                "9": ""
            },
            "e": {}
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/root1".as_ref(), "/root2".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..50, cx),
        &[
            "v root1",
            "    > .git",
            "    > a",
            "    > b",
            "    > C",
            "      .dockerignore",
            "v root2",
            "    > d",
            "    > e",
        ]
    );

    toggle_expand_dir(&panel, "root1/b", cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..50, cx),
        &[
            "v root1",
            "    > .git",
            "    > a",
            "    v b  <== selected",
            "        > 3",
            "        > 4",
            "    > C",
            "      .dockerignore",
            "v root2",
            "    > d",
            "    > e",
        ]
    );

    assert_eq!(
        visible_entries_as_strings(&panel, 6..9, cx),
        &[
            //
            "    > C",
            "      .dockerignore",
            "v root2",
        ]
    );
}

#[gpui::test]
async fn test_opening_file(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/src"),
        json!({
            "test": {
                "first.rs": "// First Rust file",
                "second.rs": "// Second Rust file",
                "third.rs": "// Third Rust file",
            }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/src").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    toggle_expand_dir(&panel, "src/test", cx);
    select_path(&panel, "src/test/first.rs", cx);
    panel.update_in(cx, |panel, window, cx| panel.open(&Open, window, cx));
    cx.executor().run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v src",
            "    v test",
            "          first.rs  <== selected  <== marked",
            "          second.rs",
            "          third.rs"
        ]
    );
    ensure_single_file_is_opened(&workspace, "test/first.rs", cx);

    select_path(&panel, "src/test/second.rs", cx);
    panel.update_in(cx, |panel, window, cx| panel.open(&Open, window, cx));
    cx.executor().run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v src",
            "    v test",
            "          first.rs",
            "          second.rs  <== selected  <== marked",
            "          third.rs"
        ]
    );
    ensure_single_file_is_opened(&workspace, "test/second.rs", cx);
}

#[gpui::test]
async fn test_file_history_action_uses_focused_project_panel_selection(
    cx: &mut gpui::TestAppContext,
) {
    init_test_with_git_ui(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        Path::new("/project"),
        json!({
            ".git": {},
            "tracked1.txt": "tracked 1",
            "tracked2.txt": "tracked 2",
        }),
    )
    .await;

    let commits = vec![Arc::new(InitialGraphCommitData {
        sha: Oid::from_bytes(&[1; 20]).unwrap(),
        parents: smallvec![],
        ref_names: vec!["HEAD".into(), "refs/heads/main".into()],
    })];
    fs.set_graph_commits(Path::new("/project/.git"), commits);

    let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;
    cx.run_until_parked();

    let repository = project.read_with(cx, |project, cx| {
        project
            .active_repository(cx)
            .expect("should have active repository")
    });
    let project_panel_repo_path = RepoPath::new(&"tracked1.txt").unwrap();
    let editor_repo_path = RepoPath::new(&"tracked2.txt").unwrap();
    let project_panel_path = repository
        .read_with(cx, |repository, cx| {
            repository.repo_path_to_project_path(&project_panel_repo_path, cx)
        })
        .expect("project panel path should resolve");
    let editor_path = repository
        .read_with(cx, |repository, cx| {
            repository.repo_path_to_project_path(&editor_repo_path, cx)
        })
        .expect("editor path should resolve");

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(&*cx, |multi_workspace, _| {
        multi_workspace.workspace().clone()
    });
    let project_panel = multi_workspace.update_in(cx, |multi_workspace, window, cx| {
        let workspace = multi_workspace.workspace();
        workspace.update(cx, |workspace, cx| {
            let project_panel = ProjectPanel::new(workspace, window, cx);
            workspace.add_panel(project_panel.clone(), window, cx);
            project_panel
        })
    });
    cx.run_until_parked();

    let editor_buffer = project
        .update(cx, |project, cx| {
            project.open_buffer(editor_path.clone(), cx)
        })
        .await
        .expect("editor buffer should open");
    multi_workspace.update_in(cx, |multi_workspace, window, cx| {
        let workspace = multi_workspace.workspace();
        let multibuffer = cx.new(|cx| {
            let mut multibuffer = editor::MultiBuffer::new(language::Capability::ReadWrite);
            multibuffer.set_excerpts_for_buffer(
                editor_buffer.clone(),
                [Default::default()..editor_buffer.read(cx).max_point()],
                0,
                cx,
            );
            multibuffer
        });
        let editor =
            cx.new(|cx| Editor::for_multibuffer(multibuffer, Some(project.clone()), window, cx));
        workspace.update(cx, |workspace, cx| {
            workspace.add_item_to_active_pane(Box::new(editor.clone()), None, true, window, cx);
        });
        editor.update(cx, |editor, cx| {
            window.focus(&editor.focus_handle(cx), cx);
        });
    });
    cx.run_until_parked();

    multi_workspace.update_in(cx, |_multi_workspace, window, cx| {
        project_panel.update(cx, |panel, cx| {
            panel.select_path_for_test(project_panel_path.clone(), cx);
        });
        project_panel.update(cx, |panel, cx| {
            panel.focus_handle(cx).focus(window, cx);
        });
    });
    cx.run_until_parked();

    cx.update(|window, cx| {
        window.dispatch_action(Box::new(git::FileHistory), cx);
    });
    cx.run_until_parked();

    workspace.read_with(&*cx, |workspace, cx| {
        let graphs = workspace
            .items_of_type::<git_ui::git_graph::GitGraph>(cx)
            .collect::<Vec<_>>();
        assert_eq!(graphs.len(), 1);
        assert_eq!(
            graphs[0].read(cx).log_source_for_test(),
            &LogSource::Path(project_panel_repo_path)
        );
    });
}

#[gpui::test]
async fn test_file_history_action_does_not_fall_back_to_editor_when_focused_project_panel_selection_has_no_git_repo(
    cx: &mut gpui::TestAppContext,
) {
    init_test_with_git_ui(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        Path::new("/git-project"),
        json!({
            ".git": {},
            "tracked.txt": "tracked",
        }),
    )
    .await;
    fs.insert_tree(
        Path::new("/plain-project"),
        json!({
            "plain.txt": "plain",
        }),
    )
    .await;

    fs.set_graph_commits(
        Path::new("/git-project/.git"),
        vec![Arc::new(InitialGraphCommitData {
            sha: Oid::from_bytes(&[1; 20]).unwrap(),
            parents: smallvec![],
            ref_names: vec!["HEAD".into(), "refs/heads/main".into()],
        })],
    );

    let project = Project::test(
        fs.clone(),
        [Path::new("/git-project"), Path::new("/plain-project")],
        cx,
    )
    .await;
    cx.run_until_parked();

    let repository = project.read_with(cx, |project, cx| {
        project
            .active_repository(cx)
            .expect("should have active repository")
    });
    let editor_repo_path = RepoPath::new(&"tracked.txt").unwrap();
    let editor_path = repository
        .read_with(cx, |repository, cx| {
            repository.repo_path_to_project_path(&editor_repo_path, cx)
        })
        .expect("editor path should resolve");
    let plain_worktree_id = project.read_with(cx, |project, cx| {
        project
            .worktree_for_root_name("plain-project", cx)
            .expect("plain worktree should exist")
            .read(cx)
            .id()
    });
    let plain_project_path = ProjectPath {
        worktree_id: plain_worktree_id,
        path: rel_path("plain.txt").into(),
    };

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(&*cx, |multi_workspace, _| {
        multi_workspace.workspace().clone()
    });
    let project_panel = multi_workspace.update_in(cx, |multi_workspace, window, cx| {
        let workspace = multi_workspace.workspace();
        workspace.update(cx, |workspace, cx| {
            let project_panel = ProjectPanel::new(workspace, window, cx);
            workspace.add_panel(project_panel.clone(), window, cx);
            project_panel
        })
    });
    cx.run_until_parked();

    let editor_buffer = project
        .update(cx, |project, cx| {
            project.open_buffer(editor_path.clone(), cx)
        })
        .await
        .expect("editor buffer should open");
    multi_workspace.update_in(cx, |multi_workspace, window, cx| {
        let workspace = multi_workspace.workspace();
        let multibuffer = cx.new(|cx| {
            let mut multibuffer = editor::MultiBuffer::new(language::Capability::ReadWrite);
            multibuffer.set_excerpts_for_buffer(
                editor_buffer.clone(),
                [Default::default()..editor_buffer.read(cx).max_point()],
                0,
                cx,
            );
            multibuffer
        });
        let editor =
            cx.new(|cx| Editor::for_multibuffer(multibuffer, Some(project.clone()), window, cx));
        workspace.update(cx, |workspace, cx| {
            workspace.add_item_to_active_pane(Box::new(editor.clone()), None, true, window, cx);
        });
        editor.update(cx, |editor, cx| {
            window.focus(&editor.focus_handle(cx), cx);
        });
    });
    cx.run_until_parked();

    multi_workspace.update_in(cx, |_multi_workspace, window, cx| {
        project_panel.update(cx, |panel, cx| {
            panel.select_path_for_test(plain_project_path.clone(), cx);
        });
        project_panel.update(cx, |panel, cx| {
            panel.focus_handle(cx).focus(window, cx);
        });
    });
    cx.run_until_parked();

    cx.update(|window, cx| {
        window.dispatch_action(Box::new(git::FileHistory), cx);
    });
    cx.run_until_parked();

    workspace.read_with(&*cx, |workspace, cx| {
        assert_eq!(
            workspace
                .items_of_type::<git_ui::git_graph::GitGraph>(cx)
                .count(),
            0
        );
    });
}
