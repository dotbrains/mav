use super::cli_test_helpers::{make_cli_open_request, run_cli_with_mav_handler};
use super::*;

#[gpui::test]
async fn test_e2e_no_flags_no_windows_no_prompt(cx: &mut TestAppContext) {
    let app_state = init_test(cx);

    app_state
        .fs
        .as_fake()
        .insert_tree(path!("/project"), json!({ "file.txt": "content" }))
        .await;

    assert_eq!(cx.windows().len(), 0);

    let (status, prompt_shown) = run_cli_with_mav_handler(
        cx,
        app_state,
        make_cli_open_request(
            vec![path!("/project/file.txt").to_string()],
            cli::OpenBehavior::Default,
        ),
        None,
    );

    assert_eq!(status, 0);
    assert!(
        !prompt_shown,
        "no prompt should be shown when no windows exist"
    );
    assert_eq!(cx.windows().len(), 1);
}

#[gpui::test]
async fn test_e2e_prompt_user_picks_existing_window(cx: &mut TestAppContext) {
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

    // Create an existing window so the prompt triggers
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
        app_state.clone(),
        make_cli_open_request(
            vec![path!("/project_b").to_string()],
            cli::OpenBehavior::Default,
        ),
        Some(cli::CliBehaviorSetting::ExistingWindow),
    );

    assert_eq!(status, 0);
    assert!(prompt_shown, "prompt should be shown");
    assert_eq!(cx.windows().len(), 1);

    let settings_text = app_state
        .fs
        .load(paths::settings_file())
        .await
        .unwrap_or_default();
    assert!(
        settings_text.contains("existing_window"),
        "settings should contain 'existing_window', got: {settings_text}"
    );
}

#[gpui::test]
async fn test_e2e_prompt_user_picks_new_window(cx: &mut TestAppContext) {
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

    // Create an existing window with project_a
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
        app_state.clone(),
        make_cli_open_request(
            vec![path!("/project_b").to_string()],
            cli::OpenBehavior::Default,
        ),
        Some(cli::CliBehaviorSetting::NewWindow),
    );

    assert_eq!(status, 0);
    assert!(prompt_shown, "prompt should be shown");
    assert_eq!(cx.windows().len(), 2);

    let settings_text = app_state
        .fs
        .load(paths::settings_file())
        .await
        .unwrap_or_default();
    assert!(
        settings_text.contains("new_window"),
        "settings should contain 'new_window', got: {settings_text}"
    );
}

#[gpui::test]
async fn test_e2e_setting_already_configured_no_prompt(cx: &mut TestAppContext) {
    let app_state = init_test(cx);

    app_state
        .fs
        .as_fake()
        .insert_tree(path!("/project"), json!({ "file.txt": "content" }))
        .await;

    // Pre-configure the setting in settings.json
    app_state
        .fs
        .as_fake()
        .insert_tree(
            paths::config_dir(),
            json!({
                "settings.json": r#"{"cli_default_open_behavior": "existing_window"}"#
            }),
        )
        .await;

    // Create an existing window
    open_workspace_file(path!("/project"), Default::default(), app_state.clone(), cx).await;
    assert_eq!(cx.windows().len(), 1);

    let (status, prompt_shown) = run_cli_with_mav_handler(
        cx,
        app_state,
        make_cli_open_request(
            vec![path!("/project/file.txt").to_string()],
            cli::OpenBehavior::Default,
        ),
        None,
    );

    assert_eq!(status, 0);
    assert!(
        !prompt_shown,
        "no prompt should be shown when setting already configured"
    );
}
