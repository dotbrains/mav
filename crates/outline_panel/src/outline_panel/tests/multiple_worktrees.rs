use super::*;

#[gpui::test]
async fn test_multiple_worktrees(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/root"),
        json!({
            "one": {
                "a.txt": "aaa aaa"
            },
            "two": {
                "b.txt": "a aaa"
            }

        }),
    )
    .await;
    let project = Project::test(fs.clone(), [Path::new(path!("/root/one"))], cx).await;
    let (window, workspace) = add_outline_panel(&project, cx).await;
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let outline_panel = outline_panel(&workspace, cx);
    outline_panel.update_in(cx, |outline_panel, window, cx| {
        outline_panel.set_active(true, window, cx)
    });

    let items = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_paths(
                vec![PathBuf::from(path!("/root/two"))],
                OpenOptions {
                    visible: Some(OpenVisible::OnlyDirectories),
                    ..Default::default()
                },
                None,
                window,
                cx,
            )
        })
        .await;
    assert_eq!(items.len(), 1, "Were opening another worktree directory");
    assert!(
        items[0].is_none(),
        "Directory should be opened successfully"
    );

    workspace.update_in(cx, |workspace, window, cx| {
        ProjectSearchView::deploy_search(workspace, &workspace::DeploySearch::default(), window, cx)
    });
    let search_view = workspace.update_in(cx, |workspace, _window, cx| {
        workspace
            .active_pane()
            .read(cx)
            .items()
            .find_map(|item| item.downcast::<ProjectSearchView>())
            .expect("Project search view expected to appear after new search event trigger")
    });

    let query = "aaa";
    perform_project_search(&search_view, query, cx);
    search_view.update(cx, |search_view, cx| {
        search_view
            .results_editor()
            .update(cx, |results_editor, cx| {
                assert_eq!(
                    results_editor.display_text(cx).match_indices(query).count(),
                    3
                );
            });
    });

    cx.executor()
        .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
    cx.run_until_parked();
    outline_panel.update(cx, |outline_panel, cx| {
        assert_eq!(
            display_entries(
                &project,
                &snapshot(outline_panel, cx),
                &outline_panel.cached_entries,
                outline_panel.selected_entry(),
                cx,
            ),
            format!(
                r#"one/
  a.txt
search: «aaa» aaa  <==== selected
search: aaa «aaa»
two/
  b.txt
search: a «aaa»"#,
            ),
        );
    });

    outline_panel.update_in(cx, |outline_panel, window, cx| {
        outline_panel.select_previous(&SelectPrevious, window, cx);
        outline_panel.collapse_selected_entry(&CollapseSelectedEntry, window, cx);
    });
    cx.executor()
        .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
    cx.run_until_parked();
    outline_panel.update(cx, |outline_panel, cx| {
        assert_eq!(
            display_entries(
                &project,
                &snapshot(outline_panel, cx),
                &outline_panel.cached_entries,
                outline_panel.selected_entry(),
                cx,
            ),
            format!(
                r#"one/
  a.txt  <==== selected
two/
  b.txt
search: a «aaa»"#,
            ),
        );
    });

    outline_panel.update_in(cx, |outline_panel, window, cx| {
        outline_panel.select_next(&SelectNext, window, cx);
        outline_panel.collapse_selected_entry(&CollapseSelectedEntry, window, cx);
    });
    cx.executor()
        .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
    cx.run_until_parked();
    outline_panel.update(cx, |outline_panel, cx| {
        assert_eq!(
            display_entries(
                &project,
                &snapshot(outline_panel, cx),
                &outline_panel.cached_entries,
                outline_panel.selected_entry(),
                cx,
            ),
            format!(
                r#"one/
  a.txt
two/  <==== selected"#,
            ),
        );
    });

    outline_panel.update_in(cx, |outline_panel, window, cx| {
        outline_panel.expand_selected_entry(&ExpandSelectedEntry, window, cx);
    });
    cx.executor()
        .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
    cx.run_until_parked();
    outline_panel.update(cx, |outline_panel, cx| {
        assert_eq!(
            display_entries(
                &project,
                &snapshot(outline_panel, cx),
                &outline_panel.cached_entries,
                outline_panel.selected_entry(),
                cx,
            ),
            format!(
                r#"one/
  a.txt
two/  <==== selected
  b.txt
search: a «aaa»"#,
            )
        );
    });
}
