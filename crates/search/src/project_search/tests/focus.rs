use super::*;

#[gpui::test]
async fn test_collapse_state_syncs_after_manual_buffer_fold(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "one.rs": "const ONE: usize = 1;",
            "two.rs": "const TWO: usize = one::ONE + one::ONE;",
            "three.rs": "const THREE: usize = one::ONE + two::TWO;",
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let search = cx.new(|cx| ProjectSearch::new(project.clone(), cx));
    let search_view = cx.add_window(|window, cx| {
        ProjectSearchView::new(workspace.downgrade(), search.clone(), window, cx, None)
    });

    // Search for "ONE" which appears in all 3 files
    perform_search(search_view, "ONE", cx);

    // Verify initial state: no folds
    let has_any_folded = search_view
        .read_with(cx, |search_view, cx| {
            search_view
                .results_editor
                .read(cx)
                .has_any_buffer_folded(cx)
        })
        .expect("should read state");
    assert!(!has_any_folded, "No buffers should be folded initially");

    // Fold all via fold_all
    search_view
        .update(cx, |search_view, window, cx| {
            search_view.results_editor.update(cx, |editor, cx| {
                editor.fold_all(&FoldAll, window, cx);
            })
        })
        .expect("Should fold fine");
    cx.run_until_parked();

    let has_any_folded = search_view
        .read_with(cx, |search_view, cx| {
            search_view
                .results_editor
                .read(cx)
                .has_any_buffer_folded(cx)
        })
        .expect("should read state");
    assert!(
        has_any_folded,
        "All buffers should be folded after fold_all"
    );

    // Manually unfold one buffer (simulating a chevron click)
    let first_buffer_id = search_view
        .read_with(cx, |search_view, cx| {
            search_view
                .results_editor
                .read(cx)
                .buffer()
                .read(cx)
                .snapshot(cx)
                .excerpts()
                .next()
                .unwrap()
                .context
                .start
                .buffer_id
        })
        .expect("should read buffer ids");

    search_view
        .update(cx, |search_view, _window, cx| {
            search_view.results_editor.update(cx, |editor, cx| {
                editor.unfold_buffer(first_buffer_id, cx);
            })
        })
        .expect("Should unfold one buffer");

    let has_any_folded = search_view
        .read_with(cx, |search_view, cx| {
            search_view
                .results_editor
                .read(cx)
                .has_any_buffer_folded(cx)
        })
        .expect("should read state");
    assert!(
        has_any_folded,
        "Should still report folds when only one buffer is unfolded"
    );

    // Unfold all via unfold_all
    search_view
        .update(cx, |search_view, window, cx| {
            search_view.results_editor.update(cx, |editor, cx| {
                editor.unfold_all(&UnfoldAll, window, cx);
            })
        })
        .expect("Should unfold fine");
    cx.run_until_parked();

    let has_any_folded = search_view
        .read_with(cx, |search_view, cx| {
            search_view
                .results_editor
                .read(cx)
                .has_any_buffer_folded(cx)
        })
        .expect("should read state");
    assert!(!has_any_folded, "No folds should remain after unfold_all");

    // Manually fold one buffer back (simulating a chevron click)
    search_view
        .update(cx, |search_view, _window, cx| {
            search_view.results_editor.update(cx, |editor, cx| {
                editor.fold_buffer(first_buffer_id, cx);
            })
        })
        .expect("Should fold one buffer");

    let has_any_folded = search_view
        .read_with(cx, |search_view, cx| {
            search_view
                .results_editor
                .read(cx)
                .has_any_buffer_folded(cx)
        })
        .expect("should read state");
    assert!(
        has_any_folded,
        "Should report folds after manually folding one buffer"
    );
}
