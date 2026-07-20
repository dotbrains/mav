use super::*;

#[gpui::test]
async fn test_select_git_entry(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "tree1": {
                ".git": {},
                "dir1": {
                    "modified1.txt": "1",
                    "unmodified1.txt": "1",
                    "modified2.txt": "1",
                },
                "dir2": {
                    "modified3.txt": "1",
                    "unmodified2.txt": "1",
                },
                "modified4.txt": "1",
                "unmodified3.txt": "1",
            },
            "tree2": {
                ".git": {},
                "dir3": {
                    "modified5.txt": "1",
                    "unmodified4.txt": "1",
                },
                "modified6.txt": "1",
                "unmodified5.txt": "1",
            }
        }),
    )
    .await;

    // Mark files as git modified
    fs.set_head_and_index_for_repo(
        path!("/root/tree1/.git").as_ref(),
        &[
            ("dir1/modified1.txt", "modified".into()),
            ("dir1/modified2.txt", "modified".into()),
            ("modified4.txt", "modified".into()),
            ("dir2/modified3.txt", "modified".into()),
        ],
    );
    fs.set_head_and_index_for_repo(
        path!("/root/tree2/.git").as_ref(),
        &[
            ("dir3/modified5.txt", "modified".into()),
            ("modified6.txt", "modified".into()),
        ],
    );

    let project = Project::test(
        fs.clone(),
        [path!("/root/tree1").as_ref(), path!("/root/tree2").as_ref()],
        cx,
    )
    .await;

    let (scan1_complete, scan2_complete) = project.update(cx, |project, cx| {
        let mut worktrees = project.worktrees(cx);
        let worktree1 = worktrees.next().unwrap();
        let worktree2 = worktrees.next().unwrap();
        (
            worktree1.read(cx).as_local().unwrap().scan_complete(),
            worktree2.read(cx).as_local().unwrap().scan_complete(),
        )
    });
    scan1_complete.await;
    scan2_complete.await;
    cx.run_until_parked();

    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    // Check initial state
    assert_eq!(
        visible_entries_as_strings(&panel, 0..15, cx),
        &[
            "v tree1",
            "    > .git",
            "    > dir1",
            "    > dir2",
            "      modified4.txt",
            "      unmodified3.txt",
            "v tree2",
            "    > .git",
            "    > dir3",
            "      modified6.txt",
            "      unmodified5.txt"
        ],
    );

    // Test selecting next modified entry
    panel.update_in(cx, |panel, window, cx| {
        panel.select_next_git_entry(&SelectNextGitEntry, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..6, cx),
        &[
            "v tree1",
            "    > .git",
            "    v dir1",
            "          modified1.txt  <== selected",
            "          modified2.txt",
            "          unmodified1.txt",
        ],
    );

    panel.update_in(cx, |panel, window, cx| {
        panel.select_next_git_entry(&SelectNextGitEntry, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..6, cx),
        &[
            "v tree1",
            "    > .git",
            "    v dir1",
            "          modified1.txt",
            "          modified2.txt  <== selected",
            "          unmodified1.txt",
        ],
    );

    panel.update_in(cx, |panel, window, cx| {
        panel.select_next_git_entry(&SelectNextGitEntry, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 6..9, cx),
        &[
            "    v dir2",
            "          modified3.txt  <== selected",
            "          unmodified2.txt",
        ],
    );

    panel.update_in(cx, |panel, window, cx| {
        panel.select_next_git_entry(&SelectNextGitEntry, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 9..11, cx),
        &["      modified4.txt  <== selected", "      unmodified3.txt",],
    );

    panel.update_in(cx, |panel, window, cx| {
        panel.select_next_git_entry(&SelectNextGitEntry, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 13..16, cx),
        &[
            "    v dir3",
            "          modified5.txt  <== selected",
            "          unmodified4.txt",
        ],
    );

    panel.update_in(cx, |panel, window, cx| {
        panel.select_next_git_entry(&SelectNextGitEntry, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 16..18, cx),
        &["      modified6.txt  <== selected", "      unmodified5.txt",],
    );

    // Wraps around to first modified file
    panel.update_in(cx, |panel, window, cx| {
        panel.select_next_git_entry(&SelectNextGitEntry, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..18, cx),
        &[
            "v tree1",
            "    > .git",
            "    v dir1",
            "          modified1.txt  <== selected",
            "          modified2.txt",
            "          unmodified1.txt",
            "    v dir2",
            "          modified3.txt",
            "          unmodified2.txt",
            "      modified4.txt",
            "      unmodified3.txt",
            "v tree2",
            "    > .git",
            "    v dir3",
            "          modified5.txt",
            "          unmodified4.txt",
            "      modified6.txt",
            "      unmodified5.txt",
        ],
    );

    // Wraps around again to last modified file
    panel.update_in(cx, |panel, window, cx| {
        panel.select_prev_git_entry(&SelectPrevGitEntry, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 16..18, cx),
        &["      modified6.txt  <== selected", "      unmodified5.txt",],
    );

    panel.update_in(cx, |panel, window, cx| {
        panel.select_prev_git_entry(&SelectPrevGitEntry, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 13..16, cx),
        &[
            "    v dir3",
            "          modified5.txt  <== selected",
            "          unmodified4.txt",
        ],
    );

    panel.update_in(cx, |panel, window, cx| {
        panel.select_prev_git_entry(&SelectPrevGitEntry, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 9..11, cx),
        &["      modified4.txt  <== selected", "      unmodified3.txt",],
    );

    panel.update_in(cx, |panel, window, cx| {
        panel.select_prev_git_entry(&SelectPrevGitEntry, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 6..9, cx),
        &[
            "    v dir2",
            "          modified3.txt  <== selected",
            "          unmodified2.txt",
        ],
    );

    panel.update_in(cx, |panel, window, cx| {
        panel.select_prev_git_entry(&SelectPrevGitEntry, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..6, cx),
        &[
            "v tree1",
            "    > .git",
            "    v dir1",
            "          modified1.txt",
            "          modified2.txt  <== selected",
            "          unmodified1.txt",
        ],
    );

    panel.update_in(cx, |panel, window, cx| {
        panel.select_prev_git_entry(&SelectPrevGitEntry, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..6, cx),
        &[
            "v tree1",
            "    > .git",
            "    v dir1",
            "          modified1.txt  <== selected",
            "          modified2.txt",
            "          unmodified1.txt",
        ],
    );
}
