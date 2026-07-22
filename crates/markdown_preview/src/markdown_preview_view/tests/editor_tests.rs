use super::*;

#[gpui::test]
async fn toggles_task_checkbox_and_saves_when_preview_is_active(cx: &mut TestAppContext) {
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
    let preview = multi_workspace
        .update(cx, |multi_workspace, window, cx| {
            let workspace = multi_workspace.workspace().clone();
            let editor: Entity<Editor> = workspace
                .read(cx)
                .active_item(cx)
                .and_then(|item| item.act_as::<Editor>(cx))
                .unwrap();

            workspace.update(cx, |workspace, cx| {
                let preview = MarkdownPreviewView::create_markdown_view(
                    workspace,
                    editor.clone(),
                    window,
                    cx,
                );
                workspace.active_pane().update(cx, |pane, cx| {
                    pane.add_item(Box::new(preview.clone()), true, true, None, window, cx)
                });
                preview
            })
        })
        .unwrap();
    cx.run_until_parked();

    let save_task = multi_workspace
        .update(cx, |multi_workspace, window, cx| {
            let workspace: Entity<Workspace> = multi_workspace.workspace().clone();
            let view_handle = preview.downgrade();
            assert!(preview.read(cx).focus_handle.contains_focused(window, cx));
            preview.update(cx, |preview, cx| {
                let editor = preview.active_editor.as_ref().unwrap().editor.clone();
                MarkdownPreviewView::apply_checkbox_toggle_to_editor(&editor, 2..5, true, cx);
            });
            MarkdownPreviewView::refresh_preview(view_handle, window, cx);

            workspace.update(cx, |workspace: &mut Workspace, cx| {
                workspace.save_active_item(SaveIntent::Save, window, cx)
            })
        })
        .unwrap();

    save_task.await.unwrap();
    cx.run_until_parked();

    assert_eq!(
        app_state
            .fs
            .load(path!("/dir/todo.md").as_ref())
            .await
            .unwrap(),
        "- [x] Finish work\n"
    );
}

#[gpui::test]
async fn preview_uses_buffer_contents_instead_of_diff_contents(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/dir"),
            json!({
                "note.md": "new\n"
            }),
        )
        .await;

    cx.update(|cx| {
        open_paths(
            &[PathBuf::from(path!("/dir/note.md"))],
            app_state.clone(),
            workspace::OpenOptions::default(),
            cx,
        )
    })
    .await
    .unwrap();

    let multi_workspace = cx.update(|cx| cx.windows()[0].downcast::<MultiWorkspace>().unwrap());
    let preview = multi_workspace
        .update(cx, |multi_workspace, window, cx| {
            let workspace = multi_workspace.workspace().clone();
            workspace.update(cx, |workspace, cx| {
                let editor: Entity<Editor> = workspace
                    .active_item(cx)
                    .and_then(|item| item.act_as::<Editor>(cx))
                    .unwrap();
                let buffer = editor.read(cx).buffer().read(cx).as_singleton().unwrap();
                let diff = cx.new(|cx| {
                    BufferDiff::new_with_base_text("old\n", &buffer.read(cx).text_snapshot(), cx)
                });
                let multibuffer = editor.read(cx).buffer().clone();
                multibuffer.update(cx, |multibuffer, cx| {
                    multibuffer.add_diff(diff, cx);
                    multibuffer.set_all_diff_hunks_expanded(cx);
                });

                let diff_text = multibuffer.read(cx).snapshot(cx).text();
                assert!(diff_text.contains("old"));
                assert!(diff_text.contains("new"));

                let preview =
                    MarkdownPreviewView::create_markdown_view(workspace, editor, window, cx);
                workspace.active_pane().update(cx, |pane, cx| {
                    pane.add_item(Box::new(preview.clone()), true, true, None, window, cx)
                });
                preview
            })
        })
        .unwrap();
    cx.run_until_parked();

    assert_eq!(
        preview.read_with(cx, |preview, cx| preview
            .markdown
            .read(cx)
            .source()
            .to_string()),
        "new\n"
    );
}

#[gpui::test]
async fn force_closing_preview_preserves_source_editor_changes(cx: &mut TestAppContext) {
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
    let (preview, editor) = multi_workspace
        .update(cx, |multi_workspace, window, cx| {
            let workspace = multi_workspace.workspace().clone();
            let editor: Entity<Editor> = workspace
                .read(cx)
                .active_item(cx)
                .and_then(|item| item.act_as::<Editor>(cx))
                .unwrap();

            let preview = workspace.update(cx, |workspace, cx| {
                let preview = MarkdownPreviewView::create_markdown_view(
                    workspace,
                    editor.clone(),
                    window,
                    cx,
                );
                workspace.active_pane().update(cx, |pane, cx| {
                    pane.add_item(Box::new(preview.clone()), true, true, None, window, cx)
                });
                preview
            });

            (preview, editor)
        })
        .unwrap();
    cx.run_until_parked();

    multi_workspace
        .update(cx, |_, window, cx| {
            let view_handle = preview.downgrade();
            assert!(preview.read(cx).focus_handle.contains_focused(window, cx));
            MarkdownPreviewView::apply_checkbox_toggle_to_editor(&editor, 2..5, true, cx);
            MarkdownPreviewView::refresh_preview(view_handle, window, cx);
        })
        .unwrap();

    assert_eq!(
        editor.read_with(cx, |editor, cx| editor.buffer().read(cx).read(cx).text()),
        "- [x] Finish work\n"
    );

    let close_task = multi_workspace
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.workspace().update(cx, |workspace, cx| {
                workspace.active_pane().update(cx, |pane, cx| {
                    pane.close_item_by_id(preview.entity_id(), SaveIntent::Skip, window, cx)
                })
            })
        })
        .unwrap();

    close_task.await.unwrap();
    cx.run_until_parked();

    assert_eq!(
        editor.read_with(cx, |editor, cx| editor.buffer().read(cx).read(cx).text()),
        "- [x] Finish work\n"
    );
}
