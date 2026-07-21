use super::*;

#[test]
fn test_git_output_handler_strips_ansi_codes() {
    let cases = [
        ("no escape codes here\n", "no escape codes here\n"),
        ("\x1b[31mhello\x1b[0m", "hello"),
        ("\x1b[1;32mfoo\x1b[0m bar", "foo bar"),
        ("progress 10%\rprogress 100%\n", "progress 100%\n"),
    ];

    for (input, expected) in cases {
        assert_eq!(terminal::strip_ansi_text(input.as_bytes()), expected);
    }
}

#[test]
fn test_commit_title_exceeds_limit() {
    // ASCII only
    let within_ascii = "abcde";
    let exceeds_ascii = "abcdef";
    assert!(!commit_title_exceeds_limit(within_ascii, 5));
    assert!(commit_title_exceeds_limit(exceeds_ascii, 5));

    // Multi-byte characters are counted as grapheme clusters
    let within_japanese = "あいうえお"; // 5 chars, 15 bytes
    let exceeds_japanese = "あいうえおか"; // 6 chars, 18 bytes
    assert!(!commit_title_exceeds_limit(within_japanese, 5));
    assert!(commit_title_exceeds_limit(exceeds_japanese, 5));

    // Mixed ASCII + multi-byte
    let within_mixed = "abcあ";
    let exceeds_mixed = "abcああ";
    assert!(!commit_title_exceeds_limit(within_mixed, 4));
    assert!(commit_title_exceeds_limit(exceeds_mixed, 4));

    // Emoji counts as one character each
    let within_emoji = "🚀";
    let exceeds_emoji = "🚀🚀";
    assert!(!commit_title_exceeds_limit(within_emoji, 1));
    assert!(commit_title_exceeds_limit(exceeds_emoji, 1));

    // A max_length of 0 disables the limit check
    assert!(!commit_title_exceeds_limit(
        "anything goes when disabled",
        0
    ));
    assert!(!commit_title_exceeds_limit("", 0));

    // Empty title never exceeds a positive limit
    assert!(!commit_title_exceeds_limit("", 72));
}

#[gpui::test]
async fn test_dispatch_context_with_focus_states(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/project"),
        json!({
            ".git": {},
            "tracked": "tracked\n",
        }),
    )
    .await;

    fs.set_head_and_index_for_repo(
        path!("/project/.git").as_ref(),
        &[("tracked", "old tracked\n".into())],
    );

    let project = Project::test(fs.clone(), [Path::new(path!("/project"))], cx).await;
    let window_handle =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window_handle
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window_handle.into(), cx);
    let panel = workspace.update_in(cx, GitPanel::new);

    let handle = cx.update_window_entity(&panel, |panel, _, _| {
        std::mem::replace(&mut panel.update_visible_entries_task, Task::ready(()))
    });
    cx.executor().advance_clock(2 * UPDATE_DEBOUNCE);
    handle.await;

    // Case 1: Focus the commit editor — should have "CommitEditor" but NOT "menu"/"ChangesList"
    panel.update_in(cx, |panel, window, cx| {
        panel.focus_editor(&FocusEditor, window, cx);
        let editor_is_focused = panel.commit_editor.read(cx).is_focused(window);
        assert!(
            editor_is_focused,
            "commit editor should be focused after focus_editor action"
        );
        let context = panel.dispatch_context(window, cx);
        assert!(
            context.contains("GitPanel"),
            "should always have GitPanel context"
        );
        assert!(
            context.contains("CommitEditor"),
            "should have CommitEditor context when commit editor is focused"
        );
        assert!(
            !context.contains("menu"),
            "should not have menu context when commit editor is focused"
        );
        assert!(
            !context.contains("ChangesList"),
            "should not have ChangesList context when commit editor is focused"
        );
    });

    // Case 2: Focus the panel's focus handle directly — should have "menu" and "ChangesList".
    // We force a draw via simulate_resize to ensure the dispatch tree is populated,
    // since contains_focused() depends on the rendered dispatch tree.
    panel.update_in(cx, |panel, window, cx| {
        panel.focus_handle.focus(window, cx);
    });
    cx.simulate_resize(gpui::size(px(800.), px(600.)));

    panel.update_in(cx, |panel, window, cx| {
        let context = panel.dispatch_context(window, cx);
        assert!(
            context.contains("GitPanel"),
            "should always have GitPanel context"
        );
        assert!(
            context.contains("menu"),
            "should have menu context when changes list is focused"
        );
        assert!(
            context.contains("ChangesList"),
            "should have ChangesList context when changes list is focused"
        );
        assert!(
            !context.contains("CommitEditor"),
            "should not have CommitEditor context when changes list is focused"
        );
    });

    // Case 3: Switch back to commit editor and verify context switches correctly
    panel.update_in(cx, |panel, window, cx| {
        panel.focus_editor(&FocusEditor, window, cx);
    });

    panel.update_in(cx, |panel, window, cx| {
        let context = panel.dispatch_context(window, cx);
        assert!(
            context.contains("CommitEditor"),
            "should have CommitEditor after switching focus back to editor"
        );
        assert!(
            !context.contains("menu"),
            "should not have menu after switching focus back to editor"
        );
    });

    // Case 4: Re-focus changes list and verify it transitions back correctly
    panel.update_in(cx, |panel, window, cx| {
        panel.focus_handle.focus(window, cx);
    });
    cx.simulate_resize(gpui::size(px(800.), px(600.)));

    panel.update_in(cx, |panel, window, cx| {
        assert!(
            panel.focus_handle.contains_focused(window, cx),
            "panel focus handle should report contains_focused when directly focused"
        );
        let context = panel.dispatch_context(window, cx);
        assert!(
            context.contains("menu"),
            "should have menu context after re-focusing changes list"
        );
        assert!(
            context.contains("ChangesList"),
            "should have ChangesList context after re-focusing changes list"
        );
    });
}

#[gpui::test]
async fn test_fill_commit_editor_toggle(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/root",
        json!({ "project": { ".git": {}, "src": { "main.rs": "fn main() {}" } } }),
    )
    .await;

    let project = Project::test(fs.clone(), [Path::new(path!("/root/project"))], cx).await;
    let window_handle =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window_handle
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window_handle.into(), cx);
    cx.executor().run_until_parked();

    let panel = workspace.update_in(cx, GitPanel::new);

    panel.update_in(cx, |panel, window, cx| {
        assert!(!panel.commit_editor_expanded);
        assert!(matches!(
            panel.commit_editor.read(cx).mode().clone(),
            EditorMode::AutoHeight { .. }
        ));

        panel.toggle_fill_commit_editor(&ToggleFillCommitEditor, window, cx);
        assert!(panel.commit_editor_expanded);
        assert!(matches!(
            panel.commit_editor.read(cx).mode().clone(),
            EditorMode::Full { .. }
        ));

        panel.toggle_fill_commit_editor(&ToggleFillCommitEditor, window, cx);
        assert!(!panel.commit_editor_expanded);
        assert!(matches!(
            panel.commit_editor.read(cx).mode().clone(),
            EditorMode::AutoHeight { .. }
        ));
    });
}

#[gpui::test]
async fn test_focus_handle(cx: &mut TestAppContext) {
    init_test(cx);

    let (_project, workspace, panel, mut cx) = setup_git_panel_with_changes(
        cx,
        json!({
            ".git": {},
            "tracked": "tracked\n",
        }),
        &[("tracked", StatusCode::Modified)],
    )
    .await;

    workspace.update_in(&mut cx, |workspace, window, cx| {
        workspace.add_panel(panel.clone(), window, cx);
    });

    // With changes present and the editor not expanded, the panel's own
    // focus handle should be returned, in order for
    // `git_panel::ToggleFocus` to focus on the panel itself.
    panel.update_in(&mut cx, |panel, _window, cx| {
        assert!(!panel.entries.is_empty());
        assert!(!panel.commit_editor_expanded);
        assert_eq!(panel.focus_handle(cx), panel.focus_handle.clone());
    });

    // Expand the editor so we can later confirm that toggling focus
    // actually focuses on the commit editor, seeing as it has been
    // expanded.
    panel.update_in(&mut cx, |panel, window, cx| {
        panel.toggle_fill_commit_editor(&ToggleFillCommitEditor, window, cx);
        assert!(panel.commit_editor_expanded);
    });

    cx.dispatch_action(super::ToggleFocus);
    panel.update_in(&mut cx, |panel, window, cx| {
        assert!(panel.commit_editor.focus_handle(cx).is_focused(window));
    });
}
