use super::*;

#[gpui::test]
async fn test_open_dev_container_action_with_single_config(cx: &mut TestAppContext) {
    let app_state = init_test(cx);

    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/project"),
            json!({
                ".devcontainer": {
                    "devcontainer.json": "{}"
                },
                "src": {
                    "main.rs": "fn main() {}"
                }
            }),
        )
        .await;

    // Open a file path (not a directory) so that the worktree root is a
    // file. This means `active_project_directory` returns `None`, which
    // causes `DevContainerContext::from_workspace` to return `None`,
    // preventing `open_dev_container` from spawning real I/O (docker
    // commands, shell environment loading) that is incompatible with the
    // test scheduler. The modal is still created and the re-entrancy
    // guard that this test validates is still exercised.
    cx.update(|cx| {
        open_paths(
            &[PathBuf::from(path!("/project/src/main.rs"))],
            app_state,
            workspace::OpenOptions::default(),
            cx,
        )
    })
    .await
    .unwrap();

    assert_eq!(cx.update(|cx| cx.windows().len()), 1);
    let multi_workspace = cx.update(|cx| cx.windows()[0].downcast::<MultiWorkspace>().unwrap());

    cx.run_until_parked();

    // This dispatch triggers with_active_or_new_workspace -> MultiWorkspace::update
    // -> Workspace::update -> toggle_modal -> new_dev_container.
    // Before the fix, this panicked with "cannot read workspace::Workspace while
    // it is already being updated" because new_dev_container and open_dev_container
    // tried to read the Workspace entity through a WeakEntity handle while it was
    // already leased by the outer update.
    cx.dispatch_action(*multi_workspace, OpenDevContainer);

    multi_workspace
        .update(cx, |multi_workspace, _, cx| {
            let modal = multi_workspace
                .workspace()
                .read(cx)
                .active_modal::<RemoteServerProjects>(cx);
            assert!(
                modal.is_some(),
                "Dev container modal should be open after dispatching OpenDevContainer"
            );
        })
        .unwrap();
}

#[gpui::test]
async fn test_open_dev_container_action_with_multiple_configs(cx: &mut TestAppContext) {
    let app_state = init_test(cx);

    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/project"),
            json!({
                ".devcontainer": {
                    "rust": {
                        "devcontainer.json": "{}"
                    },
                    "python": {
                        "devcontainer.json": "{}"
                    }
                },
                "src": {
                    "main.rs": "fn main() {}"
                }
            }),
        )
        .await;

    cx.update(|cx| {
        open_paths(
            &[PathBuf::from(path!("/project"))],
            app_state,
            workspace::OpenOptions::default(),
            cx,
        )
    })
    .await
    .unwrap();

    assert_eq!(cx.update(|cx| cx.windows().len()), 1);
    let multi_workspace = cx.update(|cx| cx.windows()[0].downcast::<MultiWorkspace>().unwrap());

    cx.run_until_parked();

    cx.dispatch_action(*multi_workspace, OpenDevContainer);

    multi_workspace
            .update(cx, |multi_workspace, _, cx| {
                let modal = multi_workspace
                    .workspace()
                    .read(cx)
                    .active_modal::<RemoteServerProjects>(cx);
                assert!(
                    modal.is_some(),
                    "Dev container modal should be open after dispatching OpenDevContainer with multiple configs"
                );
            })
            .unwrap();
}
