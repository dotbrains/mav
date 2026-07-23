use super::*;
use fs::Fs as _;
use gpui::TestAppContext;
use project::{FakeFs, Project};
use serde_json::json;
use settings::SettingsStore;
use std::path::PathBuf;
use util::path;

use crate::ToolCallEventStream;

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
async fn test_delete_path_global_skill_directory(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/root/project"), json!({})).await;
    let skills_dir = agent_skills::global_skills_dir();
    let skill_dir = skills_dir.join("my-skill");
    fs.insert_tree(&skill_dir, json!({ "SKILL.md": "content" }))
        .await;
    let project = Project::test(fs.clone(), [path!("/root/project").as_ref()], cx).await;
    cx.executor().run_until_parked();

    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let tool = Arc::new(DeletePathTool::new(project, action_log));
    let input_path = PathBuf::from("~")
        .join(".agents")
        .join("skills")
        .join("my-skill")
        .to_string_lossy()
        .into_owned();

    let (event_stream, mut event_rx) = ToolCallEventStream::test();
    let task = cx.update(|cx| {
        tool.run(
            ToolInput::resolved(DeletePathToolInput { path: input_path }),
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
    assert!(result.is_ok(), "should delete after approval: {result:?}");
    assert!(fs.is_dir(&skills_dir).await);
    assert!(!fs.is_dir(&skill_dir).await);
}

#[gpui::test]
async fn test_delete_path_global_skill_file(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/root/project"), json!({})).await;
    let skill_file = agent_skills::global_skills_dir()
        .join("my-skill")
        .join("references")
        .join("notes.md");
    fs.create_dir(skill_file.parent().unwrap()).await.unwrap();
    fs.insert_file(&skill_file, b"notes".to_vec()).await;
    let project = Project::test(fs.clone(), [path!("/root/project").as_ref()], cx).await;
    cx.executor().run_until_parked();

    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let tool = Arc::new(DeletePathTool::new(project, action_log));
    let input_path = PathBuf::from("~")
        .join(".agents")
        .join("skills")
        .join("my-skill")
        .join("references")
        .join("notes.md")
        .to_string_lossy()
        .into_owned();

    let (event_stream, mut event_rx) = ToolCallEventStream::test();
    let task = cx.update(|cx| {
        tool.run(
            ToolInput::resolved(DeletePathToolInput { path: input_path }),
            event_stream,
            cx,
        )
    });

    let auth = event_rx.expect_authorization().await;
    auth.response
        .send(acp_thread::SelectedPermissionOutcome::new(
            acp::PermissionOptionId::new("allow"),
            acp::PermissionOptionKind::AllowOnce,
        ))
        .expect("authorization response should send");

    let result = task.await;
    assert!(result.is_ok(), "should delete after approval: {result:?}");
    assert!(!fs.is_file(&skill_file).await);
}

#[gpui::test]
async fn test_delete_path_rejects_global_skills_root(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/root/project"), json!({})).await;
    let skills_dir = agent_skills::global_skills_dir();
    fs.create_dir(&skills_dir).await.unwrap();
    let project = Project::test(fs.clone(), [path!("/root/project").as_ref()], cx).await;
    cx.executor().run_until_parked();

    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let tool = Arc::new(DeletePathTool::new(project, action_log));
    let input_path = PathBuf::from("~")
        .join(".agents")
        .join("skills")
        .to_string_lossy()
        .into_owned();

    let (event_stream, mut event_rx) = ToolCallEventStream::test();
    let result = cx
        .update(|cx| {
            tool.run(
                ToolInput::resolved(DeletePathToolInput { path: input_path }),
                event_stream,
                cx,
            )
        })
        .await;

    assert!(result.is_err(), "should reject deleting skills root");
    assert!(fs.is_dir(&skills_dir).await);
    assert!(
        !matches!(
            event_rx.try_recv(),
            Ok(Ok(crate::ThreadEvent::ToolCallAuthorization(_)))
        ),
        "Deleting the skills root should fail before requesting authorization",
    );
}

#[gpui::test]
async fn test_delete_path_symlink_escape_requests_authorization(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "project": {
                "src": { "main.rs": "fn main() {}" }
            },
            "external": {
                "data": { "file.txt": "content" }
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

    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let tool = Arc::new(DeletePathTool::new(project, action_log));

    let (event_stream, mut event_rx) = ToolCallEventStream::test();
    let task = cx.update(|cx| {
        tool.run(
            ToolInput::resolved(DeletePathToolInput {
                path: "project/link_to_external".into(),
            }),
            event_stream,
            cx,
        )
    });

    let auth = event_rx.expect_authorization().await;
    let title = auth.tool_call.fields.title.as_deref().unwrap_or("");
    assert!(
        title.contains("points outside the project") || title.contains("symlink"),
        "Authorization title should mention symlink escape, got: {title}",
    );

    auth.response
        .send(acp_thread::SelectedPermissionOutcome::new(
            acp::PermissionOptionId::new("allow"),
            acp::PermissionOptionKind::AllowOnce,
        ))
        .unwrap();

    let result = task.await;
    // FakeFs cannot delete symlink entries (they are neither Dir nor File
    // internally), so the deletion itself may fail. The important thing is
    // that the authorization was requested and accepted — any error must
    // come from the fs layer, not from a permission denial.
    if let Err(err) = &result {
        let msg = format!("{err:#}");
        assert!(
            !msg.contains("denied") && !msg.contains("authorization"),
            "Error should not be a permission denial, got: {msg}",
        );
    }
}

#[gpui::test]
async fn test_delete_path_symlink_escape_denied(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "project": {
                "src": { "main.rs": "fn main() {}" }
            },
            "external": {
                "data": { "file.txt": "content" }
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

    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let tool = Arc::new(DeletePathTool::new(project, action_log));

    let (event_stream, mut event_rx) = ToolCallEventStream::test();
    let task = cx.update(|cx| {
        tool.run(
            ToolInput::resolved(DeletePathToolInput {
                path: "project/link_to_external".into(),
            }),
            event_stream,
            cx,
        )
    });

    let auth = event_rx.expect_authorization().await;

    drop(auth);

    let result = task.await;
    assert!(
        result.is_err(),
        "Tool should fail when authorization is denied"
    );
}

#[gpui::test]
async fn test_delete_path_symlink_escape_confirm_requires_single_approval(cx: &mut TestAppContext) {
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
                "src": { "main.rs": "fn main() {}" }
            },
            "external": {
                "data": { "file.txt": "content" }
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

    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let tool = Arc::new(DeletePathTool::new(project, action_log));

    let (event_stream, mut event_rx) = ToolCallEventStream::test();
    let task = cx.update(|cx| {
        tool.run(
            ToolInput::resolved(DeletePathToolInput {
                path: "project/link_to_external".into(),
            }),
            event_stream,
            cx,
        )
    });

    let auth = event_rx.expect_authorization().await;
    let title = auth.tool_call.fields.title.as_deref().unwrap_or("");
    assert!(
        title.contains("points outside the project") || title.contains("symlink"),
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
    if let Err(err) = &result {
        let message = format!("{err:#}");
        assert!(
            !message.contains("denied") && !message.contains("authorization"),
            "Error should not be a permission denial, got: {message}",
        );
    }
}

#[gpui::test]
async fn test_delete_path_symlink_escape_honors_deny_policy(cx: &mut TestAppContext) {
    init_test(cx);
    cx.update(|cx| {
        let mut settings = AgentSettings::get_global(cx).clone();
        settings.tool_permissions.tools.insert(
            "delete_path".into(),
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
                "src": { "main.rs": "fn main() {}" }
            },
            "external": {
                "data": { "file.txt": "content" }
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

    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let tool = Arc::new(DeletePathTool::new(project, action_log));

    let (event_stream, mut event_rx) = ToolCallEventStream::test();
    let result = cx
        .update(|cx| {
            tool.run(
                ToolInput::resolved(DeletePathToolInput {
                    path: "project/link_to_external".into(),
                }),
                event_stream,
                cx,
            )
        })
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
