use super::*;

#[gpui::test]
async fn test_deleted_file_removed_from_search_results(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "file_a.txt": "hello world",
            "file_b.txt": "hello universe",
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

    perform_search(search_view, "hello", cx);

    search_view
        .update(cx, |search_view, _window, cx| {
            let match_count = search_view.entity.read(cx).match_ranges.len();
            assert_eq!(match_count, 2, "Should have matches from both files");
        })
        .unwrap();

    // Delete file_b.txt
    fs.remove_file(
        path!("/dir/file_b.txt").as_ref(),
        fs::RemoveOptions::default(),
    )
    .await
    .unwrap();
    cx.run_until_parked();

    // Verify deleted file's results are removed proactively
    search_view
        .update(cx, |search_view, _window, cx| {
            let results_text = search_view
                .results_editor
                .update(cx, |editor, cx| editor.display_text(cx));
            assert!(
                !results_text.contains("universe"),
                "Deleted file's content should be removed from results, got: {results_text}"
            );
            assert!(
                results_text.contains("world"),
                "Remaining file's content should still be present, got: {results_text}"
            );
        })
        .unwrap();

    // Re-run the search and verify deleted file stays gone
    perform_search(search_view, "hello", cx);

    search_view
        .update(cx, |search_view, _window, cx| {
            let results_text = search_view
                .results_editor
                .update(cx, |editor, cx| editor.display_text(cx));
            assert!(
                !results_text.contains("universe"),
                "Deleted file should not reappear after re-search, got: {results_text}"
            );
            assert!(
                results_text.contains("world"),
                "Remaining file should still be found, got: {results_text}"
            );
            assert_eq!(
                search_view.entity.read(cx).match_ranges.len(),
                1,
                "Should only have match from the remaining file"
            );
        })
        .unwrap();
}
