use super::*;

#[gpui::test]
async fn test_selection_vs_marked_entries_priority(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "dir1": {
                "file1.txt": "",
                "file2.txt": "",
                "file3.txt": "",
            },
            "dir2": {
                "file4.txt": "",
                "file5.txt": "",
            },
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/root".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    toggle_expand_dir(&panel, "root/dir1", cx);
    toggle_expand_dir(&panel, "root/dir2", cx);

    cx.simulate_modifiers_change(gpui::Modifiers {
        control: true,
        ..Default::default()
    });

    select_path_with_mark(&panel, "root/dir1/file2.txt", cx);
    select_path(&panel, "root/dir1/file1.txt", cx);

    assert_eq!(
        visible_entries_as_strings(&panel, 0..15, cx),
        &[
            "v root",
            "    v dir1",
            "          file1.txt  <== selected",
            "          file2.txt  <== marked",
            "          file3.txt",
            "    v dir2",
            "          file4.txt",
            "          file5.txt",
        ],
        "Initial state with one marked entry and different selection"
    );

    // Delete should operate on the selected entry (file1.txt)
    submit_deletion(&panel, cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..15, cx),
        &[
            "v root",
            "    v dir1",
            "          file2.txt  <== selected  <== marked",
            "          file3.txt",
            "    v dir2",
            "          file4.txt",
            "          file5.txt",
        ],
        "Should delete selected file, not marked file"
    );

    select_path_with_mark(&panel, "root/dir1/file3.txt", cx);
    select_path_with_mark(&panel, "root/dir2/file4.txt", cx);
    select_path(&panel, "root/dir2/file5.txt", cx);

    assert_eq!(
        visible_entries_as_strings(&panel, 0..15, cx),
        &[
            "v root",
            "    v dir1",
            "          file2.txt  <== marked",
            "          file3.txt  <== marked",
            "    v dir2",
            "          file4.txt  <== marked",
            "          file5.txt  <== selected",
        ],
        "Initial state with multiple marked entries and different selection"
    );

    // Delete should operate on all marked entries, ignoring the selection
    submit_deletion(&panel, cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..15, cx),
        &[
            "v root",
            "    v dir1",
            "    v dir2",
            "          file5.txt  <== selected",
        ],
        "Should delete all marked files, leaving only the selected file"
    );
}

#[gpui::test]
async fn test_selection_fallback_to_next_highest_worktree(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root_b",
        json!({
            "dir1": {
                "file1.txt": "content 1",
                "file2.txt": "content 2",
            },
        }),
    )
    .await;

    fs.insert_tree(
        "/root_c",
        json!({
            "dir2": {},
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/root_b".as_ref(), "/root_c".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    toggle_expand_dir(&panel, "root_b/dir1", cx);
    toggle_expand_dir(&panel, "root_c/dir2", cx);

    cx.simulate_modifiers_change(gpui::Modifiers {
        control: true,
        ..Default::default()
    });
    select_path_with_mark(&panel, "root_b/dir1/file1.txt", cx);
    select_path_with_mark(&panel, "root_b/dir1/file2.txt", cx);

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root_b",
            "    v dir1",
            "          file1.txt  <== marked",
            "          file2.txt  <== selected  <== marked",
            "v root_c",
            "    v dir2",
        ],
        "Initial state with files marked in root_b"
    );

    submit_deletion(&panel, cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root_b",
            "    v dir1  <== selected",
            "v root_c",
            "    v dir2",
        ],
        "After deletion in root_b as it's last deletion, selection should be in root_b"
    );

    select_path_with_mark(&panel, "root_c/dir2", cx);

    submit_deletion(&panel, cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &["v root_b", "    v dir1", "v root_c  <== selected",],
        "After deleting from root_c, it should remain in root_c"
    );
}

pub(crate) fn toggle_expand_dir(
    panel: &Entity<ProjectPanel>,
    path: &str,
    cx: &mut VisualTestContext,
) {
    let path = rel_path(path);
    panel.update_in(cx, |panel, window, cx| {
        for worktree in panel.project.read(cx).worktrees(cx).collect::<Vec<_>>() {
            let worktree = worktree.read(cx);
            if let Ok(relative_path) = path.strip_prefix(worktree.root_name()) {
                let entry_id = worktree.entry_for_path(relative_path).unwrap().id;
                panel.toggle_expanded(entry_id, window, cx);
                return;
            }
        }
        panic!("no worktree for path {:?}", path);
    });
    cx.run_until_parked();
}
