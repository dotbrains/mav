use super::*;

#[gpui::test(iterations = 10)]
async fn test_frontend_repo_structure(cx: &mut TestAppContext) {
    init_test(cx);

    let root = path!("/frontend-project");
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        root,
        json!({
            "public": {
                "lottie": {
                    "syntax-tree.json": r#"{ "something": "static" }"#
                }
            },
            "src": {
                "app": {
                    "(site)": {
                        "(about)": {
                            "jobs": {
                                "[slug]": {
                                    "page.tsx": r#"static"#
                                }
                            }
                        },
                        "(blog)": {
                            "post": {
                                "[slug]": {
                                    "page.tsx": r#"static"#
                                }
                            }
                        },
                    }
                },
                "components": {
                    "ErrorBoundary.tsx": r#"static"#,
                }
            }

        }),
    )
    .await;
    let project = Project::test(fs.clone(), [Path::new(root)], cx).await;
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

    let query = "static";
    perform_project_search(&search_view, query, cx);
    search_view.update(cx, |search_view, cx| {
        search_view
            .results_editor()
            .update(cx, |results_editor, cx| {
                assert_eq!(
                    results_editor.display_text(cx).match_indices(query).count(),
                    4
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
                r#"frontend-project/
  public/lottie/
syntax-tree.json
  search: {{ "something": "«static»" }}  <==== selected
  src/
app/(site)/
  (about)/jobs/[slug]/
    page.tsx
      search: «static»
  (blog)/post/[slug]/
    page.tsx
      search: «static»
components/
  ErrorBoundary.tsx
    search: «static»"#
            )
        );
    });

    outline_panel.update_in(cx, |outline_panel, window, cx| {
        // Move to 5th element in the list, 3 items down.
        for _ in 0..2 {
            outline_panel.select_next(&SelectNext, window, cx);
        }
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
                r#"frontend-project/
  public/lottie/
syntax-tree.json
  search: {{ "something": "«static»" }}
  src/
app/(site)/  <==== selected
components/
  ErrorBoundary.tsx
    search: «static»"#
            )
        );
    });

    outline_panel.update_in(cx, |outline_panel, window, cx| {
        // Move to the next visible non-FS entry
        for _ in 0..3 {
            outline_panel.select_next(&SelectNext, window, cx);
        }
    });
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
                r#"frontend-project/
  public/lottie/
syntax-tree.json
  search: {{ "something": "«static»" }}
  src/
app/(site)/
components/
  ErrorBoundary.tsx
    search: «static»  <==== selected"#
            )
        );
    });

    outline_panel.update_in(cx, |outline_panel, window, cx| {
        outline_panel
            .active_editor()
            .expect("Should have an active editor")
            .update(cx, |editor, cx| {
                editor.toggle_fold(&editor::actions::ToggleFold, window, cx)
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
                r#"frontend-project/
  public/lottie/
syntax-tree.json
  search: {{ "something": "«static»" }}
  src/
app/(site)/
components/
  ErrorBoundary.tsx  <==== selected"#
            )
        );
    });

    outline_panel.update_in(cx, |outline_panel, window, cx| {
        outline_panel
            .active_editor()
            .expect("Should have an active editor")
            .update(cx, |editor, cx| {
                editor.toggle_fold(&editor::actions::ToggleFold, window, cx)
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
                r#"frontend-project/
  public/lottie/
syntax-tree.json
  search: {{ "something": "«static»" }}
  src/
app/(site)/
components/
  ErrorBoundary.tsx  <==== selected
    search: «static»"#
            )
        );
    });

    outline_panel.update_in(cx, |outline_panel, window, cx| {
        outline_panel.collapse_all_entries(&CollapseAllEntries, window, cx);
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
            format!(r#"frontend-project/"#)
        );
    });

    outline_panel.update_in(cx, |outline_panel, window, cx| {
        outline_panel.expand_all_entries(&ExpandAllEntries, window, cx);
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
                r#"frontend-project/
  public/lottie/
syntax-tree.json
  search: {{ "something": "«static»" }}
  src/
app/(site)/
  (about)/jobs/[slug]/
    page.tsx
      search: «static»
  (blog)/post/[slug]/
    page.tsx
      search: «static»
components/
  ErrorBoundary.tsx  <==== selected
    search: «static»"#
            )
        );
    });
}
