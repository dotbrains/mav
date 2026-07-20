use super::*;

#[gpui::test]
async fn test_dev_container_flag_opens_modal(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    cx.update(|cx| recent_projects::init(cx));

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

    let errored = cx
        .spawn({
            let app_state = app_state.clone();
            |mut cx| async move {
                let response_sink = DiscardResponseSink;
                open_local_workspace(
                    vec![path!("/project").to_owned()],
                    vec![],
                    false,
                    workspace::OpenOptions {
                        open_in_dev_container: true,
                        ..Default::default()
                    },
                    None,
                    &response_sink,
                    &app_state,
                    &mut cx,
                )
                .await
            }
        })
        .await;

    assert!(!errored);
    cx.run_until_parked();

    let multi_workspace = cx.update(|cx| cx.windows()[0].downcast::<MultiWorkspace>().unwrap());
    multi_workspace
        .update(cx, |multi_workspace, _, cx| {
            let flag = multi_workspace.workspace().read(cx).open_in_dev_container();
            assert!(
                !flag,
                "open_in_dev_container flag should be consumed by suggest_on_worktree_updated"
            );
        })
        .unwrap();
}

#[gpui::test]
async fn test_dev_container_flag_cleared_without_config(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    cx.update(|cx| recent_projects::init(cx));

    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/project"),
            json!({
                "src": {
                    "main.rs": "fn main() {}"
                }
            }),
        )
        .await;

    let errored = cx
        .spawn({
            let app_state = app_state.clone();
            |mut cx| async move {
                let response_sink = DiscardResponseSink;
                open_local_workspace(
                    vec![path!("/project").to_owned()],
                    vec![],
                    false,
                    workspace::OpenOptions {
                        open_in_dev_container: true,
                        ..Default::default()
                    },
                    None,
                    &response_sink,
                    &app_state,
                    &mut cx,
                )
                .await
            }
        })
        .await;

    assert!(!errored);

    // Let any pending worktree scan events and updates settle.
    cx.run_until_parked();

    // With no .devcontainer config, the flag should be cleared once the
    // worktree scan completes, rather than persisting on the workspace.
    let multi_workspace = cx.update(|cx| cx.windows()[0].downcast::<MultiWorkspace>().unwrap());
    multi_workspace
        .update(cx, |multi_workspace, _, cx| {
            let flag = multi_workspace.workspace().read(cx).open_in_dev_container();
            assert!(
                !flag,
                "open_in_dev_container flag should be cleared when no devcontainer config exists"
            );
        })
        .unwrap();
}
