use super::*;

#[gpui::test]
async fn test_row_height_matches_uniform_list_item_height(cx: &mut TestAppContext) {
    init_test(cx);

    cx.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                *settings.theme = ThemeSettingsContent {
                    ui_font_size: Some(12.7.into()),
                    ..Default::default()
                }
            });
        })
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        Path::new("/project"),
        serde_json::json!({
            ".git": {},
            "file.txt": "content",
        }),
    )
    .await;

    let mut rng = StdRng::seed_from_u64(99);
    let commits = generate_random_commit_dag(&mut rng, 20, false);
    fs.set_graph_commits(Path::new("/project/.git"), commits);

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

    cx.draw(
        point(px(0.), px(0.)),
        gpui::size(px(1200.), px(800.)),
        |_, _| git_graph.clone().into_any_element(),
    );
    cx.run_until_parked();

    git_graph.update_in(cx, |graph, window, cx| {
        let commit_count = graph.graph_data.commits.len();
        assert!(
            commit_count > 0,
            "need at least one commit to measure item height"
        );

        let table_state = graph.table_interaction_state.read(cx);
        let item_size = table_state.scroll_handle.0.borrow().last_item_size.expect(
            "uniform_list should have populated last_item_size after draw(); \
                     the table has not been laid out",
        );

        let measured_item_height = item_size.contents.height / commit_count as f32;
        let computed_row_height = GitGraph::row_height(window, cx);

        assert_eq!(
            computed_row_height, measured_item_height,
            "GitGraph::row_height ({}) must exactly match the height that \
                 uniform_list measured for each table row ({}). \
                 A mismatch means the canvas and table rows will drift when scrolling.",
            computed_row_height, measured_item_height,
        );
    });
}

#[gpui::test]
async fn test_copy_selected_commit_tag_with_one_tag_copies_to_clipboard(cx: &mut TestAppContext) {
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

    let commit_sha = Oid::from_bytes(&[1; 20]).unwrap();
    let commits = vec![Arc::new(InitialGraphCommitData {
        sha: commit_sha,
        parents: smallvec![],
        ref_names: vec![
            SharedString::from("HEAD -> main"),
            SharedString::from("origin/main"),
            SharedString::from("tag: v1.0.0"),
        ],
    })];
    fs.set_graph_commits(Path::new("/project/.git"), commits);

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
    let workspace = multi_workspace.read_with(&*cx, |multi, _| multi.workspace().clone());
    let workspace_weak = workspace.downgrade();

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
        assert_eq!(graph.graph_data.commits.len(), 1);
        graph.selected_entry_idx = Some(0);
        graph.copy_selected_commit_tag(&CopyCommitTag, window, cx);
    });

    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()),
        Some("v1.0.0".to_string())
    );
}

#[gpui::test]
async fn test_copy_selected_commit_tag_with_multiple_tags_opens_picker_and_copies_selected_tag(
    cx: &mut TestAppContext,
) {
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

    let commit_sha = Oid::from_bytes(&[1; 20]).unwrap();
    let commits = vec![Arc::new(InitialGraphCommitData {
        sha: commit_sha,
        parents: smallvec![],
        ref_names: vec![
            SharedString::from("HEAD -> main"),
            SharedString::from("origin/main"),
            SharedString::from("tag: v1.0.0"),
            SharedString::from("tag: v1.1.0"),
        ],
    })];
    fs.set_graph_commits(Path::new("/project/.git"), commits);

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
    let workspace = multi_workspace.read_with(&*cx, |multi, _| multi.workspace().clone());
    let workspace_weak = workspace.downgrade();

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
        assert_eq!(graph.graph_data.commits.len(), 1);
        graph.selected_entry_idx = Some(0);
        graph.copy_selected_commit_tag(&CopyCommitTag, window, cx);
    });

    // Ensure that nothing has been copied at this point
    assert_eq!(cx.read_from_clipboard().and_then(|item| item.text()), None);

    let picker = workspace.update(cx, |workspace, cx| {
        workspace
            .active_modal::<CommitTagPicker>(cx)
            .expect("commit tag picker is not open")
            .read(cx)
            .picker
            .clone()
    });

    picker.read_with(cx, |picker, _| {
        assert_eq!(picker.delegate.selected_index, 0);
        assert_eq!(
            picker.delegate.tag_names,
            [SharedString::from("v1.0.0"), SharedString::from("v1.1.0")]
        );
    });

    cx.dispatch_action(menu::Confirm);
    cx.run_until_parked();

    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()),
        Some("v1.0.0".to_string())
    );
}

#[gpui::test]
async fn test_git_graph_navigation(cx: &mut TestAppContext) {
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
    fs.set_graph_commits(Path::new("/project/.git"), commits);

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

    let workspace = multi_workspace.read_with(&*cx, |multi, _| multi.workspace().clone());
    let workspace_weak = workspace.downgrade();

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

    workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item_to_active_pane(Box::new(git_graph.clone()), None, true, window, cx);
    });
    cx.run_until_parked();

    git_graph.update_in(cx, |graph, window, cx| {
        graph.focus_handle(cx).focus(window, cx);
    });
    cx.run_until_parked();

    cx.draw(
        point(px(0.), px(0.)),
        gpui::size(px(1200.), px(800.)),
        |_, _| multi_workspace.clone().into_any_element(),
    );
    cx.run_until_parked();

    git_graph.update_in(cx, |graph, window, cx| {
        graph.focus_handle(cx).focus(window, cx);
    });
    cx.run_until_parked();

    git_graph.read_with(&*cx, |graph, _| {
        assert_eq!(graph.graph_data.commits.len(), 10);
    });
    git_graph.read_with(&*cx, |graph, _| {
        assert_eq!(graph.selected_entry_idx, None);
    });

    git_graph.update_in(cx, |graph, window, cx| {
        graph.select_first(&menu::SelectFirst, window, cx);
    });
    cx.run_until_parked();
    git_graph.read_with(&*cx, |graph, _| {
        assert_eq!(graph.selected_entry_idx, Some(0));
    });

    let scroll_step = git_graph.update_in(cx, |graph, window, cx| {
        (graph.visible_row_count(window, cx) / 2).max(1)
    });

    cx.dispatch_action(ScrollDown);
    cx.run_until_parked();
    git_graph.read_with(&*cx, |graph, _| {
        assert_eq!(graph.selected_entry_idx, Some(scroll_step));
    });

    cx.dispatch_action(ScrollUp);
    cx.run_until_parked();
    git_graph.read_with(&*cx, |graph, _| {
        assert_eq!(graph.selected_entry_idx, Some(0));
    });

    git_graph.update_in(cx, |graph, window, cx| {
        graph.select_next(&menu::SelectNext, window, cx);
    });
    cx.run_until_parked();
    git_graph.read_with(&*cx, |graph, _| {
        assert_eq!(graph.selected_entry_idx, Some(1));
    });

    git_graph.update_in(cx, |graph, window, cx| {
        graph.select_prev(&menu::SelectPrevious, window, cx);
    });
    cx.run_until_parked();
    git_graph.read_with(&*cx, |graph, _| {
        assert_eq!(graph.selected_entry_idx, Some(0));
    });

    git_graph.update_in(cx, |graph, window, cx| {
        graph.select_last(&menu::SelectLast, window, cx);
    });
    cx.run_until_parked();
    git_graph.read_with(&*cx, |graph, _| {
        assert_eq!(graph.selected_entry_idx, Some(9));
    });

    cx.dispatch_action(ScrollDown);
    cx.run_until_parked();
    git_graph.read_with(&*cx, |graph, _| {
        assert_eq!(graph.selected_entry_idx, Some(9));
    });

    git_graph.update_in(cx, |graph, window, cx| {
        graph.select_next(&menu::SelectNext, window, cx);
    });
    cx.run_until_parked();
    git_graph.read_with(&*cx, |graph, _| {
        assert_eq!(graph.selected_entry_idx, Some(9));
    });

    git_graph.update_in(cx, |graph, window, cx| {
        graph.select_prev(&menu::SelectPrevious, window, cx);
    });
    cx.run_until_parked();
    git_graph.read_with(&*cx, |graph, _| {
        assert_eq!(graph.selected_entry_idx, Some(8));
    });

    git_graph.update(cx, |graph, cx| {
        graph.selected_entry_idx = None;
        cx.notify();
    });
    cx.run_until_parked();
    git_graph.update_in(cx, |graph, window, cx| {
        graph.select_prev(&menu::SelectPrevious, window, cx);
    });
    cx.run_until_parked();
    git_graph.read_with(&*cx, |graph, _| {
        assert_eq!(graph.selected_entry_idx, Some(0));
    });

    git_graph.update(cx, |graph, cx| {
        graph.selected_entry_idx = None;
        cx.notify();
    });
    cx.run_until_parked();
    git_graph.update_in(cx, |graph, window, cx| {
        graph.select_next(&menu::SelectNext, window, cx);
    });
    cx.run_until_parked();
    git_graph.read_with(&*cx, |graph, _| {
        assert_eq!(graph.selected_entry_idx, Some(0));
    });
}
