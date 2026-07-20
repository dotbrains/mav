use super::cli_test_helpers::{
    make_cli_open_request, make_cli_url_open_request, run_cli_with_mav_handler,
};
use super::*;

#[gpui::test]
async fn test_e2e_explicit_existing_flag_no_prompt(cx: &mut TestAppContext) {
    let app_state = init_test(cx);

    app_state
        .fs
        .as_fake()
        .insert_tree(path!("/project"), json!({ "file.txt": "content" }))
        .await;

    // Create an existing window
    open_workspace_file(path!("/project"), Default::default(), app_state.clone(), cx).await;
    assert_eq!(cx.windows().len(), 1);

    let (status, prompt_shown) = run_cli_with_mav_handler(
        cx,
        app_state,
        make_cli_open_request(
            vec![path!("/project/file.txt").to_string()],
            cli::OpenBehavior::ExistingWindow, // -e flag: force existing window
        ),
        None,
    );

    assert_eq!(status, 0);
    assert!(!prompt_shown, "no prompt should be shown with -e flag");
    assert_eq!(cx.windows().len(), 1);
}

#[gpui::test]
async fn test_e2e_explicit_new_flag_no_prompt(cx: &mut TestAppContext) {
    let app_state = init_test(cx);

    app_state
        .fs
        .as_fake()
        .insert_tree(path!("/project_a"), json!({ "file.txt": "content" }))
        .await;
    app_state
        .fs
        .as_fake()
        .insert_tree(path!("/project_b"), json!({ "file.txt": "content" }))
        .await;

    // Create an existing window
    open_workspace_file(
        path!("/project_a"),
        Default::default(),
        app_state.clone(),
        cx,
    )
    .await;
    assert_eq!(cx.windows().len(), 1);

    let (status, prompt_shown) = run_cli_with_mav_handler(
        cx,
        app_state,
        make_cli_open_request(
            vec![path!("/project_b/file.txt").to_string()],
            cli::OpenBehavior::AlwaysNew, // -n flag: force new window
        ),
        None,
    );

    assert_eq!(status, 0);
    assert!(!prompt_shown, "no prompt should be shown with -n flag");
    assert_eq!(cx.windows().len(), 2);
}

#[gpui::test]
async fn test_e2e_explicit_new_flag_with_file_url_opens_new_window(cx: &mut TestAppContext) {
    let app_state = init_test(cx);

    app_state
        .fs
        .as_fake()
        .insert_tree(path!("/project"), json!({ "file.txt": "content" }))
        .await;

    open_workspace_file(path!("/project"), Default::default(), app_state.clone(), cx).await;
    assert_eq!(cx.windows().len(), 1);

    let file_url = format!(
        "file://{}",
        urlencoding::encode(path!("/project/file.txt")).into_owned()
    );
    let (status, prompt_shown) = run_cli_with_mav_handler(
        cx,
        app_state,
        make_cli_url_open_request(vec![file_url], cli::OpenBehavior::AlwaysNew),
        None,
    );

    assert_eq!(status, 0);
    assert!(!prompt_shown, "no prompt should be shown with -n flag");
    assert_eq!(cx.windows().len(), 2);
}

#[gpui::test]
async fn test_e2e_paths_in_existing_workspace_no_prompt(cx: &mut TestAppContext) {
    let app_state = init_test(cx);

    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/project"),
            json!({
                "src": {
                    "main.rs": "fn main() {}",
                }
            }),
        )
        .await;

    // Open the project directory as a workspace
    open_workspace_file(path!("/project"), Default::default(), app_state.clone(), cx).await;
    assert_eq!(cx.windows().len(), 1);

    // Opening a file inside the already-open workspace should not prompt
    let (status, prompt_shown) = run_cli_with_mav_handler(
        cx,
        app_state,
        make_cli_open_request(
            vec![path!("/project/src/main.rs").to_string()],
            cli::OpenBehavior::Default,
        ),
        None,
    );

    assert_eq!(status, 0);
    assert!(
        !prompt_shown,
        "no prompt should be shown when paths are in an existing workspace"
    );
    // File opened in existing window
    assert_eq!(cx.windows().len(), 1);
}
