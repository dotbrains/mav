use super::*;

#[gpui::test]
async fn test_reopening_closed_items(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/root"),
            json!({
                "a": {
                    "file1": "",
                    "file2": "",
                    "file3": "",
                    "file4": "",
                },
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/root").as_ref()], cx).await;
    project.update(cx, |project, _cx| project.languages().add(markdown_lang()));
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(*window, cx);
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    let entries = cx.read(|cx| workspace.file_project_paths(cx));
    let file1 = entries[0].clone();
    let file2 = entries[1].clone();
    let file3 = entries[2].clone();
    let file4 = entries[3].clone();

    let file1_item_id = workspace
        .update_in(cx, |w, window, cx| {
            w.open_path(file1.clone(), None, true, window, cx)
        })
        .await
        .unwrap()
        .item_id();
    let file2_item_id = workspace
        .update_in(cx, |w, window, cx| {
            w.open_path(file2.clone(), None, true, window, cx)
        })
        .await
        .unwrap()
        .item_id();
    let file3_item_id = workspace
        .update_in(cx, |w, window, cx| {
            w.open_path(file3.clone(), None, true, window, cx)
        })
        .await
        .unwrap()
        .item_id();
    let file4_item_id = workspace
        .update_in(cx, |w, window, cx| {
            w.open_path(file4.clone(), None, true, window, cx)
        })
        .await
        .unwrap()
        .item_id();
    assert_eq!(active_path(&workspace, cx), Some(file4.clone()));

    // Close all the pane items in some arbitrary order.
    workspace
        .update_in(cx, |_, window, cx| {
            pane.update(cx, |pane, cx| {
                pane.close_item_by_id(file1_item_id, SaveIntent::Close, window, cx)
            })
        })
        .await
        .unwrap();
    assert_eq!(active_path(&workspace, cx), Some(file4.clone()));

    workspace
        .update_in(cx, |_, window, cx| {
            pane.update(cx, |pane, cx| {
                pane.close_item_by_id(file4_item_id, SaveIntent::Close, window, cx)
            })
        })
        .await
        .unwrap();
    assert_eq!(active_path(&workspace, cx), Some(file3.clone()));

    workspace
        .update_in(cx, |_, window, cx| {
            pane.update(cx, |pane, cx| {
                pane.close_item_by_id(file2_item_id, SaveIntent::Close, window, cx)
            })
        })
        .await
        .unwrap();
    assert_eq!(active_path(&workspace, cx), Some(file3.clone()));
    workspace
        .update_in(cx, |_, window, cx| {
            pane.update(cx, |pane, cx| {
                pane.close_item_by_id(file3_item_id, SaveIntent::Close, window, cx)
            })
        })
        .await
        .unwrap();

    assert_eq!(active_path(&workspace, cx), None);

    // Reopen all the closed items, ensuring they are reopened in the same order
    // in which they were closed.
    workspace
        .update_in(cx, Workspace::reopen_closed_item)
        .await
        .unwrap();
    assert_eq!(active_path(&workspace, cx), Some(file3.clone()));

    workspace
        .update_in(cx, Workspace::reopen_closed_item)
        .await
        .unwrap();
    assert_eq!(active_path(&workspace, cx), Some(file2.clone()));

    workspace
        .update_in(cx, Workspace::reopen_closed_item)
        .await
        .unwrap();
    assert_eq!(active_path(&workspace, cx), Some(file4.clone()));

    workspace
        .update_in(cx, Workspace::reopen_closed_item)
        .await
        .unwrap();
    assert_eq!(active_path(&workspace, cx), Some(file1.clone()));

    // Reopening past the last closed item is a no-op.
    workspace
        .update_in(cx, Workspace::reopen_closed_item)
        .await
        .unwrap();
    assert_eq!(active_path(&workspace, cx), Some(file1.clone()));

    // Reopening closed items doesn't interfere with navigation history.
    // Verify we can navigate back through the history after reopening items.
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.go_back(workspace.active_pane().downgrade(), window, cx)
        })
        .await
        .unwrap();

    // After go_back, we should be at a different file than file1
    let after_go_back = active_path(&workspace, cx);
    assert!(
        after_go_back.is_some() && after_go_back != Some(file1.clone()),
        "After go_back from file1, should be at a different file"
    );

    pane.read_with(cx, |pane, _| {
        assert!(pane.can_navigate_forward(), "Should be able to go forward");
    });

    fn active_path(workspace: &Entity<Workspace>, cx: &VisualTestContext) -> Option<ProjectPath> {
        workspace.read_with(cx, |workspace, cx| {
            let item = workspace.active_item(cx)?;
            item.project_path(cx)
        })
    }
}

fn init_keymap_test(cx: &mut TestAppContext) -> Arc<AppState> {
    cx.update(|cx| {
        let app_state = AppState::test(cx);

        theme_settings::init(theme::LoadThemes::JustBase, cx);
        client::init(&app_state.client, cx);
        workspace::init(app_state.clone(), cx);
        onboarding::init(cx);
        app_state
    })
}

actions!(test_only, [ActionA, ActionB]);
