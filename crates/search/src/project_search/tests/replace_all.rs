use super::*;

#[gpui::test]
async fn test_replace_all_with_shared_heading_prefix_does_not_loop(cx: &mut TestAppContext) {
    init_test(cx);
    let search_text = "## この日に作成したノート";
    let replacement_text = "## この日に関連するノート";

    let file_a_before = format!("{search_text}\n- a\n\n{search_text}\n- b\n");
    let file_b_before = format!("# Daily\n\n{search_text}\n- c\n");
    let file_a_after = format!("{replacement_text}\n- a\n\n{replacement_text}\n- b\n");
    let file_b_after = format!("# Daily\n\n{replacement_text}\n- c\n");

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "a.md": file_a_before,
            "b.md": file_b_before,
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let worktree_id = project.update(cx, |project, cx| {
        project.worktrees(cx).next().unwrap().read(cx).id()
    });
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let search = cx.new(|cx| ProjectSearch::new(project.clone(), cx));
    let search_view = cx.add_window(|window, cx| {
        ProjectSearchView::new(workspace.downgrade(), search.clone(), window, cx, None)
    });

    perform_search(search_view, search_text, cx);

    search_view
        .update(cx, |search_view, _window, cx| {
            assert_eq!(search_view.entity.read(cx).match_ranges.len(), 3);
        })
        .unwrap();

    search_view
        .update(cx, |search_view, window, cx| {
            search_view.replacement_editor.update(cx, |editor, cx| {
                editor.set_text(replacement_text, window, cx);
            });
            search_view.replace_all(&ReplaceAll, window, cx);
        })
        .unwrap();

    cx.run_until_parked();

    let buffer_a = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("a.md")), cx)
        })
        .await
        .unwrap();
    let buffer_b = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("b.md")), cx)
        })
        .await
        .unwrap();

    assert_eq!(
        buffer_a.read_with(cx, |buffer, _| buffer.text()),
        file_a_after
    );
    assert_eq!(
        buffer_b.read_with(cx, |buffer, _| buffer.text()),
        file_b_after
    );
}
