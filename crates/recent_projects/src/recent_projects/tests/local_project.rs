use super::*;

#[gpui::test]
async fn test_open_local_project_reuses_multi_workspace_window(cx: &mut TestAppContext) {
    let app_state = init_test(cx);

    // Disable system path prompts so the injected mock is used.
    cx.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.workspace.use_system_path_prompts = Some(false);
            });
        });
    });

    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/initial-project"),
            json!({ "src": { "main.rs": "" } }),
        )
        .await;
    app_state
        .fs
        .as_fake()
        .insert_tree(path!("/new-project"), json!({ "lib": { "mod.rs": "" } }))
        .await;

    cx.update(|cx| {
        open_paths(
            &[PathBuf::from(path!("/initial-project"))],
            app_state.clone(),
            workspace::OpenOptions::default(),
            cx,
        )
    })
    .await
    .unwrap();

    let initial_window_count = cx.update(|cx| cx.windows().len());
    assert_eq!(initial_window_count, 1);

    let multi_workspace = cx.update(|cx| cx.windows()[0].downcast::<MultiWorkspace>().unwrap());
    cx.run_until_parked();

    let workspace = multi_workspace
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();

    // Set up the prompt mock to return the new project path.
    workspace.update(cx, |workspace, _cx| {
        workspace.set_prompt_for_open_path(Box::new(|_, _, _, _| {
            let (tx, rx) = futures::channel::oneshot::channel();
            tx.send(Some(vec![PathBuf::from(path!("/new-project"))]))
                .ok();
            rx
        }));
    });

    // Call open_local_project with create_new_window: false.
    let weak_workspace = workspace.downgrade();
    multi_workspace
        .update(cx, |_, window, cx| {
            open_local_project(weak_workspace, false, window, cx);
        })
        .unwrap();

    cx.run_until_parked();

    // Should NOT have opened a new window.
    let final_window_count = cx.update(|cx| cx.windows().len());
    assert_eq!(
        final_window_count, initial_window_count,
        "open_local_project with create_new_window=false should reuse the current multi-workspace window"
    );
}

#[gpui::test]
async fn test_open_local_project_new_window_creates_new_window(cx: &mut TestAppContext) {
    let app_state = init_test(cx);

    // Disable system path prompts so the injected mock is used.
    cx.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.workspace.use_system_path_prompts = Some(false);
            });
        });
    });

    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/initial-project"),
            json!({ "src": { "main.rs": "" } }),
        )
        .await;
    app_state
        .fs
        .as_fake()
        .insert_tree(path!("/new-project"), json!({ "lib": { "mod.rs": "" } }))
        .await;

    cx.update(|cx| {
        open_paths(
            &[PathBuf::from(path!("/initial-project"))],
            app_state.clone(),
            workspace::OpenOptions::default(),
            cx,
        )
    })
    .await
    .unwrap();

    let initial_window_count = cx.update(|cx| cx.windows().len());
    assert_eq!(initial_window_count, 1);

    let multi_workspace = cx.update(|cx| cx.windows()[0].downcast::<MultiWorkspace>().unwrap());
    cx.run_until_parked();

    let workspace = multi_workspace
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();

    // Set up the prompt mock to return the new project path.
    workspace.update(cx, |workspace, _cx| {
        workspace.set_prompt_for_open_path(Box::new(|_, _, _, _| {
            let (tx, rx) = futures::channel::oneshot::channel();
            tx.send(Some(vec![PathBuf::from(path!("/new-project"))]))
                .ok();
            rx
        }));
    });

    // Call open_local_project with create_new_window: true.
    let weak_workspace = workspace.downgrade();
    multi_workspace
        .update(cx, |_, window, cx| {
            open_local_project(weak_workspace, true, window, cx);
        })
        .unwrap();

    cx.run_until_parked();

    // Should have opened a new window.
    let final_window_count = cx.update(|cx| cx.windows().len());
    assert_eq!(
        final_window_count,
        initial_window_count + 1,
        "open_local_project with create_new_window=true should open a new window"
    );
}
