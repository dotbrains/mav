use super::*;

/// An intra-project symlink like `safe -> .mav` keeps a path's
/// raw components clean of `.mav`, and `resolve_project_path`
/// (correctly) doesn't flag the symlink as an escape because the
/// target stays inside the worktree. The canonical-path recheck is
/// the only thing standing between the agent and a silent settings
/// rewrite, so verify it fires.
#[gpui::test]
async fn test_streaming_authorize_blocks_intra_project_symlink_bypass(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = project::FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            ".mav": { "settings.json": "{}" },
        }),
    )
    .await;
    fs.insert_symlink(path!("/root/safe"), PathBuf::from(".mav"))
        .await;
    let (edit_tool, _project, _action_log, _fs, _thread) =
        setup_test_with_fs(cx, fs, &[path!("/root").as_ref()]).await;

    let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
    let _auth = cx.update(|cx| {
        edit_tool.authorize(
            &PathBuf::from(path!("/root/safe/settings.json")),
            &stream_tx,
            cx,
        )
    });
    let event = stream_rx.expect_authorization().await;
    assert!(
        event
            .tool_call
            .fields
            .title
            .as_deref()
            .is_some_and(|title| title.ends_with("(local settings)")),
        "Intra-project symlink to .mav must still prompt: {:?}",
        event.tool_call.fields.title,
    );
}

/// Same as the previous test but for the agent-skills sensitive
/// path, via an intra-project symlink `safe -> .agents/skills`.
#[gpui::test]
async fn test_streaming_authorize_blocks_intra_project_symlink_skills_bypass(
    cx: &mut TestAppContext,
) {
    init_test(cx);
    let fs = project::FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            ".agents": {
                "skills": { "my-skill": { "SKILL.md": "target" } },
            },
        }),
    )
    .await;
    fs.insert_symlink(path!("/root/safe"), PathBuf::from(".agents/skills"))
        .await;
    let (edit_tool, _project, _action_log, _fs, _thread) =
        setup_test_with_fs(cx, fs, &[path!("/root").as_ref()]).await;

    let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
    let _auth = cx.update(|cx| {
        edit_tool.authorize(
            &PathBuf::from(path!("/root/safe/my-skill/SKILL.md")),
            &stream_tx,
            cx,
        )
    });
    let event = stream_rx.expect_authorization().await;
    assert!(
        event
            .tool_call
            .fields
            .title
            .as_deref()
            .is_some_and(|title| title.ends_with("(agent skills)")),
        "Intra-project symlink to .agents/skills must still prompt: {:?}",
        event.tool_call.fields.title,
    );
}

#[gpui::test]
async fn test_streaming_authorize_create_under_symlink_with_allow(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = project::FakeFs::new(cx.executor());
    fs.insert_tree("/root", json!({})).await;
    fs.insert_tree("/outside", json!({})).await;
    fs.insert_symlink("/root/link", PathBuf::from("/outside"))
        .await;
    let (edit_tool, _project, _action_log, _fs, _thread) =
        setup_test_with_fs(cx, fs, &[path!("/root").as_ref()]).await;

    cx.update(|cx| {
        let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
        settings.tool_permissions.default = settings::ToolPermissionMode::Allow;
        agent_settings::AgentSettings::override_global(settings, cx);
    });

    let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
    let authorize_task =
        cx.update(|cx| edit_tool.authorize(&PathBuf::from("link/new.txt"), &stream_tx, cx));

    let event = stream_rx.expect_authorization().await;
    assert!(
        event
            .tool_call
            .fields
            .title
            .as_deref()
            .is_some_and(|title| title.contains("points outside the project")),
        "Expected symlink escape authorization for create under external symlink"
    );

    event
        .response
        .send(acp_thread::SelectedPermissionOutcome::new(
            acp::PermissionOptionId::new("allow"),
            acp::PermissionOptionKind::AllowOnce,
        ))
        .unwrap();
    authorize_task.await.unwrap();
}

#[gpui::test]
async fn test_streaming_edit_file_symlink_escape_requests_authorization(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = project::FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "src": { "main.rs": "fn main() {}" }
        }),
    )
    .await;
    fs.insert_tree(
        path!("/outside"),
        json!({
            "config.txt": "old content"
        }),
    )
    .await;
    fs.create_symlink(
        path!("/root/link_to_external").as_ref(),
        PathBuf::from("/outside"),
    )
    .await
    .unwrap();
    let (edit_tool, _project, _action_log, _fs, _thread) =
        setup_test_with_fs(cx, fs, &[path!("/root").as_ref()]).await;

    let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
    let _authorize_task = cx.update(|cx| {
        edit_tool.authorize(
            &PathBuf::from("link_to_external/config.txt"),
            &stream_tx,
            cx,
        )
    });

    let auth = stream_rx.expect_authorization().await;
    let title = auth.tool_call.fields.title.as_deref().unwrap_or("");
    assert!(
        title.contains("points outside the project"),
        "title should mention symlink escape, got: {title}"
    );
}

#[gpui::test]
async fn test_streaming_edit_file_symlink_escape_denied(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = project::FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "src": { "main.rs": "fn main() {}" }
        }),
    )
    .await;
    fs.insert_tree(
        path!("/outside"),
        json!({
            "config.txt": "old content"
        }),
    )
    .await;
    fs.create_symlink(
        path!("/root/link_to_external").as_ref(),
        PathBuf::from("/outside"),
    )
    .await
    .unwrap();
    let (edit_tool, _project, _action_log, _fs, _thread) =
        setup_test_with_fs(cx, fs, &[path!("/root").as_ref()]).await;

    let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
    let authorize_task = cx.update(|cx| {
        edit_tool.authorize(
            &PathBuf::from("link_to_external/config.txt"),
            &stream_tx,
            cx,
        )
    });

    let auth = stream_rx.expect_authorization().await;
    drop(auth); // deny by dropping

    let result = authorize_task.await;
    assert!(result.is_err(), "should fail when denied");
}

#[gpui::test]
async fn test_streaming_edit_file_symlink_escape_honors_deny_policy(cx: &mut TestAppContext) {
    init_test(cx);
    cx.update(|cx| {
        let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
        settings.tool_permissions.tools.insert(
            "edit_file".into(),
            agent_settings::ToolRules {
                default: Some(settings::ToolPermissionMode::Deny),
                ..Default::default()
            },
        );
        agent_settings::AgentSettings::override_global(settings, cx);
    });

    let fs = project::FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "src": { "main.rs": "fn main() {}" }
        }),
    )
    .await;
    fs.insert_tree(
        path!("/outside"),
        json!({
            "config.txt": "old content"
        }),
    )
    .await;
    fs.create_symlink(
        path!("/root/link_to_external").as_ref(),
        PathBuf::from("/outside"),
    )
    .await
    .unwrap();
    let (edit_tool, _project, _action_log, _fs, _thread) =
        setup_test_with_fs(cx, fs, &[path!("/root").as_ref()]).await;

    let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
    let result = cx
        .update(|cx| {
            edit_tool.authorize(
                &PathBuf::from("link_to_external/config.txt"),
                &stream_tx,
                cx,
            )
        })
        .await;

    assert!(result.is_err(), "Tool should fail when policy denies");
    assert!(
        !matches!(
            stream_rx.try_recv(),
            Ok(Ok(crate::ThreadEvent::ToolCallAuthorization(_)))
        ),
        "Deny policy should not emit symlink authorization prompt",
    );
}

#[gpui::test]
async fn test_streaming_authorize_global_config(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = project::FakeFs::new(cx.executor());
    fs.insert_tree("/project", json!({})).await;
    let (edit_tool, _project, _action_log, _fs, _thread) =
        setup_test_with_fs(cx, fs, &[path!("/project").as_ref()]).await;

    let test_cases = vec![
        (
            "/etc/hosts",
            true,
            "System file should require confirmation",
        ),
        (
            "/usr/local/bin/script",
            true,
            "System bin file should require confirmation",
        ),
        (
            "project/normal_file.rs",
            false,
            "Normal project file should not require confirmation",
        ),
    ];

    for (path, should_confirm, description) in test_cases {
        let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
        let auth = cx.update(|cx| edit_tool.authorize(&PathBuf::from(path), &stream_tx, cx));

        if should_confirm {
            stream_rx.expect_authorization().await;
        } else {
            auth.await.unwrap();
            assert!(
                stream_rx.try_recv().is_err(),
                "Failed for case: {} - path: {} - expected no confirmation but got one",
                description,
                path
            );
        }
    }
}
