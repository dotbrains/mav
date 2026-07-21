use super::*;

#[test]
fn test_compress_diff_no_truncation() {
    let diff = indoc! {"
        --- a/file.txt
        +++ b/file.txt
        @@ -1,2 +1,2 @@
        -old
        +new
    "};
    let result = GitPanel::compress_commit_diff(diff, 1000);
    assert_eq!(result, diff);
}

#[test]
fn test_compress_diff_truncate_long_lines() {
    let long_line = "🦀".repeat(300);
    let diff = indoc::formatdoc! {"
        --- a/file.txt
        +++ b/file.txt
        @@ -1,2 +1,3 @@
         context
        +{}
         more context
    ", long_line};
    let result = GitPanel::compress_commit_diff(&diff, 100);
    assert!(result.contains("...[truncated]"));
    assert!(result.len() < diff.len());
}

#[test]
fn test_compress_diff_truncate_hunks() {
    let diff = indoc! {"
        --- a/file.txt
        +++ b/file.txt
        @@ -1,2 +1,2 @@
         context
        -old1
        +new1
        @@ -5,2 +5,2 @@
         context 2
        -old2
        +new2
        @@ -10,2 +10,2 @@
         context 3
        -old3
        +new3
    "};
    let result = GitPanel::compress_commit_diff(diff, 100);
    let expected = indoc! {"
        --- a/file.txt
        +++ b/file.txt
        @@ -1,2 +1,2 @@
         context
        -old1
        +new1
        [...skipped 2 hunks...]
    "};
    assert_eq!(result, expected);
}

#[test]
fn test_commit_message_prompt_includes_user_agents_md_before_project_rules() {
    let prompt = GitPanel::build_commit_message_prompt(
        "Write a commit message.",
        Some("Use terse commit messages."),
        Some("Use the git_ui prefix."),
        Some("Follow the configured commit message format."),
        "Update generated message",
        "diff --git a/file b/file",
    );

    assert!(prompt.contains("Use terse commit messages."));
    assert!(prompt.contains("Use the git_ui prefix."));
    assert!(prompt.contains("Follow the configured commit message format."));
    assert!(prompt.contains("Update generated message"));
    assert!(prompt.contains("diff --git a/file b/file"));

    let user_agents_md_index = prompt.find("<rules>").unwrap();
    let project_rules_index = prompt.find("<project_rules>").unwrap();
    let instructions_index = prompt.find("<commit_message_instructions>").unwrap();
    assert!(user_agents_md_index < project_rules_index);
    assert!(project_rules_index < instructions_index);
}

#[test]
fn test_commit_message_prompt_omits_blank_instructions() {
    let prompt = GitPanel::build_commit_message_prompt(
        "Write a commit message.",
        None,
        None,
        Some("   \n  "),
        "",
        "diff --git a/file b/file",
    );

    assert!(!prompt.contains("<commit_message_instructions>"));
}

#[gpui::test]
async fn test_suggest_commit_message(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/project"),
        json!({
            ".git": {},
            "tracked": "tracked\n",
            "untracked": "\n",
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

    let entries = panel.read_with(cx, |panel, _| panel.entries.clone());

    // GitPanel
    // - Tracked:
    // - [] tracked
    // - Untracked
    // - [] untracked
    //
    // The commit message should now read:
    // "Update tracked"
    let message = panel.update(cx, |panel, cx| panel.suggest_commit_message(cx));
    assert_eq!(message, Some("Update tracked".to_string()));

    let first_status_entry = entries[1].clone();
    panel.update_in(cx, |panel, window, cx| {
        panel.toggle_staged_for_entry(&first_status_entry, window, cx);
    });

    cx.read(|cx| {
        project
            .read(cx)
            .worktrees(cx)
            .next()
            .unwrap()
            .read(cx)
            .as_local()
            .unwrap()
            .scan_complete()
    })
    .await;

    cx.executor().run_until_parked();

    let handle = cx.update_window_entity(&panel, |panel, _, _| {
        std::mem::replace(&mut panel.update_visible_entries_task, Task::ready(()))
    });
    cx.executor().advance_clock(2 * UPDATE_DEBOUNCE);
    handle.await;

    // GitPanel
    // - Tracked:
    // - [x] tracked
    // - Untracked
    // - [] untracked
    //
    // The commit message should still read:
    // "Update tracked"
    let message = panel.update(cx, |panel, cx| panel.suggest_commit_message(cx));
    assert_eq!(message, Some("Update tracked".to_string()));

    let second_status_entry = entries[3].clone();
    panel.update_in(cx, |panel, window, cx| {
        panel.toggle_staged_for_entry(&second_status_entry, window, cx);
    });

    cx.read(|cx| {
        project
            .read(cx)
            .worktrees(cx)
            .next()
            .unwrap()
            .read(cx)
            .as_local()
            .unwrap()
            .scan_complete()
    })
    .await;

    cx.executor().run_until_parked();

    let handle = cx.update_window_entity(&panel, |panel, _, _| {
        std::mem::replace(&mut panel.update_visible_entries_task, Task::ready(()))
    });
    cx.executor().advance_clock(2 * UPDATE_DEBOUNCE);
    handle.await;

    // GitPanel
    // - Tracked:
    // - [x] tracked
    // - Untracked
    // - [x] untracked
    //
    // The commit message should now read:
    // "Enter commit message"
    // (which means we should see None returned).
    let message = panel.update(cx, |panel, cx| panel.suggest_commit_message(cx));
    assert!(message.is_none());

    panel.update_in(cx, |panel, window, cx| {
        panel.toggle_staged_for_entry(&first_status_entry, window, cx);
    });

    cx.read(|cx| {
        project
            .read(cx)
            .worktrees(cx)
            .next()
            .unwrap()
            .read(cx)
            .as_local()
            .unwrap()
            .scan_complete()
    })
    .await;

    cx.executor().run_until_parked();

    let handle = cx.update_window_entity(&panel, |panel, _, _| {
        std::mem::replace(&mut panel.update_visible_entries_task, Task::ready(()))
    });
    cx.executor().advance_clock(2 * UPDATE_DEBOUNCE);
    handle.await;

    // GitPanel
    // - Tracked:
    // - [] tracked
    // - Untracked
    // - [x] untracked
    //
    // The commit message should now read:
    // "Update untracked"
    let message = panel.update(cx, |panel, cx| panel.suggest_commit_message(cx));
    assert_eq!(message, Some("Create untracked".to_string()));

    panel.update_in(cx, |panel, window, cx| {
        panel.toggle_staged_for_entry(&second_status_entry, window, cx);
    });

    cx.read(|cx| {
        project
            .read(cx)
            .worktrees(cx)
            .next()
            .unwrap()
            .read(cx)
            .as_local()
            .unwrap()
            .scan_complete()
    })
    .await;

    cx.executor().run_until_parked();

    let handle = cx.update_window_entity(&panel, |panel, _, _| {
        std::mem::replace(&mut panel.update_visible_entries_task, Task::ready(()))
    });
    cx.executor().advance_clock(2 * UPDATE_DEBOUNCE);
    handle.await;

    // GitPanel
    // - Tracked:
    // - [] tracked
    // - Untracked
    // - [] untracked
    //
    // The commit message should now read:
    // "Update tracked"
    let message = panel.update(cx, |panel, cx| panel.suggest_commit_message(cx));
    assert_eq!(message, Some("Update tracked".to_string()));
}
