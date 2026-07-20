use super::*;
// use crate::undo::tests::{build_create_operation, build_rename_operation};
use collections::HashSet;
use editor::{Editor, MultiBufferOffset};
use git::{
    Oid,
    repository::{InitialGraphCommitData, LogSource, RepoPath},
};
use gpui::{Empty, Entity, TestAppContext, VisualTestContext};
use menu::Cancel;
use pretty_assertions::assert_eq;
use project::{FakeFs, ProjectPath};
use serde_json::json;
use settings::{ProjectPanelAutoOpenSettings, SettingsStore};
use smallvec::smallvec;
use std::path::{Path, PathBuf};
use util::{path, paths::PathStyle, rel_path::rel_path};
use workspace::{
    AppState, ItemHandle, MultiWorkspace, Pane, Workspace,
    item::{Item, ProjectItem, test::TestItem},
    register_project_item,
};

#[path = "tests/adding_entries.rs"]
mod adding_entries;
#[path = "tests/autoreveal_follow.rs"]
mod autoreveal_follow;
#[path = "tests/autoreveal_gitignored.rs"]
mod autoreveal_gitignored;
#[path = "tests/collapse_basic.rs"]
mod collapse_basic;
#[path = "tests/copy_paste_directories.rs"]
mod copy_paste_directories;
#[path = "tests/copy_paste_files.rs"]
mod copy_paste_files;
#[path = "tests/deletion_basic.rs"]
mod deletion_basic;
#[path = "tests/deletion_complex.rs"]
mod deletion_complex;
#[path = "tests/deletion_priority.rs"]
mod deletion_priority;
#[path = "tests/directory_selection.rs"]
mod directory_selection;
#[path = "tests/drag_operations.rs"]
mod drag_operations;
#[path = "tests/duplicate_items.rs"]
mod duplicate_items;
#[path = "tests/duplicate_items_history.rs"]
mod duplicate_items_history;
#[path = "tests/editing_files.rs"]
mod editing_files;
#[path = "tests/excluded_creation.rs"]
mod excluded_creation;
#[path = "tests/exclusions_auto_collapse.rs"]
mod exclusions_auto_collapse;
#[path = "tests/explicit_reveal.rs"]
mod explicit_reveal;
#[path = "tests/git_entry_selection.rs"]
mod git_entry_selection;
#[path = "tests/marked_entries_drag.rs"]
mod marked_entries_drag;
#[path = "tests/remove_auto_open.rs"]
mod remove_auto_open;
#[path = "tests/rename_history.rs"]
mod rename_history;
#[path = "tests/rename_move.rs"]
mod rename_move;
#[path = "tests/visible_open_history.rs"]
mod visible_open_history;

#[gpui::test]
async fn test_expand_all_for_entry(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            ".gitignore": "**/ignored_dir\n**/ignored_nested",
            "dir1": {
                "empty1": {
                    "empty2": {
                        "empty3": {
                            "file.txt": ""
                        }
                    }
                },
                "subdir1": {
                    "file1.txt": "",
                    "file2.txt": "",
                    "ignored_nested": {
                        "ignored_file.txt": ""
                    }
                },
                "ignored_dir": {
                    "subdir": {
                        "deep_file.txt": ""
                    }
                }
            }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    // Test 1: When auto-fold is enabled
    cx.update(|_, cx| {
        let settings = *ProjectPanelSettings::get_global(cx);
        ProjectPanelSettings::override_global(
            ProjectPanelSettings {
                auto_fold_dirs: true,
                ..settings
            },
            cx,
        );
    });

    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &["v root", "    > dir1", "      .gitignore",],
        "Initial state should show collapsed root structure"
    );

    toggle_expand_dir(&panel, "root/dir1", cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root",
            "    v dir1  <== selected",
            "        > empty1/empty2/empty3",
            "        > ignored_dir",
            "        > subdir1",
            "      .gitignore",
        ],
        "Should show first level with auto-folded dirs and ignored dir visible"
    );

    let entry_id = find_project_entry(&panel, "root/dir1", cx).unwrap();
    panel.update_in(cx, |panel, window, cx| {
        let project = panel.project.read(cx);
        let worktree = project.worktrees(cx).next().unwrap().read(cx);
        panel.expand_all_for_entry(worktree.id(), entry_id, cx);
        panel.update_visible_entries(None, false, false, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root",
            "    v dir1  <== selected",
            "        v empty1",
            "            v empty2",
            "                v empty3",
            "                      file.txt",
            "        > ignored_dir",
            "        v subdir1",
            "            > ignored_nested",
            "              file1.txt",
            "              file2.txt",
            "      .gitignore",
        ],
        "After expand_all with auto-fold: should not expand ignored_dir, should expand folded dirs, and should not expand ignored_nested"
    );

    // Test 2: When auto-fold is disabled
    cx.update(|_, cx| {
        let settings = *ProjectPanelSettings::get_global(cx);
        ProjectPanelSettings::override_global(
            ProjectPanelSettings {
                auto_fold_dirs: false,
                ..settings
            },
            cx,
        );
    });

    panel.update_in(cx, |panel, window, cx| {
        panel.collapse_all_entries(&CollapseAllEntries, window, cx);
    });

    toggle_expand_dir(&panel, "root/dir1", cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root",
            "    v dir1  <== selected",
            "        > empty1",
            "        > ignored_dir",
            "        > subdir1",
            "      .gitignore",
        ],
        "With auto-fold disabled: should show all directories separately"
    );

    let entry_id = find_project_entry(&panel, "root/dir1", cx).unwrap();
    panel.update_in(cx, |panel, window, cx| {
        let project = panel.project.read(cx);
        let worktree = project.worktrees(cx).next().unwrap().read(cx);
        panel.expand_all_for_entry(worktree.id(), entry_id, cx);
        panel.update_visible_entries(None, false, false, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root",
            "    v dir1  <== selected",
            "        v empty1",
            "            v empty2",
            "                v empty3",
            "                      file.txt",
            "        > ignored_dir",
            "        v subdir1",
            "            > ignored_nested",
            "              file1.txt",
            "              file2.txt",
            "      .gitignore",
        ],
        "After expand_all without auto-fold: should expand all dirs normally, \
         expand ignored_dir itself but not its subdirs, and not expand ignored_nested"
    );

    // Test 3: When explicitly called on ignored directory
    let ignored_dir_entry = find_project_entry(&panel, "root/dir1/ignored_dir", cx).unwrap();
    panel.update_in(cx, |panel, window, cx| {
        let project = panel.project.read(cx);
        let worktree = project.worktrees(cx).next().unwrap().read(cx);
        panel.expand_all_for_entry(worktree.id(), ignored_dir_entry, cx);
        panel.update_visible_entries(None, false, false, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root",
            "    v dir1  <== selected",
            "        v empty1",
            "            v empty2",
            "                v empty3",
            "                      file.txt",
            "        v ignored_dir",
            "            v subdir",
            "                  deep_file.txt",
            "        v subdir1",
            "            > ignored_nested",
            "              file1.txt",
            "              file2.txt",
            "      .gitignore",
        ],
        "After expand_all on ignored_dir: should expand all contents of the ignored directory"
    );
}

#[gpui::test]
async fn test_collapse_all_for_entry(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "dir1": {
                "subdir1": {
                    "nested1": {
                        "file1.txt": "",
                        "file2.txt": ""
                    },
                },
                "subdir2": {
                    "file4.txt": ""
                }
            },
            "dir2": {
                "single_file": {
                    "file5.txt": ""
                }
            }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    // Test 1: Basic collapsing
    {
        let panel = workspace.update_in(cx, ProjectPanel::new);
        cx.run_until_parked();

        toggle_expand_dir(&panel, "root/dir1", cx);
        toggle_expand_dir(&panel, "root/dir1/subdir1", cx);
        toggle_expand_dir(&panel, "root/dir1/subdir1/nested1", cx);
        toggle_expand_dir(&panel, "root/dir1/subdir2", cx);

        assert_eq!(
            visible_entries_as_strings(&panel, 0..20, cx),
            &[
                "v root",
                "    v dir1",
                "        v subdir1",
                "            v nested1",
                "                  file1.txt",
                "                  file2.txt",
                "        v subdir2  <== selected",
                "              file4.txt",
                "    > dir2",
            ],
            "Initial state with everything expanded"
        );

        let entry_id = find_project_entry(&panel, "root/dir1", cx).unwrap();
        panel.update_in(cx, |panel, window, cx| {
            let project = panel.project.read(cx);
            let worktree = project.worktrees(cx).next().unwrap().read(cx);
            panel.collapse_all_for_entry(worktree.id(), entry_id, cx);
            panel.update_visible_entries(None, false, false, window, cx);
        });
        cx.run_until_parked();

        assert_eq!(
            visible_entries_as_strings(&panel, 0..20, cx),
            &["v root", "    > dir1", "    > dir2",],
            "All subdirs under dir1 should be collapsed"
        );
    }

    // Test 2: With auto-fold enabled
    {
        cx.update(|_, cx| {
            let settings = *ProjectPanelSettings::get_global(cx);
            ProjectPanelSettings::override_global(
                ProjectPanelSettings {
                    auto_fold_dirs: true,
                    ..settings
                },
                cx,
            );
        });

        let panel = workspace.update_in(cx, ProjectPanel::new);
        cx.run_until_parked();

        toggle_expand_dir(&panel, "root/dir1", cx);
        toggle_expand_dir(&panel, "root/dir1/subdir1", cx);
        toggle_expand_dir(&panel, "root/dir1/subdir1/nested1", cx);

        assert_eq!(
            visible_entries_as_strings(&panel, 0..20, cx),
            &[
                "v root",
                "    v dir1",
                "        v subdir1/nested1  <== selected",
                "              file1.txt",
                "              file2.txt",
                "        > subdir2",
                "    > dir2/single_file",
            ],
            "Initial state with some dirs expanded"
        );

        let entry_id = find_project_entry(&panel, "root/dir1", cx).unwrap();
        panel.update(cx, |panel, cx| {
            let project = panel.project.read(cx);
            let worktree = project.worktrees(cx).next().unwrap().read(cx);
            panel.collapse_all_for_entry(worktree.id(), entry_id, cx);
        });

        toggle_expand_dir(&panel, "root/dir1", cx);

        assert_eq!(
            visible_entries_as_strings(&panel, 0..20, cx),
            &[
                "v root",
                "    v dir1  <== selected",
                "        > subdir1/nested1",
                "        > subdir2",
                "    > dir2/single_file",
            ],
            "Subdirs should be collapsed and folded with auto-fold enabled"
        );
    }

    // Test 3: With auto-fold disabled
    {
        cx.update(|_, cx| {
            let settings = *ProjectPanelSettings::get_global(cx);
            ProjectPanelSettings::override_global(
                ProjectPanelSettings {
                    auto_fold_dirs: false,
                    ..settings
                },
                cx,
            );
        });

        let panel = workspace.update_in(cx, ProjectPanel::new);
        cx.run_until_parked();

        toggle_expand_dir(&panel, "root/dir1", cx);
        toggle_expand_dir(&panel, "root/dir1/subdir1", cx);
        toggle_expand_dir(&panel, "root/dir1/subdir1/nested1", cx);

        assert_eq!(
            visible_entries_as_strings(&panel, 0..20, cx),
            &[
                "v root",
                "    v dir1",
                "        v subdir1",
                "            v nested1  <== selected",
                "                  file1.txt",
                "                  file2.txt",
                "        > subdir2",
                "    > dir2",
            ],
            "Initial state with some dirs expanded and auto-fold disabled"
        );

        let entry_id = find_project_entry(&panel, "root/dir1", cx).unwrap();
        panel.update(cx, |panel, cx| {
            let project = panel.project.read(cx);
            let worktree = project.worktrees(cx).next().unwrap().read(cx);
            panel.collapse_all_for_entry(worktree.id(), entry_id, cx);
        });

        toggle_expand_dir(&panel, "root/dir1", cx);

        assert_eq!(
            visible_entries_as_strings(&panel, 0..20, cx),
            &[
                "v root",
                "    v dir1  <== selected",
                "        > subdir1",
                "        > subdir2",
                "    > dir2",
            ],
            "Subdirs should be collapsed but not folded with auto-fold disabled"
        );
    }
}

#[gpui::test]
async fn test_collapse_selected_entry_and_children_action(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "dir1": {
                "subdir1": {
                    "nested1": {
                        "file1.txt": "",
                        "file2.txt": ""
                    },
                },
                "subdir2": {
                    "file3.txt": ""
                }
            },
            "dir2": {
                "file4.txt": ""
            }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    toggle_expand_dir(&panel, "root/dir1", cx);
    toggle_expand_dir(&panel, "root/dir1/subdir1", cx);
    toggle_expand_dir(&panel, "root/dir1/subdir1/nested1", cx);
    toggle_expand_dir(&panel, "root/dir1/subdir2", cx);
    toggle_expand_dir(&panel, "root/dir2", cx);

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root",
            "    v dir1",
            "        v subdir1",
            "            v nested1",
            "                  file1.txt",
            "                  file2.txt",
            "        v subdir2",
            "              file3.txt",
            "    v dir2  <== selected",
            "          file4.txt",
        ],
        "Initial state with directories expanded"
    );

    select_path(&panel, "root/dir1", cx);
    cx.run_until_parked();

    panel.update_in(cx, |panel, window, cx| {
        panel.collapse_selected_entry_and_children(&CollapseSelectedEntryAndChildren, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root",
            "    > dir1  <== selected",
            "    v dir2",
            "          file4.txt",
        ],
        "dir1 and all its children should be collapsed, dir2 should remain expanded"
    );

    toggle_expand_dir(&panel, "root/dir1", cx);
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root",
            "    v dir1  <== selected",
            "        > subdir1",
            "        > subdir2",
            "    v dir2",
            "          file4.txt",
        ],
        "After re-expanding dir1, its children should still be collapsed"
    );
}

#[gpui::test]
async fn test_collapse_root_single_worktree(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "dir1": {
                "subdir1": {
                    "file1.txt": ""
                },
                "file2.txt": ""
            },
            "dir2": {
                "file3.txt": ""
            }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    toggle_expand_dir(&panel, "root/dir1", cx);
    toggle_expand_dir(&panel, "root/dir1/subdir1", cx);
    toggle_expand_dir(&panel, "root/dir2", cx);

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root",
            "    v dir1",
            "        v subdir1",
            "              file1.txt",
            "          file2.txt",
            "    v dir2  <== selected",
            "          file3.txt",
        ],
        "Initial state with directories expanded"
    );

    // Select the root and collapse it and its children
    select_path(&panel, "root", cx);
    cx.run_until_parked();

    panel.update_in(cx, |panel, window, cx| {
        panel.collapse_selected_entry_and_children(&CollapseSelectedEntryAndChildren, window, cx);
    });
    cx.run_until_parked();

    // The root and all its children should be collapsed
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &["> root  <== selected"],
        "Root and all children should be collapsed"
    );

    // Re-expand root and dir1, verify children were recursively collapsed
    toggle_expand_dir(&panel, "root", cx);
    toggle_expand_dir(&panel, "root/dir1", cx);
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root",
            "    v dir1  <== selected",
            "        > subdir1",
            "          file2.txt",
            "    > dir2",
        ],
        "After re-expanding root and dir1, subdir1 should still be collapsed"
    );
}

#[gpui::test]
async fn test_collapse_root_multi_worktree(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root1"),
        json!({
            "dir1": {
                "subdir1": {
                    "file1.txt": ""
                },
                "file2.txt": ""
            }
        }),
    )
    .await;
    fs.insert_tree(
        path!("/root2"),
        json!({
            "dir2": {
                "file3.txt": ""
            },
            "file4.txt": ""
        }),
    )
    .await;

    let project = Project::test(
        fs.clone(),
        [path!("/root1").as_ref(), path!("/root2").as_ref()],
        cx,
    )
    .await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    toggle_expand_dir(&panel, "root1/dir1", cx);
    toggle_expand_dir(&panel, "root1/dir1/subdir1", cx);
    toggle_expand_dir(&panel, "root2/dir2", cx);

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root1",
            "    v dir1",
            "        v subdir1",
            "              file1.txt",
            "          file2.txt",
            "v root2",
            "    v dir2  <== selected",
            "          file3.txt",
            "      file4.txt",
        ],
        "Initial state with directories expanded across worktrees"
    );

    // Select root1 and collapse it and its children.
    // In a multi-worktree project, this should only collapse the selected worktree,
    // leaving other worktrees unaffected.
    select_path(&panel, "root1", cx);
    cx.run_until_parked();

    panel.update_in(cx, |panel, window, cx| {
        panel.collapse_selected_entry_and_children(&CollapseSelectedEntryAndChildren, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "> root1  <== selected",
            "v root2",
            "    v dir2",
            "          file3.txt",
            "      file4.txt",
        ],
        "Only root1 should be collapsed, root2 should remain expanded"
    );

    // Re-expand root1 and verify its children were recursively collapsed
    toggle_expand_dir(&panel, "root1", cx);

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root1  <== selected",
            "    > dir1",
            "v root2",
            "    v dir2",
            "          file3.txt",
            "      file4.txt",
        ],
        "After re-expanding root1, dir1 should still be collapsed, root2 should be unaffected"
    );
}

#[gpui::test]
async fn test_collapse_non_root_multi_worktree(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root1"),
        json!({
            "dir1": {
                "subdir1": {
                    "file1.txt": ""
                },
                "file2.txt": ""
            }
        }),
    )
    .await;
    fs.insert_tree(
        path!("/root2"),
        json!({
            "dir2": {
                "subdir2": {
                    "file3.txt": ""
                },
                "file4.txt": ""
            }
        }),
    )
    .await;

    let project = Project::test(
        fs.clone(),
        [path!("/root1").as_ref(), path!("/root2").as_ref()],
        cx,
    )
    .await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    toggle_expand_dir(&panel, "root1/dir1", cx);
    toggle_expand_dir(&panel, "root1/dir1/subdir1", cx);
    toggle_expand_dir(&panel, "root2/dir2", cx);
    toggle_expand_dir(&panel, "root2/dir2/subdir2", cx);

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root1",
            "    v dir1",
            "        v subdir1",
            "              file1.txt",
            "          file2.txt",
            "v root2",
            "    v dir2",
            "        v subdir2  <== selected",
            "              file3.txt",
            "          file4.txt",
        ],
        "Initial state with directories expanded across worktrees"
    );

    // Select dir1 in root1 and collapse it
    select_path(&panel, "root1/dir1", cx);
    cx.run_until_parked();

    panel.update_in(cx, |panel, window, cx| {
        panel.collapse_selected_entry_and_children(&CollapseSelectedEntryAndChildren, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root1",
            "    > dir1  <== selected",
            "v root2",
            "    v dir2",
            "        v subdir2",
            "              file3.txt",
            "          file4.txt",
        ],
        "Only dir1 should be collapsed, root2 should be completely unaffected"
    );

    // Re-expand dir1 and verify subdir1 was recursively collapsed
    toggle_expand_dir(&panel, "root1/dir1", cx);

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root1",
            "    v dir1  <== selected",
            "        > subdir1",
            "          file2.txt",
            "v root2",
            "    v dir2",
            "        v subdir2",
            "              file3.txt",
            "          file4.txt",
        ],
        "After re-expanding dir1, subdir1 should still be collapsed"
    );
}

#[gpui::test]
async fn test_expand_all_entries(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/project_root",
        json!({
            "dir_1": {
                "nested_dir": {
                    "file_a.py": "# File contents",
                    "file_b.py": "# File contents",
                    "file_c.py": "# File contents",
                },
                "file_1.py": "# File contents",
                "file_2.py": "# File contents",
                "file_3.py": "# File contents",
            },
            "dir_2": {
                "file_1.py": "# File contents",
                "file_2.py": "# File contents",
                "file_3.py": "# File contents",
            }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/project_root".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    panel.update_in(cx, |panel, window, cx| {
        panel.collapse_all_entries(&CollapseAllEntries, window, cx)
    });
    cx.executor().run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &["v project_root", "    > dir_1", "    > dir_2",]
    );

    panel.update_in(cx, |panel, window, cx| {
        panel.expand_all_entries(&ExpandAllEntries, window, cx)
    });
    cx.executor().run_until_parked();

    let entries = visible_entries_as_strings(&panel, 0..20, cx);
    assert_eq!(entries.len(), 13, "should show all 13 entries");
    assert!(entries[0].starts_with("v project_root"), "root expanded");
    assert!(entries[1].contains("v dir_1"), "dir_1 expanded");
    assert!(entries[2].contains("v nested_dir"), "nested_dir expanded");
    assert!(
        entries.iter().any(|e| e.contains("file_a.py")),
        "file_a visible"
    );
    assert!(
        entries.iter().any(|e| e.contains("file_c.py")),
        "file_c visible"
    );
    assert!(
        entries.iter().any(|e| e.contains("v dir_2")),
        "dir_2 expanded"
    );
    assert!(
        !entries.iter().any(|e| e.contains("> ")),
        "no collapsed dirs"
    );
}

#[gpui::test]
async fn test_expand_all_entries_multiple_worktrees(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    let worktree_content = json!({
        "dir_1": {
            "file_1.py": "# File contents",
        },
        "dir_2": {
            "file_1.py": "# File contents",
        }
    });

    fs.insert_tree("/project_root_1", worktree_content.clone())
        .await;
    fs.insert_tree("/project_root_2", worktree_content).await;

    let project = Project::test(
        fs.clone(),
        ["/project_root_1".as_ref(), "/project_root_2".as_ref()],
        cx,
    )
    .await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    panel.update_in(cx, |panel, window, cx| {
        panel.collapse_all_entries(&CollapseAllEntries, window, cx)
    });
    cx.executor().run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &["> project_root_1", "> project_root_2",]
    );

    panel.update_in(cx, |panel, window, cx| {
        panel.expand_all_entries(&ExpandAllEntries, window, cx)
    });
    cx.executor().run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v project_root_1",
            "    v dir_1",
            "          file_1.py",
            "    v dir_2",
            "          file_1.py",
            "v project_root_2",
            "    v dir_1",
            "          file_1.py",
            "    v dir_2",
            "          file_1.py",
        ]
    );
}

#[gpui::test]
async fn test_expand_all_entries_via_window_dispatch(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    let worktree_content = json!({
        "dir_1": {
            "file_1.py": "# File contents",
        },
        "dir_2": {
            "file_1.py": "# File contents",
        }
    });

    fs.insert_tree("/project_root_1", worktree_content.clone())
        .await;
    fs.insert_tree("/project_root_2", worktree_content).await;

    let project = Project::test(
        fs.clone(),
        ["/project_root_1".as_ref(), "/project_root_2".as_ref()],
        cx,
    )
    .await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    cx.update(|_, cx| {
        let settings = *ProjectPanelSettings::get_global(cx);
        ProjectPanelSettings::override_global(
            ProjectPanelSettings {
                auto_reveal_entries: false,
                ..settings
            },
            cx,
        );
    });
    let panel = workspace.update_in(cx, |workspace, window, cx| {
        let panel = ProjectPanel::new(workspace, window, cx);
        workspace.add_panel(panel.clone(), window, cx);
        panel
    });
    cx.run_until_parked();

    panel.update_in(cx, |panel, window, cx| {
        panel.collapse_all_entries(&CollapseAllEntries, window, cx)
    });
    cx.executor().run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &["> project_root_1", "> project_root_2",]
    );

    panel.update_in(cx, |panel, window, cx| {
        panel.focus_handle(cx).focus(window, cx);
    });
    cx.dispatch_action(ExpandAllEntries);
    cx.executor().run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v project_root_1",
            "    v dir_1",
            "          file_1.py",
            "    v dir_2",
            "          file_1.py",
            "v project_root_2",
            "    v dir_1",
            "          file_1.py",
            "    v dir_2",
            "          file_1.py",
        ]
    );
}

#[gpui::test]
async fn test_expand_all_for_entry_single_worktree(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    let worktree_content = json!({
        "dir_1": {
            "file_1.py": "# File contents",
        },
        "dir_2": {
            "file_1.py": "# File contents",
        }
    });

    fs.insert_tree("/project_root_1", worktree_content.clone())
        .await;
    fs.insert_tree("/project_root_2", worktree_content).await;

    let project = Project::test(
        fs.clone(),
        ["/project_root_1".as_ref(), "/project_root_2".as_ref()],
        cx,
    )
    .await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    panel.update_in(cx, |panel, window, cx| {
        panel.collapse_all_entries(&CollapseAllEntries, window, cx)
    });
    cx.executor().run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &["> project_root_1", "> project_root_2",]
    );

    let root2_entry = find_project_entry(&panel, "project_root_2", cx).unwrap();
    panel.update_in(cx, |panel, window, cx| {
        let worktree_id = panel
            .project
            .read(cx)
            .worktree_id_for_entry(root2_entry, cx)
            .unwrap();
        panel.expand_all_for_entry(worktree_id, root2_entry, cx);
        panel.update_visible_entries(None, false, false, window, cx);
    });
    cx.executor().run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "> project_root_1",
            "v project_root_2",
            "    v dir_1",
            "          file_1.py",
            "    v dir_2",
            "          file_1.py",
        ]
    );
}

#[gpui::test]
async fn test_expand_all_entries_with_auto_fold(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "dir1": {
                "empty1": {
                    "empty2": {
                        "empty3": {
                            "file.txt": ""
                        }
                    }
                },
            }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.update(|_, cx| {
        let settings = *ProjectPanelSettings::get_global(cx);
        ProjectPanelSettings::override_global(
            ProjectPanelSettings {
                auto_fold_dirs: true,
                ..settings
            },
            cx,
        );
    });

    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    panel.update_in(cx, |panel, window, cx| {
        panel.collapse_all_entries(&CollapseAllEntries, window, cx)
    });
    cx.executor().run_until_parked();

    panel.update_in(cx, |panel, window, cx| {
        panel.expand_all_entries(&ExpandAllEntries, window, cx)
    });
    cx.executor().run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root",
            "    v dir1",
            "        v empty1",
            "            v empty2",
            "                v empty3",
            "                      file.txt",
        ],
        "expand all should unfold auto-folded directories"
    );
}

#[gpui::test]
async fn test_create_entries_without_selection(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "dir1": {
                "file1.txt": "",
            },
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    let panel = workspace.update_in(cx, |workspace, window, cx| {
        let panel = ProjectPanel::new(workspace, window, cx);
        workspace.add_panel(panel.clone(), window, cx);
        panel
    });
    cx.run_until_parked();

    #[rustfmt::skip]
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root",
            "    > dir1",
        ],
        "Initial state with nothing selected"
    );

    panel.update_in(cx, |panel, window, cx| {
        panel.new_file(&NewFile, window, cx);
    });
    cx.run_until_parked();
    panel.update_in(cx, |panel, window, cx| {
        assert!(panel.filename_editor.read(cx).is_focused(window));
    });
    panel
        .update_in(cx, |panel, window, cx| {
            panel.filename_editor.update(cx, |editor, cx| {
                editor.set_text("hello_from_no_selections", window, cx)
            });
            panel.confirm_edit(true, window, cx).unwrap()
        })
        .await
        .unwrap();
    cx.run_until_parked();
    #[rustfmt::skip]
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root",
            "    > dir1",
            "      hello_from_no_selections  <== selected  <== marked",
        ],
        "A new file is created under the root directory"
    );
}

#[gpui::test]
async fn test_create_entries_without_selection_hide_root(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "existing_dir": {
                "existing_file.txt": "",
            },
            "existing_file.txt": "",
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.update(|_, cx| {
        let settings = *ProjectPanelSettings::get_global(cx);
        ProjectPanelSettings::override_global(
            ProjectPanelSettings {
                hide_root: true,
                ..settings
            },
            cx,
        );
    });

    let panel = workspace.update_in(cx, |workspace, window, cx| {
        let panel = ProjectPanel::new(workspace, window, cx);
        workspace.add_panel(panel.clone(), window, cx);
        panel
    });
    cx.run_until_parked();

    #[rustfmt::skip]
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "> existing_dir",
            "  existing_file.txt",
        ],
        "Initial state with hide_root=true, root should be hidden and nothing selected"
    );

    panel.update(cx, |panel, _| {
        assert!(
            panel.selection.is_none(),
            "Should have no selection initially"
        );
    });

    // Test 1: Create new file when no entry is selected
    panel.update_in(cx, |panel, window, cx| {
        panel.new_file(&NewFile, window, cx);
    });
    cx.run_until_parked();
    panel.update_in(cx, |panel, window, cx| {
        assert!(panel.filename_editor.read(cx).is_focused(window));
    });
    cx.run_until_parked();
    #[rustfmt::skip]
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "> existing_dir",
            "  [EDITOR: '']  <== selected",
            "  existing_file.txt",
        ],
        "Editor should appear at root level when hide_root=true and no selection"
    );

    let confirm = panel.update_in(cx, |panel, window, cx| {
        panel.filename_editor.update(cx, |editor, cx| {
            editor.set_text("new_file_at_root.txt", window, cx)
        });
        panel.confirm_edit(true, window, cx).unwrap()
    });
    confirm.await.unwrap();
    cx.run_until_parked();

    #[rustfmt::skip]
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "> existing_dir",
            "  existing_file.txt",
            "  new_file_at_root.txt  <== selected  <== marked",
        ],
        "New file should be created at root level and visible without root prefix"
    );

    assert!(
        fs.is_file(Path::new("/root/new_file_at_root.txt")).await,
        "File should be created in the actual root directory"
    );

    // Test 2: Create new directory when no entry is selected
    panel.update(cx, |panel, _| {
        panel.selection = None;
    });

    panel.update_in(cx, |panel, window, cx| {
        panel.new_directory(&NewDirectory, window, cx);
    });
    cx.run_until_parked();

    panel.update_in(cx, |panel, window, cx| {
        assert!(panel.filename_editor.read(cx).is_focused(window));
    });

    #[rustfmt::skip]
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "> [EDITOR: '']  <== selected",
            "> existing_dir",
            "  existing_file.txt",
            "  new_file_at_root.txt",
        ],
        "Directory editor should appear at root level when hide_root=true and no selection"
    );

    let confirm = panel.update_in(cx, |panel, window, cx| {
        panel.filename_editor.update(cx, |editor, cx| {
            editor.set_text("new_dir_at_root", window, cx)
        });
        panel.confirm_edit(true, window, cx).unwrap()
    });
    confirm.await.unwrap();
    cx.run_until_parked();

    #[rustfmt::skip]
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "> existing_dir",
            "v new_dir_at_root  <== selected",
            "  existing_file.txt",
            "  new_file_at_root.txt",
        ],
        "New directory should be created at root level and visible without root prefix"
    );

    assert!(
        fs.is_dir(Path::new("/root/new_dir_at_root")).await,
        "Directory should be created in the actual root directory"
    );
}

#[gpui::test]
async fn test_context_menu_new_file_in_empty_hidden_root(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/root"), json!({})).await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.update(|_, cx| {
        let settings = *ProjectPanelSettings::get_global(cx);
        ProjectPanelSettings::override_global(
            ProjectPanelSettings {
                hide_root: true,
                ..settings
            },
            cx,
        );
    });

    let panel = workspace.update_in(cx, |workspace, window, cx| {
        let panel = ProjectPanel::new(workspace, window, cx);
        workspace.add_panel(panel.clone(), window, cx);
        panel
    });
    cx.run_until_parked();

    assert!(
        visible_entries_as_strings(&panel, 0..20, cx).is_empty(),
        "Empty worktree with hide_root=true should render no entries"
    );

    panel.update(cx, |panel, _| {
        assert!(
            panel.selection.is_none(),
            "Project panel should start without a selection"
        );
        assert!(
            panel.state.last_worktree_root_id.is_some(),
            "Project panel should still track the hidden root entry"
        );
    });

    panel.update_in(cx, |panel, window, cx| {
        let root_entry_id = panel
            .state
            .last_worktree_root_id
            .expect("hidden root should be available for background context menu actions");
        panel.deploy_context_menu(
            gpui::point(gpui::px(1.), gpui::px(1.)),
            root_entry_id,
            window,
            cx,
        );
        panel.new_file(&NewFile, window, cx);
    });
    cx.run_until_parked();

    panel.update_in(cx, |panel, window, cx| {
        assert!(
            panel.filename_editor.read(cx).is_focused(window),
            "New File from the background context menu should open the filename editor"
        );
    });

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &["  [EDITOR: '']  <== selected"],
        "New file editor should appear at the hidden root level"
    );

    let confirm = panel.update_in(cx, |panel, window, cx| {
        panel.filename_editor.update(cx, |editor, cx| {
            editor.set_text("new_file_from_context_menu.txt", window, cx)
        });
        panel.confirm_edit(true, window, cx).unwrap()
    });
    confirm.await.unwrap();
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &["  new_file_from_context_menu.txt  <== selected  <== marked"],
        "Confirmed file should appear at the hidden root level"
    );

    assert!(
        fs.is_file(Path::new("/root/new_file_from_context_menu.txt"))
            .await,
        "File should be created in the empty root directory"
    );
}

#[cfg(windows)]
#[gpui::test]
async fn test_create_entry_with_trailing_dot_windows(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "dir1": {
                "file1.txt": "",
            },
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    let panel = workspace.update_in(cx, |workspace, window, cx| {
        let panel = ProjectPanel::new(workspace, window, cx);
        workspace.add_panel(panel.clone(), window, cx);
        panel
    });
    cx.run_until_parked();

    #[rustfmt::skip]
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root",
            "    > dir1",
        ],
        "Initial state with nothing selected"
    );

    panel.update_in(cx, |panel, window, cx| {
        panel.new_file(&NewFile, window, cx);
    });
    cx.run_until_parked();
    panel.update_in(cx, |panel, window, cx| {
        assert!(panel.filename_editor.read(cx).is_focused(window));
    });
    panel
        .update_in(cx, |panel, window, cx| {
            panel
                .filename_editor
                .update(cx, |editor, cx| editor.set_text("foo.", window, cx));
            panel.confirm_edit(true, window, cx).unwrap()
        })
        .await
        .unwrap();
    cx.run_until_parked();
    #[rustfmt::skip]
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root",
            "    > dir1",
            "      foo  <== selected  <== marked",
        ],
        "A new file is created under the root directory without the trailing dot"
    );
}

#[gpui::test]
async fn test_highlight_entry_for_external_drag(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "dir1": {
                "file1.txt": "",
                "dir2": {
                    "file2.txt": ""
                }
            },
            "file3.txt": ""
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

    panel.update(cx, |panel, cx| {
        let project = panel.project.read(cx);
        let worktree = project.visible_worktrees(cx).next().unwrap();
        let worktree = worktree.read(cx);

        // Test 1: Target is a directory, should highlight the directory itself
        let dir_entry = worktree.entry_for_path(rel_path("dir1")).unwrap();
        let result = panel.highlight_entry_for_external_drag(dir_entry, worktree);
        assert_eq!(
            result,
            Some(dir_entry.id),
            "Should highlight directory itself"
        );

        // Test 2: Target is nested file, should highlight immediate parent
        let nested_file = worktree
            .entry_for_path(rel_path("dir1/dir2/file2.txt"))
            .unwrap();
        let nested_parent = worktree.entry_for_path(rel_path("dir1/dir2")).unwrap();
        let result = panel.highlight_entry_for_external_drag(nested_file, worktree);
        assert_eq!(
            result,
            Some(nested_parent.id),
            "Should highlight immediate parent"
        );

        // Test 3: Target is root level file, should highlight root
        let root_file = worktree.entry_for_path(rel_path("file3.txt")).unwrap();
        let result = panel.highlight_entry_for_external_drag(root_file, worktree);
        assert_eq!(
            result,
            Some(worktree.root_entry().unwrap().id),
            "Root level file should return None"
        );

        // Test 4: Target is root itself, should highlight root
        let root_entry = worktree.root_entry().unwrap();
        let result = panel.highlight_entry_for_external_drag(root_entry, worktree);
        assert_eq!(
            result,
            Some(root_entry.id),
            "Root level file should return None"
        );
    });
}

#[gpui::test]
async fn test_highlight_entry_for_selection_drag(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "parent_dir": {
                "child_file.txt": "",
                "sibling_file.txt": "",
                "child_dir": {
                    "nested_file.txt": ""
                }
            },
            "other_dir": {
                "other_file.txt": ""
            }
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

    panel.update(cx, |panel, cx| {
        let project = panel.project.read(cx);
        let worktree = project.visible_worktrees(cx).next().unwrap();
        let worktree_id = worktree.read(cx).id();
        let worktree = worktree.read(cx);

        let parent_dir = worktree.entry_for_path(rel_path("parent_dir")).unwrap();
        let child_file = worktree
            .entry_for_path(rel_path("parent_dir/child_file.txt"))
            .unwrap();
        let sibling_file = worktree
            .entry_for_path(rel_path("parent_dir/sibling_file.txt"))
            .unwrap();
        let child_dir = worktree
            .entry_for_path(rel_path("parent_dir/child_dir"))
            .unwrap();
        let other_dir = worktree.entry_for_path(rel_path("other_dir")).unwrap();
        let other_file = worktree
            .entry_for_path(rel_path("other_dir/other_file.txt"))
            .unwrap();

        // Test 1: Single item drag, don't highlight parent directory
        let dragged_selection = DraggedSelection {
            active_selection: SelectedEntry {
                worktree_id,
                entry_id: child_file.id,
            },
            marked_selections: Arc::new([SelectedEntry {
                worktree_id,
                entry_id: child_file.id,
            }]),
            source_pane: None,
            active_selection_is_file: true,
        };
        let result =
            panel.highlight_entry_for_selection_drag(parent_dir, worktree, &dragged_selection, cx);
        assert_eq!(result, None, "Should not highlight parent of dragged item");

        // Test 2: Single item drag, don't highlight sibling files
        let result = panel.highlight_entry_for_selection_drag(
            sibling_file,
            worktree,
            &dragged_selection,
            cx,
        );
        assert_eq!(result, None, "Should not highlight sibling files");

        // Test 3: Single item drag, highlight unrelated directory
        let result =
            panel.highlight_entry_for_selection_drag(other_dir, worktree, &dragged_selection, cx);
        assert_eq!(
            result,
            Some(other_dir.id),
            "Should highlight unrelated directory"
        );

        // Test 4: Single item drag, highlight sibling directory
        let result =
            panel.highlight_entry_for_selection_drag(child_dir, worktree, &dragged_selection, cx);
        assert_eq!(
            result,
            Some(child_dir.id),
            "Should highlight sibling directory"
        );

        // Test 5: Multiple items drag, highlight parent directory
        let dragged_selection = DraggedSelection {
            active_selection: SelectedEntry {
                worktree_id,
                entry_id: child_file.id,
            },
            marked_selections: Arc::new([
                SelectedEntry {
                    worktree_id,
                    entry_id: child_file.id,
                },
                SelectedEntry {
                    worktree_id,
                    entry_id: sibling_file.id,
                },
            ]),
            source_pane: None,
            active_selection_is_file: true,
        };
        let result =
            panel.highlight_entry_for_selection_drag(parent_dir, worktree, &dragged_selection, cx);
        assert_eq!(
            result,
            Some(parent_dir.id),
            "Should highlight parent with multiple items"
        );

        // Test 6: Target is file in different directory, highlight parent
        let result =
            panel.highlight_entry_for_selection_drag(other_file, worktree, &dragged_selection, cx);
        assert_eq!(
            result,
            Some(other_dir.id),
            "Should highlight parent of target file"
        );

        // Test 7: Target is directory, always highlight
        let result =
            panel.highlight_entry_for_selection_drag(child_dir, worktree, &dragged_selection, cx);
        assert_eq!(
            result,
            Some(child_dir.id),
            "Should always highlight directories"
        );
    });
}

#[gpui::test]
async fn test_highlight_entry_for_selection_drag_cross_worktree(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root1",
        json!({
            "src": {
                "main.rs": "",
                "lib.rs": ""
            }
        }),
    )
    .await;
    fs.insert_tree(
        "/root2",
        json!({
            "src": {
                "main.rs": "",
                "test.rs": ""
            }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/root1".as_ref(), "/root2".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    panel.update(cx, |panel, cx| {
        let project = panel.project.read(cx);
        let worktrees: Vec<_> = project.visible_worktrees(cx).collect();

        let worktree_a = &worktrees[0];
        let main_rs_from_a = worktree_a
            .read(cx)
            .entry_for_path(rel_path("src/main.rs"))
            .unwrap();

        let worktree_b = &worktrees[1];
        let src_dir_from_b = worktree_b.read(cx).entry_for_path(rel_path("src")).unwrap();
        let main_rs_from_b = worktree_b
            .read(cx)
            .entry_for_path(rel_path("src/main.rs"))
            .unwrap();

        // Test dragging file from worktree A onto parent of file with same relative path in worktree B
        let dragged_selection = DraggedSelection {
            active_selection: SelectedEntry {
                worktree_id: worktree_a.read(cx).id(),
                entry_id: main_rs_from_a.id,
            },
            marked_selections: Arc::new([SelectedEntry {
                worktree_id: worktree_a.read(cx).id(),
                entry_id: main_rs_from_a.id,
            }]),
            source_pane: None,
            active_selection_is_file: true,
        };

        let result = panel.highlight_entry_for_selection_drag(
            src_dir_from_b,
            worktree_b.read(cx),
            &dragged_selection,
            cx,
        );
        assert_eq!(
            result,
            Some(src_dir_from_b.id),
            "Should highlight target directory from different worktree even with same relative path"
        );

        // Test dragging file from worktree A onto file with same relative path in worktree B
        let result = panel.highlight_entry_for_selection_drag(
            main_rs_from_b,
            worktree_b.read(cx),
            &dragged_selection,
            cx,
        );
        assert_eq!(
            result,
            Some(src_dir_from_b.id),
            "Should highlight parent of target file from different worktree"
        );
    });
}

#[gpui::test]
async fn test_should_highlight_background_for_selection_drag(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root1",
        json!({
            "parent_dir": {
                "child_file.txt": "",
                "nested_dir": {
                    "nested_file.txt": ""
                }
            },
            "root_file.txt": ""
        }),
    )
    .await;

    fs.insert_tree(
        "/root2",
        json!({
            "other_dir": {
                "other_file.txt": ""
            }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/root1".as_ref(), "/root2".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    panel.update(cx, |panel, cx| {
        let project = panel.project.read(cx);
        let worktrees: Vec<_> = project.visible_worktrees(cx).collect();
        let worktree1 = worktrees[0].read(cx);
        let worktree2 = worktrees[1].read(cx);
        let worktree1_id = worktree1.id();
        let _worktree2_id = worktree2.id();

        let root1_entry = worktree1.root_entry().unwrap();
        let root2_entry = worktree2.root_entry().unwrap();
        let _parent_dir = worktree1.entry_for_path(rel_path("parent_dir")).unwrap();
        let child_file = worktree1
            .entry_for_path(rel_path("parent_dir/child_file.txt"))
            .unwrap();
        let nested_file = worktree1
            .entry_for_path(rel_path("parent_dir/nested_dir/nested_file.txt"))
            .unwrap();
        let root_file = worktree1.entry_for_path(rel_path("root_file.txt")).unwrap();

        // Test 1: Multiple entries - should always highlight background
        let multiple_dragged_selection = DraggedSelection {
            active_selection: SelectedEntry {
                worktree_id: worktree1_id,
                entry_id: child_file.id,
            },
            marked_selections: Arc::new([
                SelectedEntry {
                    worktree_id: worktree1_id,
                    entry_id: child_file.id,
                },
                SelectedEntry {
                    worktree_id: worktree1_id,
                    entry_id: nested_file.id,
                },
            ]),
            source_pane: None,
            active_selection_is_file: true,
        };

        let result = panel.should_highlight_background_for_selection_drag(
            &multiple_dragged_selection,
            root1_entry.id,
            cx,
        );
        assert!(result, "Should highlight background for multiple entries");

        // Test 2: Single entry with non-empty parent path - should highlight background
        let nested_dragged_selection = DraggedSelection {
            active_selection: SelectedEntry {
                worktree_id: worktree1_id,
                entry_id: nested_file.id,
            },
            marked_selections: Arc::new([SelectedEntry {
                worktree_id: worktree1_id,
                entry_id: nested_file.id,
            }]),
            source_pane: None,
            active_selection_is_file: true,
        };

        let result = panel.should_highlight_background_for_selection_drag(
            &nested_dragged_selection,
            root1_entry.id,
            cx,
        );
        assert!(result, "Should highlight background for nested file");

        // Test 3: Single entry at root level, same worktree - should NOT highlight background
        let root_file_dragged_selection = DraggedSelection {
            active_selection: SelectedEntry {
                worktree_id: worktree1_id,
                entry_id: root_file.id,
            },
            marked_selections: Arc::new([SelectedEntry {
                worktree_id: worktree1_id,
                entry_id: root_file.id,
            }]),
            source_pane: None,
            active_selection_is_file: true,
        };

        let result = panel.should_highlight_background_for_selection_drag(
            &root_file_dragged_selection,
            root1_entry.id,
            cx,
        );
        assert!(
            !result,
            "Should NOT highlight background for root file in same worktree"
        );

        // Test 4: Single entry at root level, different worktree - should highlight background
        let result = panel.should_highlight_background_for_selection_drag(
            &root_file_dragged_selection,
            root2_entry.id,
            cx,
        );
        assert!(
            result,
            "Should highlight background for root file from different worktree"
        );

        // Test 5: Single entry in subdirectory - should highlight background
        let child_file_dragged_selection = DraggedSelection {
            active_selection: SelectedEntry {
                worktree_id: worktree1_id,
                entry_id: child_file.id,
            },
            marked_selections: Arc::new([SelectedEntry {
                worktree_id: worktree1_id,
                entry_id: child_file.id,
            }]),
            source_pane: None,
            active_selection_is_file: true,
        };

        let result = panel.should_highlight_background_for_selection_drag(
            &child_file_dragged_selection,
            root1_entry.id,
            cx,
        );
        assert!(
            result,
            "Should highlight background for file with non-empty parent path"
        );
    });
}

#[gpui::test]
async fn test_hide_root(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root1",
        json!({
            "dir1": {
                "file1.txt": "content",
                "file2.txt": "content",
            },
            "dir2": {
                "file3.txt": "content",
            },
            "file4.txt": "content",
        }),
    )
    .await;

    fs.insert_tree(
        "/root2",
        json!({
            "dir3": {
                "file5.txt": "content",
            },
            "file6.txt": "content",
        }),
    )
    .await;

    // Test 1: Single worktree with hide_root = false
    {
        let project = Project::test(fs.clone(), ["/root1".as_ref()], cx).await;
        let window =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = window
            .read_with(cx, |mw, _| mw.workspace().clone())
            .unwrap();
        let cx = &mut VisualTestContext::from_window(window.into(), cx);

        cx.update(|_, cx| {
            let settings = *ProjectPanelSettings::get_global(cx);
            ProjectPanelSettings::override_global(
                ProjectPanelSettings {
                    hide_root: false,
                    ..settings
                },
                cx,
            );
        });

        let panel = workspace.update_in(cx, ProjectPanel::new);
        cx.run_until_parked();

        #[rustfmt::skip]
        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            &[
                "v root1",
                "    > dir1",
                "    > dir2",
                "      file4.txt",
            ],
            "With hide_root=false and single worktree, root should be visible"
        );
    }

    // Test 2: Single worktree with hide_root = true
    {
        let project = Project::test(fs.clone(), ["/root1".as_ref()], cx).await;
        let window =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = window
            .read_with(cx, |mw, _| mw.workspace().clone())
            .unwrap();
        let cx = &mut VisualTestContext::from_window(window.into(), cx);

        // Set hide_root to true
        cx.update(|_, cx| {
            let settings = *ProjectPanelSettings::get_global(cx);
            ProjectPanelSettings::override_global(
                ProjectPanelSettings {
                    hide_root: true,
                    ..settings
                },
                cx,
            );
        });

        let panel = workspace.update_in(cx, ProjectPanel::new);
        cx.run_until_parked();

        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            &["> dir1", "> dir2", "  file4.txt",],
            "With hide_root=true and single worktree, root should be hidden"
        );

        // Test expanding directories still works without root
        toggle_expand_dir(&panel, "root1/dir1", cx);
        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            &[
                "v dir1  <== selected",
                "      file1.txt",
                "      file2.txt",
                "> dir2",
                "  file4.txt",
            ],
            "Should be able to expand directories even when root is hidden"
        );
    }

    // Test 3: Multiple worktrees with hide_root = true
    {
        let project = Project::test(fs.clone(), ["/root1".as_ref(), "/root2".as_ref()], cx).await;
        let window =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = window
            .read_with(cx, |mw, _| mw.workspace().clone())
            .unwrap();
        let cx = &mut VisualTestContext::from_window(window.into(), cx);

        // Set hide_root to true
        cx.update(|_, cx| {
            let settings = *ProjectPanelSettings::get_global(cx);
            ProjectPanelSettings::override_global(
                ProjectPanelSettings {
                    hide_root: true,
                    ..settings
                },
                cx,
            );
        });

        let panel = workspace.update_in(cx, ProjectPanel::new);
        cx.run_until_parked();

        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            &[
                "v root1",
                "    > dir1",
                "    > dir2",
                "      file4.txt",
                "v root2",
                "    > dir3",
                "      file6.txt",
            ],
            "With hide_root=true and multiple worktrees, roots should still be visible"
        );
    }

    // Test 4: Multiple worktrees with hide_root = false
    {
        let project = Project::test(fs.clone(), ["/root1".as_ref(), "/root2".as_ref()], cx).await;
        let window =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = window
            .read_with(cx, |mw, _| mw.workspace().clone())
            .unwrap();
        let cx = &mut VisualTestContext::from_window(window.into(), cx);

        cx.update(|_, cx| {
            let settings = *ProjectPanelSettings::get_global(cx);
            ProjectPanelSettings::override_global(
                ProjectPanelSettings {
                    hide_root: false,
                    ..settings
                },
                cx,
            );
        });

        let panel = workspace.update_in(cx, ProjectPanel::new);
        cx.run_until_parked();

        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            &[
                "v root1",
                "    > dir1",
                "    > dir2",
                "      file4.txt",
                "v root2",
                "    > dir3",
                "      file6.txt",
            ],
            "With hide_root=false and multiple worktrees, roots should be visible"
        );
    }
}

#[gpui::test]
async fn test_compare_selected_files(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "file1.txt": "content of file1",
            "file2.txt": "content of file2",
            "dir1": {
                "file3.txt": "content of file3"
            }
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

    let file1_path = "root/file1.txt";
    let file2_path = "root/file2.txt";
    select_path_with_mark(&panel, file1_path, cx);
    select_path_with_mark(&panel, file2_path, cx);

    panel.update_in(cx, |panel, window, cx| {
        panel.compare_marked_files(&CompareMarkedFiles, window, cx);
    });
    cx.executor().run_until_parked();

    workspace.update_in(cx, |workspace, _, cx| {
        let active_items = workspace
            .panes()
            .iter()
            .filter_map(|pane| pane.read(cx).active_item())
            .collect::<Vec<_>>();
        assert_eq!(active_items.len(), 1);
        let diff_view = active_items
            .into_iter()
            .next()
            .unwrap()
            .downcast::<FileDiffView>()
            .expect("Open item should be an FileDiffView");
        assert_eq!(diff_view.tab_content_text(0, cx), "file1.txt ↔ file2.txt");
        assert_eq!(
            diff_view.tab_tooltip_text(cx).unwrap(),
            format!(
                "{} ↔ {}",
                rel_path(file1_path).display(PathStyle::local()),
                rel_path(file2_path).display(PathStyle::local())
            )
        );
    });

    let file1_entry_id = find_project_entry(&panel, file1_path, cx).unwrap();
    let file2_entry_id = find_project_entry(&panel, file2_path, cx).unwrap();
    let worktree_id = panel.update(cx, |panel, cx| {
        panel
            .project
            .read(cx)
            .worktrees(cx)
            .next()
            .unwrap()
            .read(cx)
            .id()
    });

    let expected_entries = [
        SelectedEntry {
            worktree_id,
            entry_id: file1_entry_id,
        },
        SelectedEntry {
            worktree_id,
            entry_id: file2_entry_id,
        },
    ];
    panel.update(cx, |panel, _cx| {
        assert_eq!(
            &panel.marked_entries, &expected_entries,
            "Should keep marked entries after comparison"
        );
    });

    panel.update(cx, |panel, cx| {
        panel.project.update(cx, |_, cx| {
            cx.emit(project::Event::RevealInProjectPanel(file2_entry_id))
        })
    });

    panel.update(cx, |panel, _cx| {
        assert_eq!(
            &panel.marked_entries, &expected_entries,
            "Marked entries should persist after focusing back on the project panel"
        );
    });
}

#[gpui::test]
async fn test_compare_files_context_menu(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "file1.txt": "content of file1",
            "file2.txt": "content of file2",
            "dir1": {},
            "dir2": {
                "file3.txt": "content of file3"
            }
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

    // Test 1: When only one file is selected, there should be no compare option
    select_path(&panel, "root/file1.txt", cx);

    let selected_files = panel.update(cx, |panel, cx| panel.file_abs_paths_to_diff(cx));
    assert_eq!(
        selected_files, None,
        "Should not have compare option when only one file is selected"
    );

    // Test 2: When multiple files are selected, there should be a compare option
    select_path_with_mark(&panel, "root/file1.txt", cx);
    select_path_with_mark(&panel, "root/file2.txt", cx);

    let selected_files = panel.update(cx, |panel, cx| panel.file_abs_paths_to_diff(cx));
    assert!(
        selected_files.is_some(),
        "Should have files selected for comparison"
    );
    if let Some((file1, file2)) = selected_files {
        assert!(
            file1.to_string_lossy().ends_with("file1.txt")
                && file2.to_string_lossy().ends_with("file2.txt"),
            "Should have file1.txt and file2.txt as the selected files when multi-selecting"
        );
    }

    // Test 3: Selecting a directory shouldn't count as a comparable file
    select_path_with_mark(&panel, "root/dir1", cx);

    let selected_files = panel.update(cx, |panel, cx| panel.file_abs_paths_to_diff(cx));
    assert!(
        selected_files.is_some(),
        "Directory selection should not affect comparable files"
    );
    if let Some((file1, file2)) = selected_files {
        assert!(
            file1.to_string_lossy().ends_with("file1.txt")
                && file2.to_string_lossy().ends_with("file2.txt"),
            "Selecting a directory should not affect the number of comparable files"
        );
    }

    // Test 4: Selecting one more file
    select_path_with_mark(&panel, "root/dir2/file3.txt", cx);

    let selected_files = panel.update(cx, |panel, cx| panel.file_abs_paths_to_diff(cx));
    assert!(
        selected_files.is_some(),
        "Directory selection should not affect comparable files"
    );
    if let Some((file1, file2)) = selected_files {
        assert!(
            file1.to_string_lossy().ends_with("file2.txt")
                && file2.to_string_lossy().ends_with("file3.txt"),
            "Selecting a directory should not affect the number of comparable files"
        );
    }
}

#[gpui::test]
async fn test_reveal_in_file_manager_path_falls_back_to_worktree_root(
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "file.txt": "content",
            "dir": {},
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

    select_path(&panel, "root/file.txt", cx);
    let selected_reveal_path = panel
        .update(cx, |panel, cx| panel.reveal_in_file_manager_path(cx))
        .expect("selected entry should produce a reveal path");
    assert!(
        selected_reveal_path.ends_with(Path::new("file.txt")),
        "Expected selected file path, got {:?}",
        selected_reveal_path
    );

    panel.update(cx, |panel, _| {
        panel.selection = None;
        panel.marked_entries.clear();
    });
    let fallback_reveal_path = panel
        .update(cx, |panel, cx| panel.reveal_in_file_manager_path(cx))
        .expect("project root should be used when selection is empty");
    assert!(
        fallback_reveal_path.ends_with(Path::new("root")),
        "Expected worktree root path, got {:?}",
        fallback_reveal_path
    );
}

#[gpui::test]
async fn test_hide_hidden_entries(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            ".hidden-file.txt": "hidden file content",
            "visible-file.txt": "visible file content",
            ".hidden-parent-dir": {
                "nested-dir": {
                    "file.txt": "file content",
                }
            },
            "visible-dir": {
                "file-in-visible.txt": "file content",
                "nested": {
                    ".hidden-nested-dir": {
                        ".double-hidden-dir": {
                            "deep-file-1.txt": "deep content 1",
                            "deep-file-2.txt": "deep content 2"
                        },
                        "hidden-nested-file-1.txt": "hidden nested 1",
                        "hidden-nested-file-2.txt": "hidden nested 2"
                    },
                    "visible-nested-file.txt": "visible nested content"
                }
            }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/root".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.update(|_, cx| {
        let settings = *ProjectPanelSettings::get_global(cx);
        ProjectPanelSettings::override_global(
            ProjectPanelSettings {
                hide_hidden: false,
                ..settings
            },
            cx,
        );
    });

    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    toggle_expand_dir(&panel, "root/.hidden-parent-dir", cx);
    toggle_expand_dir(&panel, "root/.hidden-parent-dir/nested-dir", cx);
    toggle_expand_dir(&panel, "root/visible-dir", cx);
    toggle_expand_dir(&panel, "root/visible-dir/nested", cx);
    toggle_expand_dir(&panel, "root/visible-dir/nested/.hidden-nested-dir", cx);
    toggle_expand_dir(
        &panel,
        "root/visible-dir/nested/.hidden-nested-dir/.double-hidden-dir",
        cx,
    );

    let expanded = [
        "v root",
        "    v .hidden-parent-dir",
        "        v nested-dir",
        "              file.txt",
        "    v visible-dir",
        "        v nested",
        "            v .hidden-nested-dir",
        "                v .double-hidden-dir  <== selected",
        "                      deep-file-1.txt",
        "                      deep-file-2.txt",
        "                  hidden-nested-file-1.txt",
        "                  hidden-nested-file-2.txt",
        "              visible-nested-file.txt",
        "          file-in-visible.txt",
        "      .hidden-file.txt",
        "      visible-file.txt",
    ];

    assert_eq!(
        visible_entries_as_strings(&panel, 0..30, cx),
        &expanded,
        "With hide_hidden=false, contents of hidden nested directory should be visible"
    );

    cx.update(|_, cx| {
        let settings = *ProjectPanelSettings::get_global(cx);
        ProjectPanelSettings::override_global(
            ProjectPanelSettings {
                hide_hidden: true,
                ..settings
            },
            cx,
        );
    });

    panel.update_in(cx, |panel, window, cx| {
        panel.update_visible_entries(None, false, false, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..30, cx),
        &[
            "v root",
            "    v visible-dir",
            "        v nested",
            "              visible-nested-file.txt",
            "          file-in-visible.txt",
            "      visible-file.txt",
        ],
        "With hide_hidden=false, contents of hidden nested directory should be visible"
    );

    panel.update_in(cx, |panel, window, cx| {
        let settings = *ProjectPanelSettings::get_global(cx);
        ProjectPanelSettings::override_global(
            ProjectPanelSettings {
                hide_hidden: false,
                ..settings
            },
            cx,
        );
        panel.update_visible_entries(None, false, false, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..30, cx),
        &expanded,
        "With hide_hidden=false, deeply nested hidden directories and their contents should be visible"
    );
}

pub(crate) fn select_path(panel: &Entity<ProjectPanel>, path: &str, cx: &mut VisualTestContext) {
    let path = rel_path(path);
    panel.update_in(cx, |panel, window, cx| {
        for worktree in panel.project.read(cx).worktrees(cx).collect::<Vec<_>>() {
            let worktree = worktree.read(cx);
            if let Ok(relative_path) = path.strip_prefix(worktree.root_name()) {
                let entry_id = worktree.entry_for_path(relative_path).unwrap().id;
                panel.update_visible_entries(
                    Some((worktree.id(), entry_id)),
                    false,
                    false,
                    window,
                    cx,
                );
                return;
            }
        }
        panic!("no worktree for path {:?}", path);
    });
    cx.run_until_parked();
}

pub(crate) fn select_path_with_mark(
    panel: &Entity<ProjectPanel>,
    path: &str,
    cx: &mut VisualTestContext,
) {
    let path = rel_path(path);
    panel.update(cx, |panel, cx| {
        for worktree in panel.project.read(cx).worktrees(cx).collect::<Vec<_>>() {
            let worktree = worktree.read(cx);
            if let Ok(relative_path) = path.strip_prefix(worktree.root_name()) {
                let entry_id = worktree.entry_for_path(relative_path).unwrap().id;
                let entry = crate::SelectedEntry {
                    worktree_id: worktree.id(),
                    entry_id,
                };
                if !panel.marked_entries.contains(&entry) {
                    panel.marked_entries.push(entry);
                }
                panel.selection = Some(entry);
                return;
            }
        }
        panic!("no worktree for path {:?}", path);
    });
}

/// `leaf_path` is the full path to the leaf entry (e.g., "root/a/b/c")
/// `active_ancestor_path` is the path to the folded component that should be active.
fn select_folded_path_with_mark(
    panel: &Entity<ProjectPanel>,
    leaf_path: &str,
    active_ancestor_path: &str,
    cx: &mut VisualTestContext,
) {
    select_path_with_mark(panel, leaf_path, cx);
    set_folded_active_ancestor(panel, leaf_path, active_ancestor_path, cx);
}

fn set_folded_active_ancestor(
    panel: &Entity<ProjectPanel>,
    leaf_path: &str,
    active_ancestor_path: &str,
    cx: &mut VisualTestContext,
) {
    let leaf_path = rel_path(leaf_path);
    let active_ancestor_path = rel_path(active_ancestor_path);
    panel.update(cx, |panel, cx| {
        let mut leaf_entry_id = None;
        let mut target_entry_id = None;

        for worktree in panel.project.read(cx).worktrees(cx).collect::<Vec<_>>() {
            let worktree = worktree.read(cx);
            if let Ok(relative_path) = leaf_path.strip_prefix(worktree.root_name()) {
                leaf_entry_id = worktree.entry_for_path(relative_path).map(|entry| entry.id);
            }
            if let Ok(relative_path) = active_ancestor_path.strip_prefix(worktree.root_name()) {
                target_entry_id = worktree.entry_for_path(relative_path).map(|entry| entry.id);
            }
        }

        let leaf_entry_id =
            leaf_entry_id.unwrap_or_else(|| panic!("no entry for leaf path {leaf_path:?}"));
        let target_entry_id = target_entry_id
            .unwrap_or_else(|| panic!("no entry for active path {active_ancestor_path:?}"));
        let folded_ancestors = panel
            .state
            .ancestors
            .get_mut(&leaf_entry_id)
            .unwrap_or_else(|| panic!("leaf path {leaf_path:?} should be folded"));
        let ancestor_ids = folded_ancestors.ancestors.clone();

        let mut depth_for_target = None;
        for depth in 0..ancestor_ids.len() {
            let resolved_entry_id = if depth == 0 {
                leaf_entry_id
            } else {
                ancestor_ids.get(depth).copied().unwrap_or(leaf_entry_id)
            };
            if resolved_entry_id == target_entry_id {
                depth_for_target = Some(depth);
                break;
            }
        }

        folded_ancestors.current_ancestor_depth = depth_for_target.unwrap_or_else(|| {
            panic!(
                "active path {active_ancestor_path:?} is not part of folded ancestors {ancestor_ids:?}"
            )
        });
    });
}

pub(crate) fn drag_selection_to(
    panel: &Entity<ProjectPanel>,
    target_path: &str,
    is_file: bool,
    cx: &mut VisualTestContext,
) {
    let target_entry = find_project_entry(panel, target_path, cx)
        .unwrap_or_else(|| panic!("no entry for target path {target_path:?}"));

    panel.update_in(cx, |panel, window, cx| {
        let selection = panel
            .selection
            .expect("a selection is required before dragging");
        let drag = DraggedSelection {
            active_selection: SelectedEntry {
                worktree_id: selection.worktree_id,
                entry_id: panel.resolve_entry(selection.entry_id),
            },
            marked_selections: Arc::from(panel.marked_entries.clone()),
            source_pane: None,
            active_selection_is_file: true,
        };
        panel.drag_onto(&drag, target_entry, is_file, window, cx);
    });
    cx.executor().run_until_parked();
}

pub(crate) fn find_project_entry(
    panel: &Entity<ProjectPanel>,
    path: &str,
    cx: &mut VisualTestContext,
) -> Option<ProjectEntryId> {
    let path = rel_path(path);
    panel.update(cx, |panel, cx| {
        for worktree in panel.project.read(cx).worktrees(cx).collect::<Vec<_>>() {
            let worktree = worktree.read(cx);
            if let Ok(relative_path) = path.strip_prefix(worktree.root_name()) {
                return worktree.entry_for_path(relative_path).map(|entry| entry.id);
            }
        }
        panic!("no worktree for path {path:?}");
    })
}

fn visible_entries_as_strings(
    panel: &Entity<ProjectPanel>,
    range: Range<usize>,
    cx: &mut VisualTestContext,
) -> Vec<String> {
    let mut result = Vec::new();
    let mut project_entries = HashSet::default();
    let mut has_editor = false;

    panel.update_in(cx, |panel, window, cx| {
        panel.for_each_visible_entry(range, window, cx, &mut |project_entry, details, _, _| {
            if details.is_editing {
                assert!(!has_editor, "duplicate editor entry");
                has_editor = true;
            } else {
                assert!(
                    project_entries.insert(project_entry),
                    "duplicate project entry {:?} {:?}",
                    project_entry,
                    details
                );
            }

            let indent = "    ".repeat(details.depth);
            let icon = if details.kind.is_dir() {
                if details.is_expanded { "v " } else { "> " }
            } else {
                "  "
            };
            #[cfg(windows)]
            let filename = details.filename.replace("\\", "/");
            #[cfg(not(windows))]
            let filename = details.filename;
            let name = if details.is_editing {
                format!("[EDITOR: '{}']", filename)
            } else if details.is_processing {
                format!("[PROCESSING: '{}']", filename)
            } else {
                filename
            };
            let selected = if details.is_selected {
                "  <== selected"
            } else {
                ""
            };
            let marked = if details.is_marked {
                "  <== marked"
            } else {
                ""
            };

            result.push(format!("{indent}{icon}{name}{selected}{marked}"));
        });
    });

    result
}

/// Test that missing sort_mode field defaults to DirectoriesFirst
#[gpui::test]
async fn test_sort_mode_default_fallback(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    // Verify that when sort_mode is not specified, it defaults to DirectoriesFirst
    let default_settings = cx.read(|cx| *ProjectPanelSettings::get_global(cx));
    assert_eq!(
        default_settings.sort_mode,
        settings::ProjectPanelSortMode::DirectoriesFirst,
        "sort_mode should default to DirectoriesFirst"
    );
}

/// Test sort modes: DirectoriesFirst (default) vs Mixed
#[gpui::test]
async fn test_sort_mode_directories_first(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "zebra.txt": "",
            "Apple": {},
            "banana.rs": "",
            "Carrot": {},
            "aardvark.txt": "",
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

    // Default sort mode should be DirectoriesFirst
    assert_eq!(
        visible_entries_as_strings(&panel, 0..50, cx),
        &[
            "v root",
            "    > Apple",
            "    > Carrot",
            "      aardvark.txt",
            "      banana.rs",
            "      zebra.txt",
        ]
    );
}

#[gpui::test]
async fn test_sort_mode_mixed(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "Zebra.txt": "",
            "apple": {},
            "Banana.rs": "",
            "carrot": {},
            "Aardvark.txt": "",
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/root".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    // Switch to Mixed mode
    cx.update(|_, cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project_panel.get_or_insert_default().sort_mode =
                    Some(settings::ProjectPanelSortMode::Mixed);
            });
        });
    });

    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    // Mixed mode: case-insensitive sorting
    // Aardvark < apple < Banana < carrot < Zebra (all case-insensitive)
    assert_eq!(
        visible_entries_as_strings(&panel, 0..50, cx),
        &[
            "v root",
            "      Aardvark.txt",
            "    > apple",
            "      Banana.rs",
            "    > carrot",
            "      Zebra.txt",
        ]
    );
}

#[gpui::test]
async fn test_sort_mode_files_first(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "Zebra.txt": "",
            "apple": {},
            "Banana.rs": "",
            "carrot": {},
            "Aardvark.txt": "",
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/root".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    // Switch to FilesFirst mode
    cx.update(|_, cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project_panel.get_or_insert_default().sort_mode =
                    Some(settings::ProjectPanelSortMode::FilesFirst);
            });
        });
    });

    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    // FilesFirst mode: files first, then directories (both case-insensitive)
    assert_eq!(
        visible_entries_as_strings(&panel, 0..50, cx),
        &[
            "v root",
            "      Aardvark.txt",
            "      Banana.rs",
            "      Zebra.txt",
            "    > apple",
            "    > carrot",
        ]
    );
}

#[gpui::test]
async fn test_sort_mode_toggle(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "file2.txt": "",
            "dir1": {},
            "file1.txt": "",
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

    // Initially DirectoriesFirst
    assert_eq!(
        visible_entries_as_strings(&panel, 0..50, cx),
        &["v root", "    > dir1", "      file1.txt", "      file2.txt",]
    );

    // Toggle to Mixed
    cx.update(|_, cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project_panel.get_or_insert_default().sort_mode =
                    Some(settings::ProjectPanelSortMode::Mixed);
            });
        });
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..50, cx),
        &["v root", "    > dir1", "      file1.txt", "      file2.txt",]
    );

    // Toggle back to DirectoriesFirst
    cx.update(|_, cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project_panel.get_or_insert_default().sort_mode =
                    Some(settings::ProjectPanelSortMode::DirectoriesFirst);
            });
        });
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..50, cx),
        &["v root", "    > dir1", "      file1.txt", "      file2.txt",]
    );
}

#[gpui::test]
async fn test_ensure_temporary_folding_when_creating_in_different_nested_dirs(
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx);

    // parent: accept
    run_create_file_in_folded_path_case(
        "parent",
        "root1/parent",
        "file_in_parent.txt",
        &[
            "v root1",
            "    v parent",
            "        > subdir/child",
            "          [EDITOR: '']  <== selected",
        ],
        &[
            "v root1",
            "    v parent",
            "        > subdir/child",
            "          file_in_parent.txt  <== selected  <== marked",
        ],
        true,
        cx,
    )
    .await;

    // parent: cancel
    run_create_file_in_folded_path_case(
        "parent",
        "root1/parent",
        "file_in_parent.txt",
        &[
            "v root1",
            "    v parent",
            "        > subdir/child",
            "          [EDITOR: '']  <== selected",
        ],
        &["v root1", "    > parent/subdir/child  <== selected"],
        false,
        cx,
    )
    .await;

    // subdir: accept
    run_create_file_in_folded_path_case(
        "subdir",
        "root1/parent/subdir",
        "file_in_subdir.txt",
        &[
            "v root1",
            "    v parent/subdir",
            "        > child",
            "          [EDITOR: '']  <== selected",
        ],
        &[
            "v root1",
            "    v parent/subdir",
            "        > child",
            "          file_in_subdir.txt  <== selected  <== marked",
        ],
        true,
        cx,
    )
    .await;

    // subdir: cancel
    run_create_file_in_folded_path_case(
        "subdir",
        "root1/parent/subdir",
        "file_in_subdir.txt",
        &[
            "v root1",
            "    v parent/subdir",
            "        > child",
            "          [EDITOR: '']  <== selected",
        ],
        &["v root1", "    > parent/subdir/child  <== selected"],
        false,
        cx,
    )
    .await;

    // child: accept
    run_create_file_in_folded_path_case(
        "child",
        "root1/parent/subdir/child",
        "file_in_child.txt",
        &[
            "v root1",
            "    v parent/subdir/child",
            "          [EDITOR: '']  <== selected",
        ],
        &[
            "v root1",
            "    v parent/subdir/child",
            "          file_in_child.txt  <== selected  <== marked",
        ],
        true,
        cx,
    )
    .await;

    // child: cancel
    run_create_file_in_folded_path_case(
        "child",
        "root1/parent/subdir/child",
        "file_in_child.txt",
        &[
            "v root1",
            "    v parent/subdir/child",
            "          [EDITOR: '']  <== selected",
        ],
        &["v root1", "    v parent/subdir/child  <== selected"],
        false,
        cx,
    )
    .await;
}

#[gpui::test]
async fn test_preserve_temporary_unfolded_active_index_on_blur_from_context_menu(
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root1",
        json!({
            "parent": {
                "subdir": {
                    "child": {},
                }
            }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/root1".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    let panel = workspace.update_in(cx, |workspace, window, cx| {
        let panel = ProjectPanel::new(workspace, window, cx);
        workspace.add_panel(panel.clone(), window, cx);
        panel
    });

    cx.update(|_, cx| {
        let settings = *ProjectPanelSettings::get_global(cx);
        ProjectPanelSettings::override_global(
            ProjectPanelSettings {
                auto_fold_dirs: true,
                ..settings
            },
            cx,
        );
    });

    panel.update_in(cx, |panel, window, cx| {
        panel.collapse_all_entries(&CollapseAllEntries, window, cx);
    });
    cx.run_until_parked();

    select_folded_path_with_mark(
        &panel,
        "root1/parent/subdir/child",
        "root1/parent/subdir",
        cx,
    );
    panel.update(cx, |panel, _| {
        panel.marked_entries.clear();
    });

    let parent_entry_id = find_project_entry(&panel, "root1/parent", cx)
        .expect("parent directory should exist for this test");
    let subdir_entry_id = find_project_entry(&panel, "root1/parent/subdir", cx)
        .expect("subdir directory should exist for this test");
    let child_entry_id = find_project_entry(&panel, "root1/parent/subdir/child", cx)
        .expect("child directory should exist for this test");

    panel.update(cx, |panel, _| {
        let selection = panel
            .selection
            .expect("leaf directory should be selected before creating a new entry");
        assert_eq!(
            selection.entry_id, child_entry_id,
            "initial selection should be the folded leaf entry"
        );
        assert_eq!(
            panel.resolve_entry(selection.entry_id),
            subdir_entry_id,
            "active folded component should start at subdir"
        );
    });

    panel.update_in(cx, |panel, window, cx| {
        panel.deploy_context_menu(
            gpui::point(gpui::px(1.), gpui::px(1.)),
            child_entry_id,
            window,
            cx,
        );
        panel.new_file(&NewFile, window, cx);
    });
    cx.run_until_parked();
    panel.update_in(cx, |panel, window, cx| {
        assert!(panel.filename_editor.read(cx).is_focused(window));
    });
    cx.run_until_parked();

    set_folded_active_ancestor(&panel, "root1/parent/subdir", "root1/parent", cx);

    panel.update_in(cx, |panel, window, cx| {
        panel.deploy_context_menu(
            gpui::point(gpui::px(2.), gpui::px(2.)),
            subdir_entry_id,
            window,
            cx,
        );
    });
    cx.run_until_parked();

    panel.update(cx, |panel, _| {
        assert!(
            panel.state.edit_state.is_none(),
            "opening another context menu should blur the filename editor and discard edit state"
        );
        let selection = panel
            .selection
            .expect("selection should restore to the previously focused leaf entry");
        assert_eq!(
            selection.entry_id, child_entry_id,
            "blur-driven cancellation should restore the previous leaf selection"
        );
        assert_eq!(
            panel.resolve_entry(selection.entry_id),
            parent_entry_id,
            "temporary unfolded pending state should preserve the active ancestor chosen before blur"
        );
    });

    panel.update_in(cx, |panel, window, cx| {
        panel.new_file(&NewFile, window, cx);
    });
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root1",
            "    v parent",
            "        > subdir/child",
            "          [EDITOR: '']  <== selected",
        ],
        "new file after blur should use the preserved active ancestor"
    );
    panel.update(cx, |panel, _| {
        let edit_state = panel
            .state
            .edit_state
            .as_ref()
            .expect("new file should enter edit state");
        assert_eq!(
            edit_state.temporarily_unfolded,
            Some(parent_entry_id),
            "temporary unfolding should now target parent after restoring the active ancestor"
        );
    });

    let file_name = "created_after_blur.txt";
    panel
        .update_in(cx, |panel, window, cx| {
            panel.filename_editor.update(cx, |editor, cx| {
                editor.set_text(file_name, window, cx);
            });
            panel.confirm_edit(true, window, cx).expect(
                "confirm_edit should start creation for the file created after blur transition",
            )
        })
        .await
        .expect("creating file after blur transition should succeed");
    cx.run_until_parked();

    assert!(
        fs.is_file(Path::new("/root1/parent/created_after_blur.txt"))
            .await,
        "file should be created under parent after active ancestor is restored to parent"
    );
    assert!(
        !fs.is_file(Path::new("/root1/parent/subdir/created_after_blur.txt"))
            .await,
        "file should not be created under subdir when parent is the active ancestor"
    );
}

async fn run_create_file_in_folded_path_case(
    case_name: &str,
    active_ancestor_path: &str,
    created_file_name: &str,
    expected_temporary_state: &[&str],
    expected_final_state: &[&str],
    accept_creation: bool,
    cx: &mut gpui::TestAppContext,
) {
    let expected_collapsed_state = &["v root1", "    > parent/subdir/child  <== selected"];

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root1",
        json!({
            "parent": {
                "subdir": {
                    "child": {},
                }
            }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/root1".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    let panel = workspace.update_in(cx, |workspace, window, cx| {
        let panel = ProjectPanel::new(workspace, window, cx);
        workspace.add_panel(panel.clone(), window, cx);
        panel
    });

    cx.update(|_, cx| {
        let settings = *ProjectPanelSettings::get_global(cx);
        ProjectPanelSettings::override_global(
            ProjectPanelSettings {
                auto_fold_dirs: true,
                ..settings
            },
            cx,
        );
    });

    panel.update_in(cx, |panel, window, cx| {
        panel.collapse_all_entries(&CollapseAllEntries, window, cx);
    });
    cx.run_until_parked();

    select_folded_path_with_mark(
        &panel,
        "root1/parent/subdir/child",
        active_ancestor_path,
        cx,
    );
    panel.update(cx, |panel, _| {
        panel.marked_entries.clear();
    });

    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        expected_collapsed_state,
        "case '{}' should start from a folded state",
        case_name
    );

    panel.update_in(cx, |panel, window, cx| {
        panel.new_file(&NewFile, window, cx);
    });
    cx.run_until_parked();
    panel.update_in(cx, |panel, window, cx| {
        assert!(panel.filename_editor.read(cx).is_focused(window));
    });
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        expected_temporary_state,
        "case '{}' ({}) should temporarily unfold the active ancestor while editing",
        case_name,
        if accept_creation { "accept" } else { "cancel" }
    );

    let relative_directory = active_ancestor_path
        .strip_prefix("root1/")
        .expect("active_ancestor_path should start with root1/");
    let created_file_path = PathBuf::from("/root1")
        .join(relative_directory)
        .join(created_file_name);

    if accept_creation {
        panel
            .update_in(cx, |panel, window, cx| {
                panel.filename_editor.update(cx, |editor, cx| {
                    editor.set_text(created_file_name, window, cx);
                });
                panel.confirm_edit(true, window, cx).unwrap()
            })
            .await
            .unwrap();
        cx.run_until_parked();

        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            expected_final_state,
            "case '{}' should keep the newly created file selected and marked after accept",
            case_name
        );
        assert!(
            fs.is_file(created_file_path.as_path()).await,
            "case '{}' should create file '{}'",
            case_name,
            created_file_path.display()
        );
    } else {
        panel.update_in(cx, |panel, window, cx| {
            panel.cancel(&Cancel, window, cx);
        });
        cx.run_until_parked();

        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            expected_final_state,
            "case '{}' should keep the expected panel state after cancel",
            case_name
        );
        assert!(
            !fs.is_file(created_file_path.as_path()).await,
            "case '{}' should not create a file after cancel",
            case_name
        );
    }
}

pub(crate) fn init_test(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
        theme_settings::init(theme::LoadThemes::JustBase, cx);
        crate::init(cx);

        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings
                    .project_panel
                    .get_or_insert_default()
                    .auto_fold_dirs = Some(false);
                settings.project.worktree.file_scan_exclusions = Some(Vec::new());
            });
        });
    });
}

fn init_test_with_editor(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let app_state = AppState::test(cx);
        theme_settings::init(theme::LoadThemes::JustBase, cx);
        editor::init(cx);
        crate::init(cx);
        workspace::init(app_state, cx);

        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings
                    .project_panel
                    .get_or_insert_default()
                    .auto_fold_dirs = Some(false);
                settings.project.worktree.file_scan_exclusions = Some(Vec::new())
            });
        });
    });
}

fn init_test_with_git_ui(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let app_state = AppState::test(cx);
        theme_settings::init(theme::LoadThemes::JustBase, cx);
        editor::init(cx);
        git_ui::init(cx);
        crate::init(cx);
        workspace::init(app_state, cx);

        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings
                    .project_panel
                    .get_or_insert_default()
                    .auto_fold_dirs = Some(false);
                settings.project.worktree.file_scan_exclusions = Some(Vec::new())
            });
        });
    });
}

fn set_auto_open_settings(
    cx: &mut TestAppContext,
    auto_open_settings: ProjectPanelAutoOpenSettings,
) {
    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project_panel.get_or_insert_default().auto_open = Some(auto_open_settings);
            });
        })
    });
}

fn ensure_single_file_is_opened(
    workspace: &Entity<Workspace>,
    expected_path: &str,
    cx: &mut VisualTestContext,
) {
    workspace.update_in(cx, |workspace, _, cx| {
        let worktrees = workspace.worktrees(cx).collect::<Vec<_>>();
        assert_eq!(worktrees.len(), 1);
        let worktree_id = worktrees[0].read(cx).id();

        let open_project_paths = workspace
            .panes()
            .iter()
            .filter_map(|pane| pane.read(cx).active_item()?.project_path(cx))
            .collect::<Vec<_>>();
        assert_eq!(
            open_project_paths,
            vec![ProjectPath {
                worktree_id,
                path: Arc::from(rel_path(expected_path))
            }],
            "Should have opened file, selected in project panel"
        );
    });
}

fn submit_deletion(panel: &Entity<ProjectPanel>, cx: &mut VisualTestContext) {
    assert!(
        !cx.has_pending_prompt(),
        "Should have no prompts before the deletion"
    );
    panel.update_in(cx, |panel, window, cx| {
        panel.delete(&Delete { skip_prompt: false }, window, cx)
    });
    assert!(
        cx.has_pending_prompt(),
        "Should have a prompt after the deletion"
    );
    cx.simulate_prompt_answer("Delete");
    assert!(
        !cx.has_pending_prompt(),
        "Should have no prompts after prompt was replied to"
    );
    cx.executor().run_until_parked();
}

fn submit_deletion_skipping_prompt(panel: &Entity<ProjectPanel>, cx: &mut VisualTestContext) {
    assert!(
        !cx.has_pending_prompt(),
        "Should have no prompts before the deletion"
    );
    panel.update_in(cx, |panel, window, cx| {
        panel.delete(&Delete { skip_prompt: true }, window, cx)
    });
    assert!(!cx.has_pending_prompt(), "Should have received no prompts");
    cx.executor().run_until_parked();
}

fn ensure_no_open_items_and_panes(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) {
    assert!(
        !cx.has_pending_prompt(),
        "Should have no prompts after deletion operation closes the file"
    );
    workspace.update_in(cx, |workspace, _window, cx| {
        let open_project_paths = workspace
            .panes()
            .iter()
            .filter_map(|pane| pane.read(cx).active_item()?.project_path(cx))
            .collect::<Vec<_>>();
        assert!(
            open_project_paths.is_empty(),
            "Deleted file's buffer should be closed, but got open files: {open_project_paths:?}"
        );
    });
}

struct TestProjectItemView {
    focus_handle: FocusHandle,
    path: ProjectPath,
}

struct TestProjectItem {
    path: ProjectPath,
}

impl project::ProjectItem for TestProjectItem {
    fn try_open(
        _project: &Entity<Project>,
        path: &ProjectPath,
        cx: &mut App,
    ) -> Option<Task<anyhow::Result<Entity<Self>>>> {
        let path = path.clone();
        Some(cx.spawn(async move |cx| Ok(cx.new(|_| Self { path }))))
    }

    fn entry_id(&self, _: &App) -> Option<ProjectEntryId> {
        None
    }

    fn project_path(&self, _: &App) -> Option<ProjectPath> {
        Some(self.path.clone())
    }

    fn is_dirty(&self) -> bool {
        false
    }
}

impl ProjectItem for TestProjectItemView {
    type Item = TestProjectItem;

    fn for_project_item(
        _: Entity<Project>,
        _: Option<&Pane>,
        project_item: Entity<Self::Item>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self
    where
        Self: Sized,
    {
        Self {
            path: project_item.update(cx, |project_item, _| project_item.path.clone()),
            focus_handle: cx.focus_handle(),
        }
    }
}

impl Item for TestProjectItemView {
    type Event = ();

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        "Test".into()
    }
}

impl EventEmitter<()> for TestProjectItemView {}

impl Focusable for TestProjectItemView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TestProjectItemView {
    fn render(&mut self, _window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

#[gpui::test]
async fn test_delete_prompt_escapes_markdown_in_file_name(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "__somefile__": "",
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

    select_path(&panel, "root/__somefile__", cx);
    panel.update_in(cx, |panel, window, cx| {
        panel.delete(&Delete { skip_prompt: false }, window, cx)
    });
    let (message, _detail) = cx
        .pending_prompt()
        .expect("delete should show a confirmation prompt");

    assert_eq!(
        message,
        "Are you sure you want to permanently delete `__somefile__`?"
    );
}
