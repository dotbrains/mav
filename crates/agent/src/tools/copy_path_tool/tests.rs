use super::*;
use fs::Fs as _;
use gpui::TestAppContext;
use project::{FakeFs, Project};
use serde_json::json;
use settings::SettingsStore;
use std::path::PathBuf;
use util::path;

fn init_test(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
    });
    cx.update(|cx| {
        let mut settings = AgentSettings::get_global(cx).clone();
        settings.tool_permissions.default = settings::ToolPermissionMode::Allow;
        AgentSettings::override_global(settings, cx);
    });
}

#[gpui::test]
async fn test_copy_path_global_skill_directory_to_project(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/root/project"), json!({})).await;
    let skill_dir = agent_skills::global_skills_dir().join("my-skill");
    fs.insert_tree(&skill_dir, json!({ "SKILL.md": "content" }))
        .await;
    let project = Project::test(fs.clone(), [path!("/root/project").as_ref()], cx).await;
    cx.executor().run_until_parked();

    let tool = Arc::new(CopyPathTool::new(project));
    let input_path = PathBuf::from("~")
        .join(".agents")
        .join("skills")
        .join("my-skill")
        .to_string_lossy()
        .into_owned();

    let (event_stream, mut event_rx) = ToolCallEventStream::test();
    let task = cx.update(|cx| {
        tool.run(
            ToolInput::resolved(CopyPathToolInput {
                source_path: input_path,
                destination_path: path!("/root/project/my-skill").to_string(),
            }),
            event_stream,
            cx,
        )
    });

    let auth = event_rx.expect_authorization().await;
    let title = auth.tool_call.fields.title.as_deref().unwrap_or("");
    assert!(
        title.contains("agent skills"),
        "Authorization title should mention agent skills, got: {title}",
    );
    assert!(
        auth.options
            .first_option_of_kind(acp::PermissionOptionKind::AllowAlways)
            .is_none(),
        "agent skills prompt must not offer an \"Always allow\" option: {:?}",
        auth.options,
    );
    auth.response
        .send(acp_thread::SelectedPermissionOutcome::new(
            acp::PermissionOptionId::new("allow"),
            acp::PermissionOptionKind::AllowOnce,
        ))
        .expect("authorization response should send");

    let result = task.await;
    assert!(result.is_ok(), "should copy after approval: {result:?}");
    assert!(fs.is_dir(&skill_dir).await);
    assert_eq!(
        fs.load(path!("/root/project/my-skill/SKILL.md").as_ref())
            .await
            .unwrap(),
        "content"
    );
}

#[gpui::test]
async fn test_copy_path_project_directory_to_global_skill_directory(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root/project"),
        json!({ "exported-skill": { "SKILL.md": "content" } }),
    )
    .await;
    let skills_dir = agent_skills::global_skills_dir();
    fs.create_dir(&skills_dir).await.unwrap();
    let project = Project::test(fs.clone(), [path!("/root/project").as_ref()], cx).await;
    cx.executor().run_until_parked();

    let tool = Arc::new(CopyPathTool::new(project));
    let destination_path = PathBuf::from("~")
        .join(".agents")
        .join("skills")
        .join("exported-skill")
        .to_string_lossy()
        .into_owned();

    let (event_stream, mut event_rx) = ToolCallEventStream::test();
    let task = cx.update(|cx| {
        tool.run(
            ToolInput::resolved(CopyPathToolInput {
                source_path: path!("/root/project/exported-skill").to_string(),
                destination_path,
            }),
            event_stream,
            cx,
        )
    });

    let auth = event_rx.expect_authorization().await;
    let title = auth.tool_call.fields.title.as_deref().unwrap_or("");
    assert!(
        title.contains("agent skills"),
        "Authorization title should mention agent skills, got: {title}",
    );
    assert!(
        auth.options
            .first_option_of_kind(acp::PermissionOptionKind::AllowAlways)
            .is_none(),
        "agent skills prompt must not offer an \"Always allow\" option: {:?}",
        auth.options,
    );
    auth.response
        .send(acp_thread::SelectedPermissionOutcome::new(
            acp::PermissionOptionId::new("allow"),
            acp::PermissionOptionKind::AllowOnce,
        ))
        .expect("authorization response should send");

    let result = task.await;
    assert!(result.is_ok(), "should copy after approval: {result:?}");
    assert!(
        fs.is_dir(path!("/root/project/exported-skill").as_ref())
            .await
    );
    assert_eq!(
        fs.load(skills_dir.join("exported-skill").join("SKILL.md").as_ref())
            .await
            .unwrap(),
        "content"
    );
}

#[gpui::test]
async fn test_copy_path_symlink_escape_source_requests_authorization(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "project": {
                "src": { "file.txt": "content" }
            },
            "external": {
                "secret.txt": "SECRET"
            }
        }),
    )
    .await;

    fs.create_symlink(
        path!("/root/project/link_to_external").as_ref(),
        PathBuf::from("../external"),
    )
    .await
    .unwrap();

    let project = Project::test(fs.clone(), [path!("/root/project").as_ref()], cx).await;
    cx.executor().run_until_parked();

    let tool = Arc::new(CopyPathTool::new(project));

    let input = CopyPathToolInput {
        source_path: "project/link_to_external".into(),
        destination_path: "project/external_copy".into(),
    };

    let (event_stream, mut event_rx) = ToolCallEventStream::test();
    let task = cx.update(|cx| tool.run(ToolInput::resolved(input), event_stream, cx));

    let auth = event_rx.expect_authorization().await;
    let title = auth.tool_call.fields.title.as_deref().unwrap_or("");
    assert!(
        title.contains("points outside the project") || title.contains("symlinks outside project"),
        "Authorization title should mention symlink escape, got: {title}",
    );

    auth.response
        .send(acp_thread::SelectedPermissionOutcome::new(
            acp::PermissionOptionId::new("allow"),
            acp::PermissionOptionKind::AllowOnce,
        ))
        .unwrap();

    let result = task.await;
    assert!(result.is_ok(), "should succeed after approval: {result:?}");
}

#[gpui::test]
async fn test_copy_path_symlink_escape_denied(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "project": {
                "src": { "file.txt": "content" }
            },
            "external": {
                "secret.txt": "SECRET"
            }
        }),
    )
    .await;

    fs.create_symlink(
        path!("/root/project/link_to_external").as_ref(),
        PathBuf::from("../external"),
    )
    .await
    .unwrap();

    let project = Project::test(fs.clone(), [path!("/root/project").as_ref()], cx).await;
    cx.executor().run_until_parked();

    let tool = Arc::new(CopyPathTool::new(project));

    let input = CopyPathToolInput {
        source_path: "project/link_to_external".into(),
        destination_path: "project/external_copy".into(),
    };

    let (event_stream, mut event_rx) = ToolCallEventStream::test();
    let task = cx.update(|cx| tool.run(ToolInput::resolved(input), event_stream, cx));

    let auth = event_rx.expect_authorization().await;
    drop(auth);

    let result = task.await;
    assert!(result.is_err(), "should fail when denied");
}

#[gpui::test]
async fn test_copy_path_symlink_escape_confirm_requires_single_approval(cx: &mut TestAppContext) {
    init_test(cx);
    cx.update(|cx| {
        let mut settings = AgentSettings::get_global(cx).clone();
        settings.tool_permissions.default = settings::ToolPermissionMode::Confirm;
        AgentSettings::override_global(settings, cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "project": {
                "src": { "file.txt": "content" }
            },
            "external": {
                "secret.txt": "SECRET"
            }
        }),
    )
    .await;

    fs.create_symlink(
        path!("/root/project/link_to_external").as_ref(),
        PathBuf::from("../external"),
    )
    .await
    .unwrap();

    let project = Project::test(fs.clone(), [path!("/root/project").as_ref()], cx).await;
    cx.executor().run_until_parked();

    let tool = Arc::new(CopyPathTool::new(project));

    let input = CopyPathToolInput {
        source_path: "project/link_to_external".into(),
        destination_path: "project/external_copy".into(),
    };

    let (event_stream, mut event_rx) = ToolCallEventStream::test();
    let task = cx.update(|cx| tool.run(ToolInput::resolved(input), event_stream, cx));

    let auth = event_rx.expect_authorization().await;
    let title = auth.tool_call.fields.title.as_deref().unwrap_or("");
    assert!(
        title.contains("points outside the project") || title.contains("symlinks outside project"),
        "Authorization title should mention symlink escape, got: {title}",
    );

    auth.response
        .send(acp_thread::SelectedPermissionOutcome::new(
            acp::PermissionOptionId::new("allow"),
            acp::PermissionOptionKind::AllowOnce,
        ))
        .unwrap();

    assert!(
        !matches!(
            event_rx.try_recv(),
            Ok(Ok(crate::ThreadEvent::ToolCallAuthorization(_)))
        ),
        "Expected a single authorization prompt",
    );

    let result = task.await;
    assert!(
        result.is_ok(),
        "Tool should succeed after one authorization: {result:?}"
    );
}

#[gpui::test]
async fn test_copy_path_symlink_escape_honors_deny_policy(cx: &mut TestAppContext) {
    init_test(cx);
    cx.update(|cx| {
        let mut settings = AgentSettings::get_global(cx).clone();
        settings.tool_permissions.tools.insert(
            "copy_path".into(),
            agent_settings::ToolRules {
                default: Some(settings::ToolPermissionMode::Deny),
                ..Default::default()
            },
        );
        AgentSettings::override_global(settings, cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "project": {
                "src": { "file.txt": "content" }
            },
            "external": {
                "secret.txt": "SECRET"
            }
        }),
    )
    .await;

    fs.create_symlink(
        path!("/root/project/link_to_external").as_ref(),
        PathBuf::from("../external"),
    )
    .await
    .unwrap();

    let project = Project::test(fs.clone(), [path!("/root/project").as_ref()], cx).await;
    cx.executor().run_until_parked();

    let tool = Arc::new(CopyPathTool::new(project));

    let input = CopyPathToolInput {
        source_path: "project/link_to_external".into(),
        destination_path: "project/external_copy".into(),
    };

    let (event_stream, mut event_rx) = ToolCallEventStream::test();
    let result = cx
        .update(|cx| tool.run(ToolInput::resolved(input), event_stream, cx))
        .await;

    assert!(result.is_err(), "Tool should fail when policy denies");
    assert!(
        !matches!(
            event_rx.try_recv(),
            Ok(Ok(crate::ThreadEvent::ToolCallAuthorization(_)))
        ),
        "Deny policy should not emit symlink authorization prompt",
    );
}
