use super::*;

#[gpui::test]
async fn test_read_directory_path(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "some_dir": {}
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let tool = Arc::new(ReadFileTool::new(project, action_log, true));
    let (event_stream, _) = ToolCallEventStream::test();

    let result = cx
        .update(|cx| {
            let input = ReadFileToolInput {
                path: "root/some_dir".to_string(),
                start_line: None,
                end_line: None,
            };
            tool.run(ToolInput::resolved(input), event_stream, cx)
        })
        .await;
    assert_eq!(
        error_text(result.unwrap_err()),
        "root/some_dir is a directory, not a file. Use the list_directory tool to explore directory contents."
    );
}

#[gpui::test]
async fn test_read_nonexistent_file(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/root"), json!({})).await;
    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let tool = Arc::new(ReadFileTool::new(project, action_log, true));
    let (event_stream, _) = ToolCallEventStream::test();

    let result = cx
        .update(|cx| {
            let input = ReadFileToolInput {
                path: "root/nonexistent_file.txt".to_string(),
                start_line: None,
                end_line: None,
            };
            tool.run(ToolInput::resolved(input), event_stream, cx)
        })
        .await;
    assert_eq!(
        error_text(result.unwrap_err()),
        "root/nonexistent_file.txt not found"
    );
}

#[gpui::test]
async fn test_read_small_file(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "small_file.txt": "This is a small file content"
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let tool = Arc::new(ReadFileTool::new(project, action_log, true));
    let result = cx
        .update(|cx| {
            let input = ReadFileToolInput {
                path: "root/small_file.txt".into(),
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
    assert_eq!(
        result.unwrap(),
        "     1\tThis is a small file content".into()
    );
}

#[gpui::test]
async fn test_read_large_file(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "large_file.rs": (0..1000).map(|i| format!("struct Test{} {{\n    a: u32,\n    b: usize,\n}}", i)).collect::<Vec<_>>().join("\n")
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(language::rust_lang());
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let tool = Arc::new(ReadFileTool::new(project, action_log, true));
    let result = cx
        .update(|cx| {
            let input = ReadFileToolInput {
                path: "root/large_file.rs".into(),
                start_line: None,
                end_line: None,
            };
            tool.clone().run(
                ToolInput::resolved(input),
                ToolCallEventStream::test().0,
                cx,
            )
        })
        .await
        .unwrap();
    let content = result.to_str().unwrap();

    assert_eq!(
        content.lines().skip(7).take(6).collect::<Vec<_>>(),
        vec![
            "struct Test0 [L1-4]",
            " a [L2]",
            " b [L3]",
            "struct Test1 [L5-8]",
            " a [L6]",
            " b [L7]",
        ]
    );

    let result = cx
        .update(|cx| {
            let input = ReadFileToolInput {
                path: "root/large_file.rs".into(),
                start_line: None,
                end_line: None,
            };
            tool.run(
                ToolInput::resolved(input),
                ToolCallEventStream::test().0,
                cx,
            )
        })
        .await
        .unwrap();
    let content = result.to_str().unwrap();
    let expected_content = (0..1000)
        .flat_map(|i| {
            vec![
                format!("struct Test{} [L{}-{}]", i, i * 4 + 1, i * 4 + 4),
                format!(" a [L{}]", i * 4 + 2),
                format!(" b [L{}]", i * 4 + 3),
            ]
        })
        .collect::<Vec<_>>();
    pretty_assertions::assert_eq!(
        content
            .lines()
            .skip(7)
            .take(expected_content.len())
            .collect::<Vec<_>>(),
        expected_content
    );
}

// The outline returned for a large file is not valid source for the file's
// language, so the UI-side markdown wrapping must omit the path tag.
// Otherwise the markdown renderer routes the fenced block through
// `CodeBlockKind::FencedSrc`, resolves the file's language, and runs
// tree-sitter against pseudo-code outline text on every paint.
#[gpui::test]
async fn test_outline_response_uses_untagged_code_block(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "large_file.rs": (0..1000).map(|i| format!("struct Test{} {{\n    a: u32,\n    b: usize,\n}}", i)).collect::<Vec<_>>().join("\n")
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(language::rust_lang());
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let tool = Arc::new(ReadFileTool::new(project, action_log, true));
    let (event_stream, mut rx) = ToolCallEventStream::test();

    let result = cx
        .update(|cx| {
            let input = ReadFileToolInput {
                path: "root/large_file.rs".into(),
                start_line: None,
                end_line: None,
            };
            tool.clone()
                .run(ToolInput::resolved(input), event_stream, cx)
        })
        .await
        .unwrap();

    // Sanity-check: the file is large enough to trigger the outline branch.
    assert!(
        result
            .to_str()
            .unwrap()
            .starts_with("SUCCESS: File outline retrieved."),
        "expected outline response, got: {:?}",
        result.to_str().unwrap()
    );

    // The first update carries the location; the second carries the
    // markdown content destined for the tool-call UI.
    let _location_update = rx.expect_update_fields().await;
    let content_update = rx.expect_update_fields().await;
    let content_blocks = content_update.content.expect("expected content update");
    let acp::ToolCallContent::Content(content) = content_blocks
        .first()
        .expect("expected at least one content block")
    else {
        panic!("expected ContentBlock, got {:?}", content_blocks.first());
    };
    let acp::ContentBlock::Text(text) = &content.content else {
        panic!("expected text content block, got {:?}", content.content);
    };

    assert!(
        text.text.starts_with("```\n"),
        "outline response must use an untagged fenced code block; got first line: {:?}",
        text.text.lines().next()
    );
    assert!(
        !text.text.starts_with("```root/"),
        "outline response must not include the file path as a code block tag"
    );
}

// The full-file (non-outline) response should still tag the code block
// with the file path so the markdown renderer can resolve the file's
// language for syntax highlighting.
#[gpui::test]
async fn test_full_file_response_keeps_path_tag(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "small_file.rs": "fn main() {}"
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let tool = Arc::new(ReadFileTool::new(project, action_log, true));
    let (event_stream, mut rx) = ToolCallEventStream::test();

    cx.update(|cx| {
        let input = ReadFileToolInput {
            path: "root/small_file.rs".into(),
            start_line: None,
            end_line: None,
        };
        tool.clone()
            .run(ToolInput::resolved(input), event_stream, cx)
    })
    .await
    .unwrap();

    let _location_update = rx.expect_update_fields().await;
    let content_update = rx.expect_update_fields().await;
    let content_blocks = content_update.content.expect("expected content update");
    let acp::ToolCallContent::Content(content) = content_blocks
        .first()
        .expect("expected at least one content block")
    else {
        panic!("expected ContentBlock, got {:?}", content_blocks.first());
    };
    let acp::ContentBlock::Text(text) = &content.content else {
        panic!("expected text content block, got {:?}", content.content);
    };

    assert!(
        text.text.starts_with("```root/small_file.rs\n"),
        "full-file response must tag the code block with the file path; got first line: {:?}",
        text.text.lines().next()
    );
}

// When a worktree is named "foo" and contains a subdirectory also named "foo",
// read_file({"path": "foo/test.txt"}) should return the file at the worktree
// root (as the tool schema promises), not the one inside the foo/ subdirectory.
#[gpui::test]
async fn test_read_file_worktree_root_not_shadowed_by_subdir(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/foo"),
        json!({
            "test.txt": "root content",
            "foo": {
                "test.txt": "subdir content"
            }
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/foo").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let tool = Arc::new(ReadFileTool::new(project, action_log, true));

    // The tool schema says the first component must be the worktree root name,
    // so "foo/test.txt" means test.txt at the root of the "foo" worktree.
    let result = cx
        .update(|cx| {
            let input = ReadFileToolInput {
                path: "foo/test.txt".into(),
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
    assert_eq!(result.unwrap(), "     1\troot content".into());
}

#[gpui::test]
async fn test_read_file_with_line_range(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "multiline.txt": "Line 1\nLine 2\nLine 3\nLine 4\nLine 5"
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;

    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let tool = Arc::new(ReadFileTool::new(project, action_log, true));
    let result = cx
        .update(|cx| {
            let input = ReadFileToolInput {
                path: "root/multiline.txt".to_string(),
                start_line: Some(2),
                end_line: Some(4),
            };
            tool.run(
                ToolInput::resolved(input),
                ToolCallEventStream::test().0,
                cx,
            )
        })
        .await;
    assert_eq!(
        result.unwrap(),
        "     2\tLine 2\n     3\tLine 3\n     4\tLine 4\n".into()
    );
}

#[gpui::test]
async fn test_read_file_line_range_edge_cases(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "multiline.txt": "Line 1\nLine 2\nLine 3\nLine 4\nLine 5"
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let tool = Arc::new(ReadFileTool::new(project, action_log, true));

    // start_line of 0 should be treated as 1
    let result = cx
        .update(|cx| {
            let input = ReadFileToolInput {
                path: "root/multiline.txt".to_string(),
                start_line: Some(0),
                end_line: Some(2),
            };
            tool.clone().run(
                ToolInput::resolved(input),
                ToolCallEventStream::test().0,
                cx,
            )
        })
        .await;
    assert_eq!(result.unwrap(), "     1\tLine 1\n     2\tLine 2\n".into());

    // end_line of 0 should result in at least 1 line
    let result = cx
        .update(|cx| {
            let input = ReadFileToolInput {
                path: "root/multiline.txt".to_string(),
                start_line: Some(1),
                end_line: Some(0),
            };
            tool.clone().run(
                ToolInput::resolved(input),
                ToolCallEventStream::test().0,
                cx,
            )
        })
        .await;
    assert_eq!(result.unwrap(), "     1\tLine 1\n".into());

    // when start_line > end_line, should still return at least 1 line
    let result = cx
        .update(|cx| {
            let input = ReadFileToolInput {
                path: "root/multiline.txt".to_string(),
                start_line: Some(3),
                end_line: Some(2),
            };
            tool.clone().run(
                ToolInput::resolved(input),
                ToolCallEventStream::test().0,
                cx,
            )
        })
        .await;
    assert_eq!(result.unwrap(), "     3\tLine 3\n".into());
}
