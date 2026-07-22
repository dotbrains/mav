use super::*;

#[gpui::test]
async fn test_scroll_search_results_to_top(cx: &mut TestAppContext) {
    init_test(cx);
    // We need many lines in the search results to be able to scroll the window
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "1.txt": "\n\n\n\n\n A \n\n\n\n\n",
            "2.txt": "\n\n\n\n\n A \n\n\n\n\n",
            "3.rs": "\n\n\n\n\n A \n\n\n\n\n",
            "4.rs": "\n\n\n\n\n A \n\n\n\n\n",
            "5.rs": "\n\n\n\n\n A \n\n\n\n\n",
            "6.rs": "\n\n\n\n\n A \n\n\n\n\n",
            "7.rs": "\n\n\n\n\n A \n\n\n\n\n",
            "8.rs": "\n\n\n\n\n A \n\n\n\n\n",
            "9.rs": "\n\n\n\n\n A \n\n\n\n\n",
            "a.rs": "\n\n\n\n\n A \n\n\n\n\n",
            "b.rs": "\n\n\n\n\n B \n\n\n\n\n",
            "c.rs": "\n\n\n\n\n B \n\n\n\n\n",
            "d.rs": "\n\n\n\n\n B \n\n\n\n\n",
            "e.rs": "\n\n\n\n\n B \n\n\n\n\n",
            "f.rs": "\n\n\n\n\n B \n\n\n\n\n",
            "g.rs": "\n\n\n\n\n B \n\n\n\n\n",
            "h.rs": "\n\n\n\n\n B \n\n\n\n\n",
            "i.rs": "\n\n\n\n\n B \n\n\n\n\n",
            "j.rs": "\n\n\n\n\n B \n\n\n\n\n",
            "k.rs": "\n\n\n\n\n B \n\n\n\n\n",
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let search = cx.new(|cx| ProjectSearch::new(project, cx));
    let search_view = cx.add_window(|window, cx| {
        ProjectSearchView::new(workspace.downgrade(), search.clone(), window, cx, None)
    });

    // First search
    perform_search(search_view, "A", cx);
    search_view
        .update(cx, |search_view, window, cx| {
            search_view.results_editor.update(cx, |results_editor, cx| {
                // Results are correct and scrolled to the top
                assert_eq!(
                    results_editor.display_text(cx).match_indices(" A ").count(),
                    10
                );
                assert_eq!(results_editor.scroll_position(cx), Point::default());

                // Scroll results all the way down
                results_editor.scroll(Point::new(0., f64::MAX), Some(Axis::Vertical), window, cx);
            });
        })
        .expect("unable to update search view");

    // Second search
    perform_search(search_view, "B", cx);
    search_view
        .update(cx, |search_view, _, cx| {
            search_view.results_editor.update(cx, |results_editor, cx| {
                // Results are correct...
                assert_eq!(
                    results_editor.display_text(cx).match_indices(" B ").count(),
                    10
                );
                // ...and scrolled back to the top
                assert_eq!(results_editor.scroll_position(cx), Point::default());
            });
        })
        .expect("unable to update search view");
}
