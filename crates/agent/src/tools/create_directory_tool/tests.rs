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
async fn test_create_directory_allows_global_skill_directory(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/root/project"), json!({})).await;
    let project = Project::test(fs.clone(), [path!("/root/project").as_ref()], cx).await;
    cx.executor().run_until_parked();

    let tool = Arc::new(CreateDirectoryTool::new(project));
    let input_path = PathBuf::from("~")
        .join(".agents")
        .join("skills")
        .join("my-skill")
        .to_string_lossy()
        .into_owned();
    let created_path = agent_skills::global_skills_dir().join("my-skill");

    let (event_stream, mut event_rx) = ToolCallEventStream::test();
    let task = cx.update(|cx| {
        tool.run(
            ToolInput::resolved(CreateDirectoryToolInput { path: input_path }),
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
    assert!(
        result.is_ok(),
        "Tool should create global skill directory: {result:?}"
    );
    assert!(fs.is_dir(&created_path).await);
}

#[gpui::test]
async fn test_create_directory_rejects_other_global_paths(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/root/project"), json!({})).await;
    let project = Project::test(fs.clone(), [path!("/root/project").as_ref()], cx).await;
    cx.executor().run_until_parked();

    let tool = Arc::new(CreateDirectoryTool::new(project));
    let outside_path = agent_skills::global_skills_dir()
        .parent()
        .expect("global skills directory should have a parent")
        .join("not-skills");

    let (event_stream, mut event_rx) = ToolCallEventStream::test();
    let result = cx
        .update(|cx| {
            tool.run(
                ToolInput::resolved(CreateDirectoryToolInput {
                    path: outside_path.to_string_lossy().into_owned(),
                }),
                event_stream,
                cx,
            )
        })
        .await;

    assert!(
        result.is_err(),
        "Tool should reject paths outside the project and global skills directory"
    );
    assert!(!fs.is_dir(&outside_path).await);
    assert!(
        !matches!(
            event_rx.try_recv(),
            Ok(Ok(crate::ThreadEvent::ToolCallAuthorization(_)))
        ),
        "Non-skill global path should not emit an agent-skills authorization prompt",
    );
}

#[gpui::test]
async fn test_create_directory_symlink_escape_requests_authorization(cx: &mut TestAppContext) {
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

    let tool = Arc::new(CreateDirectoryTool::new(project));

    let (event_stream, mut event_rx) = ToolCallEventStream::test();
    let task = cx.update(|cx| {
        tool.run(
            ToolInput::resolved(CreateDirectoryToolInput {
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
    assert!(
        result.is_ok(),
        "Tool should succeed after authorization: {result:?}"
    );
}

#[gpui::test]
async fn test_create_directory_symlink_escape_denied(cx: &mut TestAppContext) {
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

    let tool = Arc::new(CreateDirectoryTool::new(project));

    let (event_stream, mut event_rx) = ToolCallEventStream::test();
    let task = cx.update(|cx| {
        tool.run(
            ToolInput::resolved(CreateDirectoryToolInput {
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
async fn test_create_directory_symlink_escape_confirm_requires_single_approval(
    cx: &mut TestAppContext,
) {
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

    let tool = Arc::new(CreateDirectoryTool::new(project));

    let (event_stream, mut event_rx) = ToolCallEventStream::test();
    let task = cx.update(|cx| {
        tool.run(
            ToolInput::resolved(CreateDirectoryToolInput {
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
    assert!(
        result.is_ok(),
        "Tool should succeed after one authorization: {result:?}"
    );
}

#[gpui::test]
async fn test_create_directory_symlink_escape_honors_deny_policy(cx: &mut TestAppContext) {
    init_test(cx);
    cx.update(|cx| {
        let mut settings = AgentSettings::get_global(cx).clone();
        settings.tool_permissions.tools.insert(
            "create_directory".into(),
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

    let tool = Arc::new(CreateDirectoryTool::new(project));

    let (event_stream, mut event_rx) = ToolCallEventStream::test();
    let result = cx
        .update(|cx| {
            tool.run(
                ToolInput::resolved(CreateDirectoryToolInput {
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
