use super::*;

#[gpui::test]
async fn test_entry_worktree_paths(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/root",
        json!({
            "mav": {
                ".git": {},
                "crates": {
                    "gpui": {
                        "gpui.rs": "fn main() {}"
                    },
                    "util": {
                        "util.rs": "fn do_it() {}"
                    }
                }
            },
        }),
    )
    .await;

    fs.set_status_for_repo(
        Path::new(path!("/root/mav/.git")),
        &[
            ("crates/gpui/gpui.rs", StatusCode::Modified.worktree()),
            ("crates/util/util.rs", StatusCode::Modified.worktree()),
        ],
    );

    let project = Project::test(fs.clone(), [path!("/root/mav/crates/gpui").as_ref()], cx).await;
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
    pretty_assertions::assert_eq!(
        entries,
        [
            GitListEntry::Header(GitHeaderEntry {
                header: Section::Tracked
            }),
            GitListEntry::Status(GitStatusEntry {
                repo_path: repo_path("crates/gpui/gpui.rs"),
                status: StatusCode::Modified.worktree(),
                staging: StageStatus::Unstaged,
                diff_stat: Some(DiffStat {
                    added: 1,
                    deleted: 1,
                }),
            }),
            GitListEntry::Status(GitStatusEntry {
                repo_path: repo_path("crates/util/util.rs"),
                status: StatusCode::Modified.worktree(),
                staging: StageStatus::Unstaged,
                diff_stat: Some(DiffStat {
                    added: 1,
                    deleted: 1,
                }),
            },),
        ],
    );

    let handle = cx.update_window_entity(&panel, |panel, _, _| {
        std::mem::replace(&mut panel.update_visible_entries_task, Task::ready(()))
    });
    cx.executor().advance_clock(2 * UPDATE_DEBOUNCE);
    handle.await;
    let entries = panel.read_with(cx, |panel, _| panel.entries.clone());
    pretty_assertions::assert_eq!(
        entries,
        [
            GitListEntry::Header(GitHeaderEntry {
                header: Section::Tracked
            }),
            GitListEntry::Status(GitStatusEntry {
                repo_path: repo_path("crates/gpui/gpui.rs"),
                status: StatusCode::Modified.worktree(),
                staging: StageStatus::Unstaged,
                diff_stat: Some(DiffStat {
                    added: 1,
                    deleted: 1,
                }),
            }),
            GitListEntry::Status(GitStatusEntry {
                repo_path: repo_path("crates/util/util.rs"),
                status: StatusCode::Modified.worktree(),
                staging: StageStatus::Unstaged,
                diff_stat: Some(DiffStat {
                    added: 1,
                    deleted: 1,
                }),
            },),
        ],
    );
}

#[gpui::test]
async fn test_discard_prompt_escapes_markdown_in_file_name(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/root",
        json!({
            "project": {
                ".git": {},
                "__somefile__": "modified\n",
            },
        }),
    )
    .await;

    fs.set_status_for_repo(
        Path::new(path!("/root/project/.git")),
        &[("__somefile__", StatusCode::Modified.worktree())],
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

    panel.update_in(cx, |panel, window, cx| {
        panel.selected_entry = Some(1);
        panel.revert_selected(&git::RestoreFile::default(), window, cx);
    });

    let (message, _detail) = cx
        .pending_prompt()
        .expect("discard should show a confirmation prompt");

    assert_eq!(
        message,
        "Are you sure you want to discard changes to `__somefile__`?"
    );
}
