use super::*;

#[gpui::test(iterations = 10)]
async fn test_item_opening(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.background_executor.clone());
    let root = path!("/rust-analyzer");
    populate_with_test_ra_project(&fs, root).await;
    let project = Project::test(fs.clone(), [Path::new(root)], cx).await;
    project.read_with(cx, |project, _| project.languages().add(rust_lang()));
    let (window, workspace) = add_outline_panel(&project, cx).await;
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let outline_panel = outline_panel(&workspace, cx);
    outline_panel.update_in(cx, |outline_panel, window, cx| {
        outline_panel.set_active(true, window, cx)
    });

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

    let query = "param_names_for_lifetime_elision_hints";
    perform_project_search(&search_view, query, cx);
    search_view.update(cx, |search_view, cx| {
        search_view
            .results_editor()
            .update(cx, |results_editor, cx| {
                assert_eq!(
                    results_editor.display_text(cx).match_indices(query).count(),
                    9
                );
            });
    });
    let all_matches = r#"rust-analyzer/
  crates/
    ide/src/
      inlay_hints/
        fn_lifetime_fn.rs
          search: match config.«param_names_for_lifetime_elision_hints» {
          search: allocated_lifetimes.push(if config.«param_names_for_lifetime_elision_hints» {
          search: Some(it) if config.«param_names_for_lifetime_elision_hints» => {
          search: InlayHintsConfig { «param_names_for_lifetime_elision_hints»: true, ..TEST_CONFIG },
      inlay_hints.rs
        search: pub «param_names_for_lifetime_elision_hints»: bool,
        search: «param_names_for_lifetime_elision_hints»: self
      static_index.rs
        search: «param_names_for_lifetime_elision_hints»: false,
    rust-analyzer/src/
      cli/
        analysis_stats.rs
          search: «param_names_for_lifetime_elision_hints»: true,
      config.rs
        search: «param_names_for_lifetime_elision_hints»: self"#
            .to_string();
    let select_first_in_all_matches = |line_to_select: &str| {
        assert!(
            all_matches.contains(line_to_select),
            "`{line_to_select}` was not found in all matches `{all_matches}`"
        );
        all_matches.replacen(
            line_to_select,
            &format!("{line_to_select}{SELECTED_MARKER}"),
            1,
        )
    };
    let clear_outline_metadata = |input: &str| {
        input
            .replace("search: ", "")
            .replace("«", "")
            .replace("»", "")
    };

    cx.executor()
        .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
    cx.run_until_parked();

    let active_editor = outline_panel.read_with(cx, |outline_panel, _| {
        outline_panel
            .active_editor()
            .expect("should have an active editor open")
    });
    let initial_outline_selection =
        "search: match config.«param_names_for_lifetime_elision_hints» {";
    outline_panel.update_in(cx, |outline_panel, window, cx| {
        assert_eq!(
            display_entries(
                &project,
                &snapshot(outline_panel, cx),
                &outline_panel.cached_entries,
                outline_panel.selected_entry(),
                cx,
            ),
            select_first_in_all_matches(initial_outline_selection)
        );
        assert_eq!(
            selected_row_text(&active_editor, cx),
            clear_outline_metadata(initial_outline_selection),
            "Should place the initial editor selection on the corresponding search result"
        );

        outline_panel.select_next(&SelectNext, window, cx);
        outline_panel.select_next(&SelectNext, window, cx);
    });

    let navigated_outline_selection =
        "search: Some(it) if config.«param_names_for_lifetime_elision_hints» => {";
    outline_panel.update(cx, |outline_panel, cx| {
        assert_eq!(
            display_entries(
                &project,
                &snapshot(outline_panel, cx),
                &outline_panel.cached_entries,
                outline_panel.selected_entry(),
                cx,
            ),
            select_first_in_all_matches(navigated_outline_selection)
        );
    });
    cx.executor()
        .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
    outline_panel.update(cx, |_, cx| {
        assert_eq!(
            selected_row_text(&active_editor, cx),
            clear_outline_metadata(navigated_outline_selection),
            "Should still have the initial caret position after SelectNext calls"
        );
    });

    outline_panel.update_in(cx, |outline_panel, window, cx| {
        outline_panel.open_selected_entry(&OpenSelectedEntry, window, cx);
    });
    outline_panel.update(cx, |_outline_panel, cx| {
        assert_eq!(
            selected_row_text(&active_editor, cx),
            clear_outline_metadata(navigated_outline_selection),
            "After opening, should move the caret to the opened outline entry's position"
        );
    });

    outline_panel.update_in(cx, |outline_panel, window, cx| {
        outline_panel.select_next(&SelectNext, window, cx);
    });
    let next_navigated_outline_selection = "search: InlayHintsConfig { «param_names_for_lifetime_elision_hints»: true, ..TEST_CONFIG },";
    outline_panel.update(cx, |outline_panel, cx| {
        assert_eq!(
            display_entries(
                &project,
                &snapshot(outline_panel, cx),
                &outline_panel.cached_entries,
                outline_panel.selected_entry(),
                cx,
            ),
            select_first_in_all_matches(next_navigated_outline_selection)
        );
    });
    cx.executor()
        .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
    outline_panel.update(cx, |_outline_panel, cx| {
        assert_eq!(
            selected_row_text(&active_editor, cx),
            clear_outline_metadata(next_navigated_outline_selection),
            "Should again preserve the selection after another SelectNext call"
        );
    });

    outline_panel.update_in(cx, |outline_panel, window, cx| {
        outline_panel.open_excerpts(&editor::actions::OpenExcerpts, window, cx);
    });
    cx.executor()
        .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
    cx.run_until_parked();
    let new_active_editor = outline_panel.read_with(cx, |outline_panel, _| {
        outline_panel
            .active_editor()
            .expect("should have an active editor open")
    });
    outline_panel.update(cx, |outline_panel, cx| {
        assert_ne!(
            active_editor, new_active_editor,
            "After opening an excerpt, new editor should be open"
        );
        assert_eq!(
            display_entries(
                &project,
                &snapshot(outline_panel, cx),
                &outline_panel.cached_entries,
                outline_panel.selected_entry(),
                cx,
            ),
            "outline: pub(super) fn hints
outline: fn hints_lifetimes_named  <==== selected"
        );
        assert_eq!(
            selected_row_text(&new_active_editor, cx),
            clear_outline_metadata(next_navigated_outline_selection),
            "When opening the excerpt, should navigate to the place corresponding the outline entry"
        );
    });
}
