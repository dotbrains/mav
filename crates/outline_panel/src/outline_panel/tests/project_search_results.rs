use super::*;

#[gpui::test(iterations = 10)]
async fn test_project_search_results_toggling(cx: &mut TestAppContext) {
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
            select_first_in_all_matches(
                "search: match config.«param_names_for_lifetime_elision_hints» {"
            )
        );
    });

    outline_panel.update_in(cx, |outline_panel, window, cx| {
        outline_panel.select_parent(&SelectParent, window, cx);
        assert_eq!(
            display_entries(
                &project,
                &snapshot(outline_panel, cx),
                &outline_panel.cached_entries,
                outline_panel.selected_entry(),
                cx,
            ),
            select_first_in_all_matches("fn_lifetime_fn.rs")
        );
    });
    outline_panel.update_in(cx, |outline_panel, window, cx| {
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
                r#"rust-analyzer/
  crates/
    ide/src/
      inlay_hints/
        fn_lifetime_fn.rs{SELECTED_MARKER}
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
        search: «param_names_for_lifetime_elision_hints»: self"#,
            )
        );
    });

    outline_panel.update_in(cx, |outline_panel, window, cx| {
        outline_panel.expand_all_entries(&ExpandAllEntries, window, cx);
    });
    cx.executor()
        .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
    cx.run_until_parked();
    outline_panel.update_in(cx, |outline_panel, window, cx| {
        outline_panel.select_parent(&SelectParent, window, cx);
        assert_eq!(
            display_entries(
                &project,
                &snapshot(outline_panel, cx),
                &outline_panel.cached_entries,
                outline_panel.selected_entry(),
                cx,
            ),
            select_first_in_all_matches("inlay_hints/")
        );
    });

    outline_panel.update_in(cx, |outline_panel, window, cx| {
        outline_panel.select_parent(&SelectParent, window, cx);
        assert_eq!(
            display_entries(
                &project,
                &snapshot(outline_panel, cx),
                &outline_panel.cached_entries,
                outline_panel.selected_entry(),
                cx,
            ),
            select_first_in_all_matches("ide/src/")
        );
    });

    outline_panel.update_in(cx, |outline_panel, window, cx| {
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
                r#"rust-analyzer/
  crates/
    ide/src/{SELECTED_MARKER}
    rust-analyzer/src/
      cli/
        analysis_stats.rs
          search: «param_names_for_lifetime_elision_hints»: true,
      config.rs
        search: «param_names_for_lifetime_elision_hints»: self"#,
            )
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
            select_first_in_all_matches("ide/src/")
        );
    });
}
