use super::*;

#[gpui::test]
async fn preview_serialized_path_updates_when_source_file_is_renamed(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/dir"),
            json!({
                "todo.md": "![image](image.png)\n",
                "subdir": {},
            }),
        )
        .await;

    cx.update(|cx| {
        open_paths(
            &[PathBuf::from(path!("/dir"))],
            app_state.clone(),
            workspace::OpenOptions::default(),
            cx,
        )
    })
    .await
    .unwrap();

    let multi_workspace = cx.update(|cx| cx.windows()[0].downcast::<MultiWorkspace>().unwrap());
    let open_task = multi_workspace
        .update(cx, |multi_workspace, window, cx| {
            let workspace = multi_workspace.workspace().clone();
            workspace.update(cx, |workspace, cx| {
                let worktree_id = workspace
                    .project()
                    .read(cx)
                    .worktrees(cx)
                    .next()
                    .unwrap()
                    .read(cx)
                    .id();
                workspace.open_path((worktree_id, rel_path("todo.md")), None, true, window, cx)
            })
        })
        .unwrap();
    open_task.await.unwrap();
    cx.run_until_parked();

    let (preview, project, workspace_id) = multi_workspace
        .update(cx, |multi_workspace, window, cx| {
            let workspace = multi_workspace.workspace().clone();
            workspace.update(cx, |workspace, cx| {
                workspace.set_random_database_id();
                let workspace_id = workspace.database_id().unwrap();
                let project = workspace.project().clone();
                let editor: Entity<Editor> = workspace
                    .active_item(cx)
                    .and_then(|item| item.act_as::<Editor>(cx))
                    .unwrap();
                let preview =
                    MarkdownPreviewView::create_markdown_view(workspace, editor, window, cx);
                workspace.active_pane().update(cx, |pane, cx| {
                    pane.add_item(Box::new(preview.clone()), true, true, None, window, cx)
                });
                (preview, project, workspace_id)
            })
        })
        .unwrap();
    let workspace_serialization_tasks = multi_workspace
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.flush_all_serialization(window, cx)
        })
        .unwrap();
    for task in workspace_serialization_tasks {
        task.await;
    }

    let serialize_task = multi_workspace
        .update(cx, |multi_workspace, window, cx| {
            let workspace = multi_workspace.workspace().clone();
            workspace.update(cx, |workspace, cx| {
                preview
                    .update(cx, |preview, cx| {
                        preview.serialize(workspace, cx.entity_id().as_u64(), false, window, cx)
                    })
                    .unwrap()
            })
        })
        .unwrap();
    serialize_task.await.unwrap();

    assert_eq!(
        saved_preview_path(cx, preview.entity_id().as_u64(), workspace_id),
        PathBuf::from(path!("/dir/todo.md"))
    );

    let (entry_id, worktree_id, destination_path) = preview.read_with(cx, |preview, cx| {
        let editor = &preview.active_editor.as_ref().unwrap().editor;
        let buffer = editor.read(cx).buffer().read(cx).as_singleton().unwrap();
        let buffer = buffer.read(cx);
        let file = buffer.file().unwrap();
        let worktree_id = file.worktree_id(cx);
        let source_path = file.path();
        let mut destination_path = source_path.to_rel_path_buf();
        destination_path.pop();
        destination_path.push(rel_path("subdir/renamed.md"));
        let worktree = project.read(cx).worktree_for_id(worktree_id, cx).unwrap();
        let entry_id = worktree.read(cx).entry_for_path(source_path).unwrap().id;
        (
            entry_id,
            worktree_id,
            destination_path.as_rel_path().into_arc(),
        )
    });
    project
        .update(cx, |project, cx| {
            project.rename_entry(entry_id, (worktree_id, destination_path).into(), cx)
        })
        .await
        .unwrap();
    wait_for_preview_serialization(cx).await;

    assert_eq!(
        preview.read_with(cx, |preview, _| preview.base_directory.clone()),
        Some(PathBuf::from(path!("/dir/subdir")))
    );
    assert_eq!(
        saved_preview_path(cx, preview.entity_id().as_u64(), workspace_id),
        PathBuf::from(path!("/dir/subdir/renamed.md"))
    );
}

#[gpui::test]
async fn follow_preview_serialized_path_updates_when_followed_editor_changes(
    cx: &mut TestAppContext,
) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/dir"),
            json!({
                "a.md": "# A\n",
                "b.md": "# B\n",
            }),
        )
        .await;

    cx.update(|cx| {
        open_paths(
            &[PathBuf::from(path!("/dir"))],
            app_state.clone(),
            workspace::OpenOptions::default(),
            cx,
        )
    })
    .await
    .unwrap();

    let multi_workspace = cx.update(|cx| cx.windows()[0].downcast::<MultiWorkspace>().unwrap());
    let worktree_id = multi_workspace
        .update(cx, |multi_workspace, _, cx| {
            multi_workspace
                .workspace()
                .read(cx)
                .project()
                .read(cx)
                .worktrees(cx)
                .next()
                .unwrap()
                .read(cx)
                .id()
        })
        .unwrap();

    let open_task = multi_workspace
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.workspace().update(cx, |workspace, cx| {
                workspace.open_path((worktree_id, rel_path("a.md")), None, true, window, cx)
            })
        })
        .unwrap();
    let opened_item = open_task.await.unwrap();
    cx.run_until_parked();
    let editor_a = cx.update(|cx| opened_item.act_as::<Editor>(cx).unwrap());

    let open_task = multi_workspace
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.workspace().update(cx, |workspace, cx| {
                workspace.open_path((worktree_id, rel_path("b.md")), None, true, window, cx)
            })
        })
        .unwrap();
    let opened_item = open_task.await.unwrap();
    cx.run_until_parked();
    let editor_b = cx.update(|cx| opened_item.act_as::<Editor>(cx).unwrap());
    let editor_b_path = editor_source_path(cx, &editor_b);
    assert_eq!(editor_b_path.as_ref(), rel_path("b.md"));

    let (preview, workspace_id) = multi_workspace
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.workspace().update(cx, |workspace, cx| {
                workspace.set_random_database_id();
                let workspace_id = workspace.database_id().unwrap();
                let preview = MarkdownPreviewView::create_following_markdown_view(
                    workspace, editor_a, window, cx,
                );
                workspace.active_pane().update(cx, |pane, cx| {
                    pane.add_item(Box::new(preview.clone()), true, true, None, window, cx)
                });
                (preview, workspace_id)
            })
        })
        .unwrap();
    let workspace_serialization_tasks = multi_workspace
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.flush_all_serialization(window, cx)
        })
        .unwrap();
    for task in workspace_serialization_tasks {
        task.await;
    }
    wait_for_preview_serialization(cx).await;

    let serialize_task = multi_workspace
        .update(cx, |multi_workspace, window, cx| {
            let workspace = multi_workspace.workspace().clone();
            workspace.update(cx, |workspace, cx| {
                preview
                    .update(cx, |preview, cx| {
                        preview.serialize(workspace, cx.entity_id().as_u64(), false, window, cx)
                    })
                    .unwrap()
            })
        })
        .unwrap();
    serialize_task.await.unwrap();

    assert_eq!(
        saved_preview_path(cx, preview.entity_id().as_u64(), workspace_id),
        PathBuf::from(path!("/dir/a.md"))
    );

    multi_workspace
        .update(cx, |_, window, cx| {
            preview.update(cx, |preview, cx| {
                preview.set_editor(editor_b, window, cx);
            });
        })
        .unwrap();
    wait_for_preview_serialization(cx).await;

    let followed_path = preview_source_path(cx, &preview);
    assert_eq!(followed_path.as_ref(), rel_path("b.md"));

    assert_eq!(
        saved_preview_path(cx, preview.entity_id().as_u64(), workspace_id),
        PathBuf::from(path!("/dir/b.md")),
        "a Follow preview should persist the source editor it most recently followed"
    );
}
