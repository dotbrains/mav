use super::*;

#[gpui::test]
async fn test_streaming_authorize(cx: &mut TestAppContext) {
    let (edit_tool, _project, _action_log, _fs, _thread) = setup_test(cx, json!({})).await;

    // Test 1: Path with .mav component should require confirmation
    let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
    let _auth =
        cx.update(|cx| edit_tool.authorize(&PathBuf::from(".mav/settings.json"), &stream_tx, cx));

    let event = stream_rx.expect_authorization().await;
    assert_eq!(
        event.tool_call.fields.title,
        Some("Edit `.mav/settings.json` (local settings)".into())
    );

    // Test 2: Path outside project should require confirmation
    let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
    let _auth = cx.update(|cx| edit_tool.authorize(&PathBuf::from("/etc/hosts"), &stream_tx, cx));

    let event = stream_rx.expect_authorization().await;
    assert_eq!(
        event.tool_call.fields.title,
        Some("Edit `/etc/hosts`".into())
    );

    // Test 3: Relative path without .mav should not require confirmation
    let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
    cx.update(|cx| edit_tool.authorize(&PathBuf::from("root/src/main.rs"), &stream_tx, cx))
        .await
        .unwrap();
    assert!(stream_rx.try_recv().is_err());

    // Test 4: Path with .mav in the middle should require confirmation
    let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
    let _auth =
        cx.update(|cx| edit_tool.authorize(&PathBuf::from("root/.mav/tasks.json"), &stream_tx, cx));
    let event = stream_rx.expect_authorization().await;
    assert_eq!(
        event.tool_call.fields.title,
        Some("Edit `root/.mav/tasks.json` (local settings)".into())
    );

    // Test 5: When global default is allow, sensitive and outside-project
    // paths still require confirmation
    cx.update(|cx| {
        let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
        settings.tool_permissions.default = settings::ToolPermissionMode::Allow;
        agent_settings::AgentSettings::override_global(settings, cx);
    });

    // 5.1: .mav/settings.json is a sensitive path — still prompts
    let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
    let _auth =
        cx.update(|cx| edit_tool.authorize(&PathBuf::from(".mav/settings.json"), &stream_tx, cx));
    let event = stream_rx.expect_authorization().await;
    assert_eq!(
        event.tool_call.fields.title,
        Some("Edit `.mav/settings.json` (local settings)".into())
    );

    // 5.2: /etc/hosts is outside the project, but Allow auto-approves
    let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
    cx.update(|cx| edit_tool.authorize(&PathBuf::from("/etc/hosts"), &stream_tx, cx))
        .await
        .unwrap();
    assert!(stream_rx.try_recv().is_err());

    // 5.3: Normal in-project path with allow — no confirmation needed
    let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
    cx.update(|cx| edit_tool.authorize(&PathBuf::from("root/src/main.rs"), &stream_tx, cx))
        .await
        .unwrap();
    assert!(stream_rx.try_recv().is_err());

    // 5.4: With Confirm default, non-project paths still prompt
    cx.update(|cx| {
        let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
        settings.tool_permissions.default = settings::ToolPermissionMode::Confirm;
        agent_settings::AgentSettings::override_global(settings, cx);
    });

    let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
    let _auth = cx.update(|cx| edit_tool.authorize(&PathBuf::from("/etc/hosts"), &stream_tx, cx));

    let event = stream_rx.expect_authorization().await;
    assert_eq!(
        event.tool_call.fields.title,
        Some("Edit `/etc/hosts`".into())
    );

    // 5.5: .agents/skills is a sensitive path — still prompts. The
    // sensitive-path classifier runs regardless of the default mode, so
    // it doesn't matter that we're now in Confirm mode — we're checking
    // that the path is recognized and gets the "(agent skills)" tag.
    let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
    let _auth = cx.update(|cx| {
        edit_tool.authorize(
            &PathBuf::from("root/.agents/skills/my-skill/SKILL.md"),
            &stream_tx,
            cx,
        )
    });
    let event = stream_rx.expect_authorization().await;
    assert_eq!(
        event.tool_call.fields.title,
        Some("Edit `root/.agents/skills/my-skill/SKILL.md` (agent skills)".into())
    );
    // Skills always prompt, so no "Always allow" option is offered.
    assert!(
        event
            .options
            .first_option_of_kind(acp::PermissionOptionKind::AllowAlways)
            .is_none(),
        "agent skills prompt must not offer an \"Always allow\" option: {:?}",
        event.options,
    );
    assert!(
        matches!(event.options, acp_thread::PermissionOptions::Flat(_)),
        "agent skills prompt should use flat allow/deny options: {:?}",
        event.options,
    );

    // 5.6: The global .agents/skills directory is sensitive — still prompts
    let global_skill_path = agent_skills::global_skills_dir()
        .join("my-skill")
        .join("SKILL.md");
    let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
    let _auth = cx.update(|cx| edit_tool.authorize(&global_skill_path, &stream_tx, cx));
    let event = stream_rx.expect_authorization().await;
    assert!(
        event
            .tool_call
            .fields
            .title
            .as_deref()
            .is_some_and(|title| title.ends_with("(agent skills)"))
    );
}

/// `.agents/foo/../skills/SKILL.md` would slip past the raw
/// `is_agents_skills_path` check (the components `.agents` and
/// `skills` aren't consecutive once `..` sits between them), but it
/// canonicalizes to a path inside `.agents/skills/`, so it has to
/// still prompt with the agent-skills tag.
#[gpui::test]
async fn test_streaming_authorize_blocks_dotdot_skills_bypass(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = project::FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            ".agents": {
                "foo": {},
                "skills": { "my-skill": { "SKILL.md": "target" } },
            },
        }),
    )
    .await;
    let (edit_tool, _project, _action_log, _fs, _thread) =
        setup_test_with_fs(cx, fs, &[path!("/root").as_ref()]).await;

    let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
    let _auth = cx.update(|cx| {
        edit_tool.authorize(
            &PathBuf::from(path!("/root/.agents/foo/../skills/my-skill/SKILL.md")),
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
        "`..` traversal into .agents/skills must still prompt: {:?}",
        event.tool_call.fields.title,
    );
}

/// `.mav/foo/../../safe.json` similarly sidesteps the consecutive-
/// component scan for `.mav/`, so the canonical-path recheck has to
/// catch it. (We escape *out* of `.mav/` here and back in via `..`,
/// just to confirm the recheck doesn't naively trust the raw scan.)
#[gpui::test]
async fn test_streaming_authorize_blocks_dotdot_settings_bypass(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = project::FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            ".mav": { "foo": {}, "settings.json": "{}" },
        }),
    )
    .await;
    let (edit_tool, _project, _action_log, _fs, _thread) =
        setup_test_with_fs(cx, fs, &[path!("/root").as_ref()]).await;

    let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
    let _auth = cx.update(|cx| {
        edit_tool.authorize(
            &PathBuf::from(path!("/root/.mav/foo/../settings.json")),
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
        "`..` traversal into .mav must still prompt: {:?}",
        event.tool_call.fields.title,
    );
}
