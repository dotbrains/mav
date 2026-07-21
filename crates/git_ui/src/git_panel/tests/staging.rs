use super::*;

#[gpui::test]
async fn test_bulk_staging(cx: &mut TestAppContext) {
    use GitListEntry::*;

    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/root",
        json!({
            "project": {
                ".git": {},
                "src": {
                    "main.rs": "fn main() {}",
                    "lib.rs": "pub fn hello() {}",
                    "utils.rs": "pub fn util() {}"
                },
                "tests": {
                    "test.rs": "fn test() {}"
                },
                "new_file.txt": "new content",
                "another_new.rs": "// new file",
                "conflict.txt": "conflicted content"
            }
        }),
    )
    .await;

    fs.set_status_for_repo(
        Path::new(path!("/root/project/.git")),
        &[
            ("src/main.rs", StatusCode::Modified.worktree()),
            ("src/lib.rs", StatusCode::Modified.worktree()),
            ("tests/test.rs", StatusCode::Modified.worktree()),
            ("new_file.txt", FileStatus::Untracked),
            ("another_new.rs", FileStatus::Untracked),
            ("src/utils.rs", FileStatus::Untracked),
            (
                "conflict.txt",
                UnmergedStatus {
                    first_head: UnmergedStatusCode::Updated,
                    second_head: UnmergedStatusCode::Updated,
                }
                .into(),
            ),
        ],
    );

    let project = Project::test(fs.clone(), [Path::new(path!("/root/project"))], cx).await;
    let window_handle =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window_handle
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window_handle.into(), cx);

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

    let panel = workspace.update_in(cx, GitPanel::new);

    let handle = cx.update_window_entity(&panel, |panel, _, _| {
        std::mem::replace(&mut panel.update_visible_entries_task, Task::ready(()))
    });
    cx.executor().advance_clock(2 * UPDATE_DEBOUNCE);
    handle.await;

    let entries = panel.read_with(cx, |panel, _| panel.entries.clone());
    #[rustfmt::skip]
    pretty_assertions::assert_matches!(
        entries.as_slice(),
        &[
            Header(GitHeaderEntry { header: Section::Conflict }),
            Status(GitStatusEntry { staging: StageStatus::Unstaged, .. }),
            Header(GitHeaderEntry { header: Section::Tracked }),
            Status(GitStatusEntry { staging: StageStatus::Unstaged, .. }),
            Status(GitStatusEntry { staging: StageStatus::Unstaged, .. }),
            Status(GitStatusEntry { staging: StageStatus::Unstaged, .. }),
            Header(GitHeaderEntry { header: Section::New }),
            Status(GitStatusEntry { staging: StageStatus::Unstaged, .. }),
            Status(GitStatusEntry { staging: StageStatus::Unstaged, .. }),
            Status(GitStatusEntry { staging: StageStatus::Unstaged, .. }),
        ],
    );

    let second_status_entry = entries[3].clone();
    panel.update_in(cx, |panel, window, cx| {
        panel.toggle_staged_for_entry(&second_status_entry, window, cx);
    });

    panel.update_in(cx, |panel, window, cx| {
        panel.selected_entry = Some(7);
        panel.stage_range(&git::StageRange, window, cx);
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

    let entries = panel.read_with(cx, |panel, _| panel.entries.clone());
    #[rustfmt::skip]
    pretty_assertions::assert_matches!(
        entries.as_slice(),
        &[
            Header(GitHeaderEntry { header: Section::Conflict }),
            Status(GitStatusEntry { staging: StageStatus::Unstaged, .. }),
            Header(GitHeaderEntry { header: Section::Tracked }),
            Status(GitStatusEntry { staging: StageStatus::Staged, .. }),
            Status(GitStatusEntry { staging: StageStatus::Staged, .. }),
            Status(GitStatusEntry { staging: StageStatus::Staged, .. }),
            Header(GitHeaderEntry { header: Section::New }),
            Status(GitStatusEntry { staging: StageStatus::Staged, .. }),
            Status(GitStatusEntry { staging: StageStatus::Unstaged, .. }),
            Status(GitStatusEntry { staging: StageStatus::Unstaged, .. }),
        ],
    );

    let third_status_entry = entries[4].clone();
    panel.update_in(cx, |panel, window, cx| {
        panel.toggle_staged_for_entry(&third_status_entry, window, cx);
    });

    panel.update_in(cx, |panel, window, cx| {
        panel.selected_entry = Some(9);
        panel.stage_range(&git::StageRange, window, cx);
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

    let entries = panel.read_with(cx, |panel, _| panel.entries.clone());
    #[rustfmt::skip]
    pretty_assertions::assert_matches!(
        entries.as_slice(),
        &[
            Header(GitHeaderEntry { header: Section::Conflict }),
            Status(GitStatusEntry { staging: StageStatus::Unstaged, .. }),
            Header(GitHeaderEntry { header: Section::Tracked }),
            Status(GitStatusEntry { staging: StageStatus::Staged, .. }),
            Status(GitStatusEntry { staging: StageStatus::Unstaged, .. }),
            Status(GitStatusEntry { staging: StageStatus::Staged, .. }),
            Header(GitHeaderEntry { header: Section::New }),
            Status(GitStatusEntry { staging: StageStatus::Staged, .. }),
            Status(GitStatusEntry { staging: StageStatus::Staged, .. }),
            Status(GitStatusEntry { staging: StageStatus::Staged, .. }),
        ],
    );
}

#[gpui::test]
async fn test_bulk_staging_with_sort_by_paths(cx: &mut TestAppContext) {
    use GitListEntry::*;

    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/root",
        json!({
            "project": {
                ".git": {},
                "src": {
                    "main.rs": "fn main() {}",
                    "lib.rs": "pub fn hello() {}",
                    "utils.rs": "pub fn util() {}"
                },
                "tests": {
                    "test.rs": "fn test() {}"
                },
                "new_file.txt": "new content",
                "another_new.rs": "// new file",
                "conflict.txt": "conflicted content"
            }
        }),
    )
    .await;

    fs.set_status_for_repo(
        Path::new(path!("/root/project/.git")),
        &[
            ("src/main.rs", StatusCode::Modified.worktree()),
            ("src/lib.rs", StatusCode::Modified.worktree()),
            ("tests/test.rs", StatusCode::Modified.worktree()),
            ("new_file.txt", FileStatus::Untracked),
            ("another_new.rs", FileStatus::Untracked),
            ("src/utils.rs", FileStatus::Untracked),
            (
                "conflict.txt",
                UnmergedStatus {
                    first_head: UnmergedStatusCode::Updated,
                    second_head: UnmergedStatusCode::Updated,
                }
                .into(),
            ),
        ],
    );

    let project = Project::test(fs.clone(), [Path::new(path!("/root/project"))], cx).await;
    let window_handle =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window_handle
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window_handle.into(), cx);

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

    let panel = workspace.update_in(cx, GitPanel::new);

    let handle = cx.update_window_entity(&panel, |panel, _, _| {
        std::mem::replace(&mut panel.update_visible_entries_task, Task::ready(()))
    });
    cx.executor().advance_clock(2 * UPDATE_DEBOUNCE);
    handle.await;

    let entries = panel.read_with(cx, |panel, _| panel.entries.clone());
    #[rustfmt::skip]
    pretty_assertions::assert_matches!(
        entries.as_slice(),
        &[
            Header(GitHeaderEntry { header: Section::Conflict }),
            Status(GitStatusEntry { staging: StageStatus::Unstaged, .. }),
            Header(GitHeaderEntry { header: Section::Tracked }),
            Status(GitStatusEntry { staging: StageStatus::Unstaged, .. }),
            Status(GitStatusEntry { staging: StageStatus::Unstaged, .. }),
            Status(GitStatusEntry { staging: StageStatus::Unstaged, .. }),
            Header(GitHeaderEntry { header: Section::New }),
            Status(GitStatusEntry { staging: StageStatus::Unstaged, .. }),
            Status(GitStatusEntry { staging: StageStatus::Unstaged, .. }),
            Status(GitStatusEntry { staging: StageStatus::Unstaged, .. }),
        ],
    );

    assert_entry_paths(
        &entries,
        &[
            None,
            Some("conflict.txt"),
            None,
            Some("src/lib.rs"),
            Some("src/main.rs"),
            Some("tests/test.rs"),
            None,
            Some("another_new.rs"),
            Some("new_file.txt"),
            Some("src/utils.rs"),
        ],
    );

    let second_status_entry = entries[3].clone();
    panel.update_in(cx, |panel, window, cx| {
        panel.toggle_staged_for_entry(&second_status_entry, window, cx);
    });

    cx.update(|_window, cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.git_panel.get_or_insert_default().group_by = Some(GitPanelGroupBy::None);
            })
        });
    });

    panel.update_in(cx, |panel, window, cx| {
        panel.selected_entry = Some(7);
        panel.stage_range(&git::StageRange, window, cx);
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

    let entries = panel.read_with(cx, |panel, _| panel.entries.clone());
    #[rustfmt::skip]
    pretty_assertions::assert_matches!(
        entries.as_slice(),
        &[
            Status(GitStatusEntry { status: FileStatus::Untracked, staging: StageStatus::Unstaged, .. }),
            Status(GitStatusEntry { status: FileStatus::Unmerged(..), staging: StageStatus::Unstaged, .. }),
            Status(GitStatusEntry { status: FileStatus::Untracked, staging: StageStatus::Unstaged, .. }),
            Status(GitStatusEntry { status: FileStatus::Tracked(..), staging: StageStatus::Staged, .. }),
            Status(GitStatusEntry { status: FileStatus::Tracked(..), staging: StageStatus::Unstaged, .. }),
            Status(GitStatusEntry { status: FileStatus::Untracked, staging: StageStatus::Unstaged, .. }),
            Status(GitStatusEntry { status: FileStatus::Tracked(..), staging: StageStatus::Unstaged, .. }),
        ],
    );

    assert_entry_paths(
        &entries,
        &[
            Some("another_new.rs"),
            Some("conflict.txt"),
            Some("new_file.txt"),
            Some("src/lib.rs"),
            Some("src/main.rs"),
            Some("src/utils.rs"),
            Some("tests/test.rs"),
        ],
    );

    let third_status_entry = entries[4].clone();
    panel.update_in(cx, |panel, window, cx| {
        panel.toggle_staged_for_entry(&third_status_entry, window, cx);
    });

    panel.update_in(cx, |panel, window, cx| {
        panel.selected_entry = Some(9);
        panel.stage_range(&git::StageRange, window, cx);
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

    let entries = panel.read_with(cx, |panel, _| panel.entries.clone());
    #[rustfmt::skip]
    pretty_assertions::assert_matches!(
        entries.as_slice(),
        &[
            Status(GitStatusEntry { status: FileStatus::Untracked, staging: StageStatus::Unstaged, .. }),
            Status(GitStatusEntry { status: FileStatus::Unmerged(..), staging: StageStatus::Unstaged, .. }),
            Status(GitStatusEntry { status: FileStatus::Untracked, staging: StageStatus::Unstaged, .. }),
            Status(GitStatusEntry { status: FileStatus::Tracked(..), staging: StageStatus::Staged, .. }),
            Status(GitStatusEntry { status: FileStatus::Tracked(..), staging: StageStatus::Staged, .. }),
            Status(GitStatusEntry { status: FileStatus::Untracked, staging: StageStatus::Unstaged, .. }),
            Status(GitStatusEntry { status: FileStatus::Tracked(..), staging: StageStatus::Unstaged, .. }),
        ],
    );

    assert_entry_paths(
        &entries,
        &[
            Some("another_new.rs"),
            Some("conflict.txt"),
            Some("new_file.txt"),
            Some("src/lib.rs"),
            Some("src/main.rs"),
            Some("src/utils.rs"),
            Some("tests/test.rs"),
        ],
    );
}
