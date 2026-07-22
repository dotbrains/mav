use super::*;

#[gpui::test]
async fn default_preview_stays_bound_to_invoking_editor_across_splits(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/dir"),
            json!({
                "todo.md": "- [ ] Finish work\n"
            }),
        )
        .await;

    cx.update(|cx| {
        open_paths(
            &[PathBuf::from(path!("/dir/todo.md"))],
            app_state.clone(),
            workspace::OpenOptions::default(),
            cx,
        )
    })
    .await
    .unwrap();

    let multi_workspace = cx.update(|cx| cx.windows()[0].downcast::<MultiWorkspace>().unwrap());
    let (preview, second_editor) = multi_workspace
        .update(cx, |multi_workspace, window, cx| {
            let workspace = multi_workspace.workspace().clone();
            workspace.update(cx, |workspace, cx| {
                let first_editor: Entity<Editor> = workspace
                    .active_item(cx)
                    .and_then(|item| item.act_as::<Editor>(cx))
                    .unwrap();
                let buffer = first_editor
                    .read(cx)
                    .buffer()
                    .read(cx)
                    .as_singleton()
                    .unwrap();
                let project = workspace.project().clone();

                let second_editor =
                    cx.new(|cx| Editor::for_buffer(buffer, Some(project), window, cx));
                let new_pane = workspace.split_pane(
                    workspace.active_pane().clone(),
                    workspace::SplitDirection::Right,
                    window,
                    cx,
                );
                new_pane.update(cx, |pane, cx| {
                    pane.add_item(
                        Box::new(second_editor.clone()),
                        true,
                        true,
                        None,
                        window,
                        cx,
                    )
                });

                let preview = MarkdownPreviewView::create_markdown_view(
                    workspace,
                    second_editor.clone(),
                    window,
                    cx,
                );
                new_pane.update(cx, |pane, cx| {
                    pane.add_item(Box::new(preview.clone()), true, true, None, window, cx)
                });
                (preview, second_editor)
            })
        })
        .unwrap();
    cx.run_until_parked();

    let bound_editor = preview.read_with(cx, |preview, _| {
        preview.active_editor.as_ref().unwrap().editor.clone()
    });
    assert_eq!(
        bound_editor, second_editor,
        "a Default preview must stay bound to the editor it was opened from, not another \
             editor that happens to share the same buffer in a different split"
    );
}
