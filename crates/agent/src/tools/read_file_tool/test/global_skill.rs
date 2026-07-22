use super::*;

#[gpui::test]
async fn test_read_global_skill_file(cx: &mut TestAppContext) {
    init_test(cx);

    // Set up a project that does NOT contain the skills tree, plus a
    // global skill file outside the worktree.
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "src": { "main.rs": "fn main() {}" }
        }),
    )
    .await;

    let skill_md_path = agent_skills::global_skills_dir()
        .join("my-skill")
        .join("references")
        .join("spec.md");
    fs.create_dir(skill_md_path.parent().unwrap())
        .await
        .unwrap();
    fs.insert_file(&skill_md_path, b"# Spec\n\nReference body.".to_vec())
        .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let tool = Arc::new(ReadFileTool::new(project, action_log, true));

    let result = cx
        .update(|cx| {
            let input = ReadFileToolInput {
                path: skill_md_path.to_string_lossy().into_owned(),
                start_line: None,
                end_line: None,
            };
            tool.run(
                ToolInput::resolved(input),
                ToolCallEventStream::test().0,
                cx,
            )
        })
        .await;

    let content = result.unwrap();
    let LanguageModelToolResultContent::Text(text) = content else {
        panic!("expected text content");
    };
    assert_eq!(
        text.as_ref(),
        "     1\t# Spec\n     2\t\n     3\tReference body."
    );
}

#[gpui::test]
async fn test_read_global_skill_file_with_line_range(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/root"), json!({})).await;

    let skill_md_path = agent_skills::global_skills_dir()
        .join("my-skill")
        .join("references")
        .join("long.md");
    fs.create_dir(skill_md_path.parent().unwrap())
        .await
        .unwrap();
    fs.insert_file(
        &skill_md_path,
        b"line one\nline two\nline three\nline four\n".to_vec(),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let tool = Arc::new(ReadFileTool::new(project, action_log, true));

    let result = cx
        .update(|cx| {
            let input = ReadFileToolInput {
                path: skill_md_path.to_string_lossy().into_owned(),
                start_line: Some(2),
                end_line: Some(3),
            };
            tool.run(
                ToolInput::resolved(input),
                ToolCallEventStream::test().0,
                cx,
            )
        })
        .await;

    let LanguageModelToolResultContent::Text(text) = result.unwrap() else {
        panic!("expected text content");
    };
    // Mirrors the buffer-backed path: lines 2-3 inclusive, WITH trailing
    // newline of the last returned line.
    assert_eq!(text.as_ref(), "     2\tline two\n     3\tline three\n");
}

#[gpui::test]
async fn test_read_global_skill_file_line_range_zero_start(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/root"), json!({})).await;

    let skill_md_path = agent_skills::global_skills_dir()
        .join("my-skill")
        .join("references")
        .join("long.md");
    fs.create_dir(skill_md_path.parent().unwrap())
        .await
        .unwrap();
    fs.insert_file(
        &skill_md_path,
        b"Line 1\nLine 2\nLine 3\nLine 4\nLine 5".to_vec(),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let tool = Arc::new(ReadFileTool::new(project, action_log, true));

    let result = cx
        .update(|cx| {
            let input = ReadFileToolInput {
                path: skill_md_path.to_string_lossy().into_owned(),
                start_line: Some(0),
                end_line: Some(2),
            };
            tool.run(
                ToolInput::resolved(input),
                ToolCallEventStream::test().0,
                cx,
            )
        })
        .await;

    let LanguageModelToolResultContent::Text(text) = result.unwrap() else {
        panic!("expected text content");
    };
    assert_eq!(text.as_ref(), "     1\tLine 1\n     2\tLine 2\n");
}

#[gpui::test]
async fn test_read_global_skill_file_line_range_zero_end(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/root"), json!({})).await;

    let skill_md_path = agent_skills::global_skills_dir()
        .join("my-skill")
        .join("references")
        .join("long.md");
    fs.create_dir(skill_md_path.parent().unwrap())
        .await
        .unwrap();
    fs.insert_file(
        &skill_md_path,
        b"Line 1\nLine 2\nLine 3\nLine 4\nLine 5".to_vec(),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let tool = Arc::new(ReadFileTool::new(project, action_log, true));

    let result = cx
        .update(|cx| {
            let input = ReadFileToolInput {
                path: skill_md_path.to_string_lossy().into_owned(),
                start_line: Some(1),
                end_line: Some(0),
            };
            tool.run(
                ToolInput::resolved(input),
                ToolCallEventStream::test().0,
                cx,
            )
        })
        .await;

    let LanguageModelToolResultContent::Text(text) = result.unwrap() else {
        panic!("expected text content");
    };
    assert_eq!(text.as_ref(), "     1\tLine 1\n");
}

#[gpui::test]
async fn test_read_global_skill_file_line_range_inverted(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/root"), json!({})).await;

    let skill_md_path = agent_skills::global_skills_dir()
        .join("my-skill")
        .join("references")
        .join("long.md");
    fs.create_dir(skill_md_path.parent().unwrap())
        .await
        .unwrap();
    fs.insert_file(
        &skill_md_path,
        b"Line 1\nLine 2\nLine 3\nLine 4\nLine 5".to_vec(),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let tool = Arc::new(ReadFileTool::new(project, action_log, true));

    let result = cx
        .update(|cx| {
            let input = ReadFileToolInput {
                path: skill_md_path.to_string_lossy().into_owned(),
                start_line: Some(3),
                end_line: Some(2),
            };
            tool.run(
                ToolInput::resolved(input),
                ToolCallEventStream::test().0,
                cx,
            )
        })
        .await;

    let LanguageModelToolResultContent::Text(text) = result.unwrap() else {
        panic!("expected text content");
    };
    assert_eq!(text.as_ref(), "     3\tLine 3\n");
}

#[gpui::test]
async fn test_read_global_skill_file_line_range_crlf(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/root"), json!({})).await;

    let skill_md_path = agent_skills::global_skills_dir()
        .join("my-skill")
        .join("references")
        .join("long.md");
    fs.create_dir(skill_md_path.parent().unwrap())
        .await
        .unwrap();
    fs.insert_file(
        &skill_md_path,
        b"line one\r\nline two\r\nline three\r\n".to_vec(),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let tool = Arc::new(ReadFileTool::new(project, action_log, true));

    let result = cx
        .update(|cx| {
            let input = ReadFileToolInput {
                path: skill_md_path.to_string_lossy().into_owned(),
                start_line: Some(1),
                end_line: Some(2),
            };
            tool.run(
                ToolInput::resolved(input),
                ToolCallEventStream::test().0,
                cx,
            )
        })
        .await;

    let LanguageModelToolResultContent::Text(text) = result.unwrap() else {
        panic!("expected text content");
    };
    assert_eq!(text.as_ref(), "     1\tline one\r\n     2\tline two\r\n");
}

#[gpui::test]
async fn test_read_outside_skills_dir_still_rejected(cx: &mut TestAppContext) {
    init_test(cx);

    // A path that's neither in the worktree nor under the global skills
    // dir should still fail — the fast path is gated, not a backdoor for
    // arbitrary external reads.
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/root"), json!({})).await;
    fs.create_dir(path!("/etc").as_ref()).await.unwrap();
    fs.insert_file(path!("/etc/secret"), b"top secret".to_vec())
        .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let tool = Arc::new(ReadFileTool::new(project, action_log, true));

    let result = cx
        .update(|cx| {
            let input = ReadFileToolInput {
                path: path!("/etc/secret").to_string(),
                start_line: None,
                end_line: None,
            };
            tool.run(
                ToolInput::resolved(input),
                ToolCallEventStream::test().0,
                cx,
            )
        })
        .await;

    assert!(
        result.is_err(),
        "path outside skills dir should be rejected"
    );
}
