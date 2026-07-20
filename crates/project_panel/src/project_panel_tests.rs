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
#[path = "tests/collapse_basic.rs"]
mod collapse_basic;
#[path = "tests/copy_paste_directories.rs"]
mod copy_paste_directories;
#[path = "tests/copy_paste_files.rs"]
mod copy_paste_files;
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
#[path = "tests/exclusions_auto_collapse.rs"]
mod exclusions_auto_collapse;
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
async fn test_autoreveal_and_gitignored_files(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);
    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.worktree.file_scan_exclusions = Some(Vec::new());
                settings
                    .project_panel
                    .get_or_insert_default()
                    .auto_reveal_entries = Some(false);
            });
        })
    });

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/project_root",
        json!({
            ".git": {},
            ".gitignore": "**/gitignored_dir",
            "dir_1": {
                "file_1.py": "# File 1_1 contents",
                "file_2.py": "# File 1_2 contents",
                "file_3.py": "# File 1_3 contents",
                "gitignored_dir": {
                    "file_a.py": "# File contents",
                    "file_b.py": "# File contents",
                    "file_c.py": "# File contents",
                },
            },
            "dir_2": {
                "file_1.py": "# File 2_1 contents",
                "file_2.py": "# File 2_2 contents",
                "file_3.py": "# File 2_3 contents",
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

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v project_root",
            "    > .git",
            "    > dir_1",
            "    > dir_2",
            "      .gitignore",
        ]
    );

    let dir_1_file = find_project_entry(&panel, "project_root/dir_1/file_1.py", cx)
        .expect("dir 1 file is not ignored and should have an entry");
    let dir_2_file = find_project_entry(&panel, "project_root/dir_2/file_1.py", cx)
        .expect("dir 2 file is not ignored and should have an entry");
    let gitignored_dir_file =
        find_project_entry(&panel, "project_root/dir_1/gitignored_dir/file_a.py", cx);
    assert_eq!(
        gitignored_dir_file, None,
        "File in the gitignored dir should not have an entry before its dir is toggled"
    );

    toggle_expand_dir(&panel, "project_root/dir_1", cx);
    toggle_expand_dir(&panel, "project_root/dir_1/gitignored_dir", cx);
    cx.executor().run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v project_root",
            "    > .git",
            "    v dir_1",
            "        v gitignored_dir  <== selected",
            "              file_a.py",
            "              file_b.py",
            "              file_c.py",
            "          file_1.py",
            "          file_2.py",
            "          file_3.py",
            "    > dir_2",
            "      .gitignore",
        ],
        "Should show gitignored dir file list in the project panel"
    );
    let gitignored_dir_file =
        find_project_entry(&panel, "project_root/dir_1/gitignored_dir/file_a.py", cx)
            .expect("after gitignored dir got opened, a file entry should be present");

    toggle_expand_dir(&panel, "project_root/dir_1/gitignored_dir", cx);
    toggle_expand_dir(&panel, "project_root/dir_1", cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v project_root",
            "    > .git",
            "    > dir_1  <== selected",
            "    > dir_2",
            "      .gitignore",
        ],
        "Should hide all dir contents again and prepare for the auto reveal test"
    );

    for file_entry in [dir_1_file, dir_2_file, gitignored_dir_file] {
        panel.update(cx, |panel, cx| {
            panel.project.update(cx, |_, cx| {
                cx.emit(project::Event::ActiveEntryChanged(Some(file_entry)))
            })
        });
        cx.run_until_parked();
        assert_eq!(
            visible_entries_as_strings(&panel, 0..20, cx),
            &[
                "v project_root",
                "    > .git",
                "    > dir_1  <== selected",
                "    > dir_2",
                "      .gitignore",
            ],
            "When no auto reveal is enabled, the selected entry should not be revealed in the project panel"
        );
    }

    cx.update(|_, cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings
                    .project_panel
                    .get_or_insert_default()
                    .auto_reveal_entries = Some(true)
            });
        })
    });

    panel.update(cx, |panel, cx| {
        panel.project.update(cx, |_, cx| {
            cx.emit(project::Event::ActiveEntryChanged(Some(dir_1_file)))
        })
    });
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v project_root",
            "    > .git",
            "    v dir_1",
            "        > gitignored_dir",
            "          file_1.py  <== selected  <== marked",
            "          file_2.py",
            "          file_3.py",
            "    > dir_2",
            "      .gitignore",
        ],
        "When auto reveal is enabled, not ignored dir_1 entry should be revealed"
    );

    panel.update(cx, |panel, cx| {
        panel.project.update(cx, |_, cx| {
            cx.emit(project::Event::ActiveEntryChanged(Some(dir_2_file)))
        })
    });
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v project_root",
            "    > .git",
            "    v dir_1",
            "        > gitignored_dir",
            "          file_1.py",
            "          file_2.py",
            "          file_3.py",
            "    v dir_2",
            "          file_1.py  <== selected  <== marked",
            "          file_2.py",
            "          file_3.py",
            "      .gitignore",
        ],
        "When auto reveal is enabled, not ignored dir_2 entry should be revealed"
    );

    panel.update(cx, |panel, cx| {
        panel.project.update(cx, |_, cx| {
            cx.emit(project::Event::ActiveEntryChanged(Some(
                gitignored_dir_file,
            )))
        })
    });
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v project_root",
            "    > .git",
            "    v dir_1",
            "        > gitignored_dir",
            "          file_1.py",
            "          file_2.py",
            "          file_3.py",
            "    v dir_2",
            "          file_1.py  <== selected  <== marked",
            "          file_2.py",
            "          file_3.py",
            "      .gitignore",
        ],
        "When auto reveal is enabled, a gitignored selected entry should not be revealed in the project panel"
    );

    panel.update(cx, |panel, cx| {
        panel.project.update(cx, |_, cx| {
            cx.emit(project::Event::RevealInProjectPanel(gitignored_dir_file))
        })
    });
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v project_root",
            "    > .git",
            "    v dir_1",
            "        v gitignored_dir",
            "              file_a.py  <== selected  <== marked",
            "              file_b.py",
            "              file_c.py",
            "          file_1.py",
            "          file_2.py",
            "          file_3.py",
            "    v dir_2",
            "          file_1.py",
            "          file_2.py",
            "          file_3.py",
            "      .gitignore",
        ],
        "When a gitignored entry is explicitly revealed, it should be shown in the project tree"
    );

    panel.update(cx, |panel, cx| {
        panel.project.update(cx, |_, cx| {
            cx.emit(project::Event::ActiveEntryChanged(Some(dir_2_file)))
        })
    });
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v project_root",
            "    > .git",
            "    v dir_1",
            "        v gitignored_dir",
            "              file_a.py",
            "              file_b.py",
            "              file_c.py",
            "          file_1.py",
            "          file_2.py",
            "          file_3.py",
            "    v dir_2",
            "          file_1.py  <== selected  <== marked",
            "          file_2.py",
            "          file_3.py",
            "      .gitignore",
        ],
        "After switching to dir_2_file, it should be selected and marked"
    );

    panel.update(cx, |panel, cx| {
        panel.project.update(cx, |_, cx| {
            cx.emit(project::Event::ActiveEntryChanged(Some(
                gitignored_dir_file,
            )))
        })
    });
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v project_root",
            "    > .git",
            "    v dir_1",
            "        v gitignored_dir",
            "              file_a.py  <== selected  <== marked",
            "              file_b.py",
            "              file_c.py",
            "          file_1.py",
            "          file_2.py",
            "          file_3.py",
            "    v dir_2",
            "          file_1.py",
            "          file_2.py",
            "          file_3.py",
            "      .gitignore",
        ],
        "When a gitignored entry is already visible, auto reveal should mark it as selected"
    );
}

#[gpui::test]
async fn test_gitignored_and_always_included(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);
    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.worktree.file_scan_exclusions = Some(Vec::new());
                settings.project.worktree.file_scan_inclusions =
                    Some(vec!["always_included_but_ignored_dir/*".to_string()]);
                settings
                    .project_panel
                    .get_or_insert_default()
                    .auto_reveal_entries = Some(false)
            });
        })
    });

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/project_root",
        json!({
            ".git": {},
            ".gitignore": "**/gitignored_dir\n/always_included_but_ignored_dir",
            "dir_1": {
                "file_1.py": "# File 1_1 contents",
                "file_2.py": "# File 1_2 contents",
                "file_3.py": "# File 1_3 contents",
                "gitignored_dir": {
                    "file_a.py": "# File contents",
                    "file_b.py": "# File contents",
                    "file_c.py": "# File contents",
                },
            },
            "dir_2": {
                "file_1.py": "# File 2_1 contents",
                "file_2.py": "# File 2_2 contents",
                "file_3.py": "# File 2_3 contents",
            },
            "always_included_but_ignored_dir": {
                "file_a.py": "# File contents",
                "file_b.py": "# File contents",
                "file_c.py": "# File contents",
            },
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

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v project_root",
            "    > .git",
            "    > always_included_but_ignored_dir",
            "    > dir_1",
            "    > dir_2",
            "      .gitignore",
        ]
    );

    let gitignored_dir_file =
        find_project_entry(&panel, "project_root/dir_1/gitignored_dir/file_a.py", cx);
    let always_included_but_ignored_dir_file = find_project_entry(
        &panel,
        "project_root/always_included_but_ignored_dir/file_a.py",
        cx,
    )
    .expect("file that is .gitignored but set to always be included should have an entry");
    assert_eq!(
        gitignored_dir_file, None,
        "File in the gitignored dir should not have an entry unless its directory is toggled"
    );

    toggle_expand_dir(&panel, "project_root/dir_1", cx);
    cx.run_until_parked();
    cx.update(|_, cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings
                    .project_panel
                    .get_or_insert_default()
                    .auto_reveal_entries = Some(true)
            });
        })
    });

    panel.update(cx, |panel, cx| {
        panel.project.update(cx, |_, cx| {
            cx.emit(project::Event::ActiveEntryChanged(Some(
                always_included_but_ignored_dir_file,
            )))
        })
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v project_root",
            "    > .git",
            "    v always_included_but_ignored_dir",
            "          file_a.py  <== selected  <== marked",
            "          file_b.py",
            "          file_c.py",
            "    v dir_1",
            "        > gitignored_dir",
            "          file_1.py",
            "          file_2.py",
            "          file_3.py",
            "    > dir_2",
            "      .gitignore",
        ],
        "When auto reveal is enabled, a gitignored but always included selected entry should be revealed in the project panel"
    );
}

#[gpui::test]
async fn test_explicit_reveal(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);
    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.worktree.file_scan_exclusions = Some(Vec::new());
                settings
                    .project_panel
                    .get_or_insert_default()
                    .auto_reveal_entries = Some(false)
            });
        })
    });

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/project_root",
        json!({
            ".git": {},
            ".gitignore": "**/gitignored_dir",
            "dir_1": {
                "file_1.py": "# File 1_1 contents",
                "file_2.py": "# File 1_2 contents",
                "file_3.py": "# File 1_3 contents",
                "gitignored_dir": {
                    "file_a.py": "# File contents",
                    "file_b.py": "# File contents",
                    "file_c.py": "# File contents",
                },
            },
            "dir_2": {
                "file_1.py": "# File 2_1 contents",
                "file_2.py": "# File 2_2 contents",
                "file_3.py": "# File 2_3 contents",
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

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v project_root",
            "    > .git",
            "    > dir_1",
            "    > dir_2",
            "      .gitignore",
        ]
    );

    let dir_1_file = find_project_entry(&panel, "project_root/dir_1/file_1.py", cx)
        .expect("dir 1 file is not ignored and should have an entry");
    let dir_2_file = find_project_entry(&panel, "project_root/dir_2/file_1.py", cx)
        .expect("dir 2 file is not ignored and should have an entry");
    let gitignored_dir_file =
        find_project_entry(&panel, "project_root/dir_1/gitignored_dir/file_a.py", cx);
    assert_eq!(
        gitignored_dir_file, None,
        "File in the gitignored dir should not have an entry before its dir is toggled"
    );

    toggle_expand_dir(&panel, "project_root/dir_1", cx);
    toggle_expand_dir(&panel, "project_root/dir_1/gitignored_dir", cx);
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v project_root",
            "    > .git",
            "    v dir_1",
            "        v gitignored_dir  <== selected",
            "              file_a.py",
            "              file_b.py",
            "              file_c.py",
            "          file_1.py",
            "          file_2.py",
            "          file_3.py",
            "    > dir_2",
            "      .gitignore",
        ],
        "Should show gitignored dir file list in the project panel"
    );
    let gitignored_dir_file =
        find_project_entry(&panel, "project_root/dir_1/gitignored_dir/file_a.py", cx)
            .expect("after gitignored dir got opened, a file entry should be present");

    toggle_expand_dir(&panel, "project_root/dir_1/gitignored_dir", cx);
    toggle_expand_dir(&panel, "project_root/dir_1", cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v project_root",
            "    > .git",
            "    > dir_1  <== selected",
            "    > dir_2",
            "      .gitignore",
        ],
        "Should hide all dir contents again and prepare for the explicit reveal test"
    );

    for file_entry in [dir_1_file, dir_2_file, gitignored_dir_file] {
        panel.update(cx, |panel, cx| {
            panel.project.update(cx, |_, cx| {
                cx.emit(project::Event::ActiveEntryChanged(Some(file_entry)))
            })
        });
        cx.run_until_parked();
        assert_eq!(
            visible_entries_as_strings(&panel, 0..20, cx),
            &[
                "v project_root",
                "    > .git",
                "    > dir_1  <== selected",
                "    > dir_2",
                "      .gitignore",
            ],
            "When no auto reveal is enabled, the selected entry should not be revealed in the project panel"
        );
    }

    panel.update(cx, |panel, cx| {
        panel.project.update(cx, |_, cx| {
            cx.emit(project::Event::RevealInProjectPanel(dir_1_file))
        })
    });
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v project_root",
            "    > .git",
            "    v dir_1",
            "        > gitignored_dir",
            "          file_1.py  <== selected  <== marked",
            "          file_2.py",
            "          file_3.py",
            "    > dir_2",
            "      .gitignore",
        ],
        "With no auto reveal, explicit reveal should show the dir_1 entry in the project panel"
    );

    panel.update(cx, |panel, cx| {
        panel.project.update(cx, |_, cx| {
            cx.emit(project::Event::RevealInProjectPanel(dir_2_file))
        })
    });
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v project_root",
            "    > .git",
            "    v dir_1",
            "        > gitignored_dir",
            "          file_1.py",
            "          file_2.py",
            "          file_3.py",
            "    v dir_2",
            "          file_1.py  <== selected  <== marked",
            "          file_2.py",
            "          file_3.py",
            "      .gitignore",
        ],
        "With no auto reveal, explicit reveal should show the dir_2 entry in the project panel"
    );

    panel.update(cx, |panel, cx| {
        panel.project.update(cx, |_, cx| {
            cx.emit(project::Event::RevealInProjectPanel(gitignored_dir_file))
        })
    });
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v project_root",
            "    > .git",
            "    v dir_1",
            "        v gitignored_dir",
            "              file_a.py  <== selected  <== marked",
            "              file_b.py",
            "              file_c.py",
            "          file_1.py",
            "          file_2.py",
            "          file_3.py",
            "    v dir_2",
            "          file_1.py",
            "          file_2.py",
            "          file_3.py",
            "      .gitignore",
        ],
        "With no auto reveal, explicit reveal should show the gitignored entry in the project panel"
    );
}

/// Mirrors real multi-buffer views (`ProjectDiagnosticsEditor`, `ProjectDiff`,
/// etc.): the workspace `Item` is a thin wrapper that holds an inner `Editor`
/// and re-emits its events.
mod multibuffer_wrapper {
    use editor::{Editor, EditorEvent};
    use gpui::{
        App, Context, Entity, EntityId, EventEmitter, FocusHandle, Focusable, IntoElement,
        ParentElement, Render, SharedString, Subscription, Window, div,
    };
    use workspace::item::{Item, ItemEvent, TabContentParams};

    pub struct TestMultibufferWrapper {
        pub editor: Entity<Editor>,
        _subscription: Subscription,
    }

    impl TestMultibufferWrapper {
        pub fn new(editor: Entity<Editor>, cx: &mut Context<Self>) -> Self {
            let _subscription = cx.subscribe(&editor, |_, _, event: &EditorEvent, cx| {
                cx.emit(event.clone());
            });
            Self {
                editor,
                _subscription,
            }
        }
    }

    impl EventEmitter<EditorEvent> for TestMultibufferWrapper {}

    impl Focusable for TestMultibufferWrapper {
        fn focus_handle(&self, cx: &App) -> FocusHandle {
            self.editor.read(cx).focus_handle(cx)
        }
    }

    impl Render for TestMultibufferWrapper {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().child(self.editor.clone())
        }
    }

    impl Item for TestMultibufferWrapper {
        type Event = EditorEvent;

        fn tab_content_text(&self, _: usize, _: &App) -> SharedString {
            "wrapper".into()
        }

        fn for_each_project_item(
            &self,
            cx: &App,
            f: &mut dyn FnMut(EntityId, &dyn project::ProjectItem),
        ) {
            self.editor.read(cx).for_each_project_item(cx, f)
        }

        fn active_project_path(&self, cx: &App) -> Option<project::ProjectPath> {
            self.editor.read(cx).active_project_path(cx)
        }

        fn to_item_events(event: &EditorEvent, f: &mut dyn FnMut(ItemEvent)) {
            Editor::to_item_events(event, f)
        }

        fn tab_content(&self, params: TabContentParams, _: &Window, cx: &App) -> gpui::AnyElement {
            ui::Label::new(self.tab_content_text(params.detail.unwrap_or_default(), cx))
                .into_any_element()
        }
    }
}

#[gpui::test]
async fn test_autoreveal_follows_multibuffer_selection(cx: &mut gpui::TestAppContext) {
    use editor::{
        Editor, EditorEvent, EditorMode, MultiBuffer, PathKey, SelectionEffects, ToOffset,
    };
    use language::Point;
    use multibuffer_wrapper::TestMultibufferWrapper;

    init_test_with_editor(cx);
    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings
                    .project_panel
                    .get_or_insert_default()
                    .auto_reveal_entries = Some(true);
            });
        });
    });

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/project_root"),
        json!({
            "dir_1": { "file_1.py": "alpha 1\nalpha 2\nalpha 3\n" },
            "dir_2": { "file_2.py": "beta 1\nbeta 2\nbeta 3\n" },
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/project_root").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    let buffer_1 = project
        .update(cx, |project, cx| {
            let project_path = project
                .find_project_path("project_root/dir_1/file_1.py", cx)
                .unwrap();
            project.open_buffer(project_path, cx)
        })
        .await
        .unwrap();
    let buffer_2 = project
        .update(cx, |project, cx| {
            let project_path = project
                .find_project_path("project_root/dir_2/file_2.py", cx)
                .unwrap();
            project.open_buffer(project_path, cx)
        })
        .await
        .unwrap();

    let multi_buffer = cx.update(|_, cx| {
        cx.new(|cx| {
            let mut multi_buffer = MultiBuffer::new(language::Capability::ReadWrite);
            multi_buffer.set_excerpts_for_path(
                PathKey::sorted(0),
                buffer_1.clone(),
                [Point::new(0, 0)..Point::new(2, 0)],
                0,
                cx,
            );
            multi_buffer.set_excerpts_for_path(
                PathKey::sorted(1),
                buffer_2.clone(),
                [Point::new(0, 0)..Point::new(2, 0)],
                0,
                cx,
            );
            multi_buffer
        })
    });

    let inner_editor = cx.update(|window, cx| {
        cx.new(|cx| {
            Editor::new(
                EditorMode::full(),
                multi_buffer.clone(),
                Some(project.clone()),
                window,
                cx,
            )
        })
    });

    // Wrap the multibuffer editor in an `Item`, mirroring real multibuffer
    // views (`ProjectDiagnosticsEditor`, `ProjectDiff`, etc.). Auto-reveal
    // should follow the inner editor's active buffer.
    workspace.update_in(cx, |workspace, window, cx| {
        let wrapper = cx.new(|cx| TestMultibufferWrapper::new(inner_editor.clone(), cx));
        workspace.add_item_to_active_pane(Box::new(wrapper), None, true, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v project_root",
            "    v dir_1",
            "          file_1.py  <== selected  <== marked",
            "    > dir_2",
        ],
        "When a multibuffer becomes active, its first excerpt's file should be revealed"
    );

    let buffer_2_offset = multi_buffer.read_with(cx, |multi_buffer, cx| {
        let snapshot = multi_buffer.snapshot(cx);
        let buffer_2_id = buffer_2.read(cx).remote_id();
        let excerpt = snapshot
            .excerpts_for_buffer(buffer_2_id)
            .next()
            .expect("buffer_2 excerpt must exist");
        snapshot
            .anchor_in_excerpt(excerpt.context.start)
            .expect("excerpt anchor must resolve")
            .to_offset(&snapshot)
    });

    inner_editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([buffer_2_offset..buffer_2_offset]);
        });
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v project_root",
            "    v dir_1",
            "          file_1.py",
            "    v dir_2",
            "          file_2.py  <== selected  <== marked",
        ],
        "Moving the cursor into a different excerpt buffer should reveal that buffer's entry"
    );

    // Wrappers re-emit inner-editor events through `to_item_events`, so a
    // benign `TitleChanged` (e.g. diagnostic summary updates) ultimately
    // reaches `Workspace::active_item_path_changed`. The active path should be
    // recomputed from the wrapper instead of falling back to a stale selection.
    inner_editor.update(cx, |_, cx| cx.emit(EditorEvent::TitleChanged));
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v project_root",
            "    v dir_1",
            "          file_1.py",
            "    v dir_2",
            "          file_2.py  <== selected  <== marked",
        ],
        "Wrapper-level title updates must not clobber the inner editor's reveal"
    );
}

#[gpui::test]
async fn test_reveal_in_project_panel_fallback(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/workspace",
        json!({
            "README.md": ""
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/workspace".as_ref()], cx).await;
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

    // Project panel should still be activated and focused, when using `pane:
    // reveal in project panel` without an active item.
    cx.dispatch_action(workspace::RevealInProjectPanel::default());
    cx.run_until_parked();

    panel.update_in(cx, |panel, window, cx| {
        panel
            .workspace
            .update(cx, |workspace, cx| {
                assert!(
                    workspace.active_item(cx).is_none(),
                    "Workspace should not have an active item."
                );
            })
            .unwrap();

        assert!(
            panel.focus_handle(cx).is_focused(window),
            "Project panel should be focused, even when there's no active item."
        );
    });

    // When working with a file that doesn't belong to an open project, we
    // should still activate the project panel on `pane: reveal in project
    // panel`.
    fs.insert_tree(
        "/external",
        json!({
            "file.txt": "External File",
        }),
    )
    .await;

    let (worktree, _) = project
        .update(cx, |project, cx| {
            project.find_or_create_worktree("/external/file.txt", false, cx)
        })
        .await
        .unwrap();

    workspace
        .update_in(cx, |workspace, window, cx| {
            let worktree_id = worktree.read(cx).id();
            let path = rel_path("").into();
            let project_path = ProjectPath { worktree_id, path };

            workspace.open_path(project_path, None, true, window, cx)
        })
        .await
        .unwrap();
    cx.run_until_parked();

    panel.update_in(cx, |panel, window, cx| {
        assert!(
            !panel.focus_handle(cx).is_focused(window),
            "Project panel should not be focused after opening an external file."
        );
    });

    cx.dispatch_action(workspace::RevealInProjectPanel::default());
    cx.run_until_parked();

    panel.update_in(cx, |panel, window, cx| {
        panel
            .workspace
            .update(cx, |workspace, cx| {
                assert!(
                    workspace.active_item(cx).is_some(),
                    "Workspace should have an active item."
                );
            })
            .unwrap();

        assert!(
            panel.focus_handle(cx).is_focused(window),
            "Project panel should be focused even for invisible worktree entry."
        );
    });

    // Focus again on the center pane so we're sure that the focus doesn't
    // remain on the project panel, otherwise later assertions wouldn't matter.
    panel.update_in(cx, |panel, window, cx| {
        panel
            .workspace
            .update(cx, |workspace, cx| {
                workspace.focus_center_pane(window, cx);
            })
            .log_err();

        assert!(
            !panel.focus_handle(cx).is_focused(window),
            "Project panel should not be focused after focusing on center pane."
        );
    });

    panel.update_in(cx, |panel, window, cx| {
        assert!(
            !panel.focus_handle(cx).is_focused(window),
            "Project panel should not be focused after focusing the center pane."
        );
    });

    // Create an unsaved buffer and verify that pane: reveal in project panel`
    // still activates and focuses the panel.
    let pane = workspace.update(cx, |workspace, _| workspace.active_pane().clone());
    pane.update_in(cx, |pane, window, cx| {
        let item = cx.new(|cx| TestItem::new(cx).with_label("Unsaved buffer"));
        pane.add_item(Box::new(item), false, false, None, window, cx);
    });

    cx.dispatch_action(workspace::RevealInProjectPanel::default());
    cx.run_until_parked();

    panel.update_in(cx, |panel, window, cx| {
        panel
            .workspace
            .update(cx, |workspace, cx| {
                assert!(
                    workspace.active_item(cx).is_some(),
                    "Workspace should have an active item."
                );
            })
            .unwrap();

        assert!(
            panel.focus_handle(cx).is_focused(window),
            "Project panel should be focused even for an unsaved buffer."
        );
    });
}

#[gpui::test]
async fn test_creating_excluded_entries(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.worktree.file_scan_exclusions =
                    Some(vec!["excluded_dir".to_string(), "**/.git".to_string()]);
            });
        });
    });

    cx.update(|cx| {
        register_project_item::<TestProjectItemView>(cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root1",
        json!({
            ".dockerignore": "",
            ".git": {
                "HEAD": "",
            },
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
    cx.run_until_parked();

    select_path(&panel, "root1", cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &["v root1  <== selected", "      .dockerignore",]
    );
    workspace.update_in(cx, |workspace, _, cx| {
        assert!(
            workspace.active_item(cx).is_none(),
            "Should have no active items in the beginning"
        );
    });

    let excluded_file_path = ".git/COMMIT_EDITMSG";
    let excluded_dir_path = "excluded_dir";

    panel.update_in(cx, |panel, window, cx| panel.new_file(&NewFile, window, cx));
    cx.run_until_parked();
    panel.update_in(cx, |panel, window, cx| {
        assert!(panel.filename_editor.read(cx).is_focused(window));
    });
    panel
        .update_in(cx, |panel, window, cx| {
            panel.filename_editor.update(cx, |editor, cx| {
                editor.set_text(excluded_file_path, window, cx)
            });
            panel.confirm_edit(true, window, cx).unwrap()
        })
        .await
        .unwrap();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..13, cx),
        &["v root1", "      .dockerignore"],
        "Excluded dir should not be shown after opening a file in it"
    );
    panel.update_in(cx, |panel, window, cx| {
        assert!(
            !panel.filename_editor.read(cx).is_focused(window),
            "Should have closed the file name editor"
        );
    });
    workspace.update_in(cx, |workspace, _, cx| {
        let active_entry_path = workspace
            .active_item(cx)
            .expect("should have opened and activated the excluded item")
            .act_as::<TestProjectItemView>(cx)
            .expect("should have opened the corresponding project item for the excluded item")
            .read(cx)
            .path
            .clone();
        assert_eq!(
            active_entry_path.path.as_ref(),
            rel_path(excluded_file_path),
            "Should open the excluded file"
        );

        assert!(
            workspace.notification_ids().is_empty(),
            "Should have no notifications after opening an excluded file"
        );
    });
    assert!(
        fs.is_file(Path::new("/root1/.git/COMMIT_EDITMSG")).await,
        "Should have created the excluded file"
    );

    select_path(&panel, "root1", cx);
    panel.update_in(cx, |panel, window, cx| {
        panel.new_directory(&NewDirectory, window, cx)
    });
    cx.run_until_parked();
    panel.update_in(cx, |panel, window, cx| {
        assert!(panel.filename_editor.read(cx).is_focused(window));
    });
    panel
        .update_in(cx, |panel, window, cx| {
            panel.filename_editor.update(cx, |editor, cx| {
                editor.set_text(excluded_file_path, window, cx)
            });
            panel.confirm_edit(true, window, cx).unwrap()
        })
        .await
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..13, cx),
        &["v root1", "      .dockerignore"],
        "Should not change the project panel after trying to create an excluded directorya directory with the same name as the excluded file"
    );
    panel.update_in(cx, |panel, window, cx| {
        assert!(
            !panel.filename_editor.read(cx).is_focused(window),
            "Should have closed the file name editor"
        );
    });
    workspace.update_in(cx, |workspace, _, cx| {
        let notifications = workspace.notification_ids();
        assert_eq!(
            notifications.len(),
            1,
            "Should receive one notification with the error message"
        );
        workspace.dismiss_notification(notifications.first().unwrap(), cx);
        assert!(workspace.notification_ids().is_empty());
    });

    select_path(&panel, "root1", cx);
    panel.update_in(cx, |panel, window, cx| {
        panel.new_directory(&NewDirectory, window, cx)
    });
    cx.run_until_parked();

    panel.update_in(cx, |panel, window, cx| {
        assert!(panel.filename_editor.read(cx).is_focused(window));
    });

    panel
        .update_in(cx, |panel, window, cx| {
            panel.filename_editor.update(cx, |editor, cx| {
                editor.set_text(excluded_dir_path, window, cx)
            });
            panel.confirm_edit(true, window, cx).unwrap()
        })
        .await
        .unwrap();

    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..13, cx),
        &["v root1", "      .dockerignore"],
        "Should not change the project panel after trying to create an excluded directory"
    );
    panel.update_in(cx, |panel, window, cx| {
        assert!(
            !panel.filename_editor.read(cx).is_focused(window),
            "Should have closed the file name editor"
        );
    });
    workspace.update_in(cx, |workspace, _, cx| {
        let notifications = workspace.notification_ids();
        assert_eq!(
            notifications.len(),
            1,
            "Should receive one notification explaining that no directory is actually shown"
        );
        workspace.dismiss_notification(notifications.first().unwrap(), cx);
        assert!(workspace.notification_ids().is_empty());
    });
    assert!(
        fs.is_dir(Path::new("/root1/excluded_dir")).await,
        "Should have created the excluded directory"
    );
}

#[gpui::test]
async fn test_selection_restored_when_creation_cancelled(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/src",
        json!({
            "test": {
                "first.rs": "// First Rust file",
                "second.rs": "// Second Rust file",
                "third.rs": "// Third Rust file",
            }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/src".as_ref()], cx).await;
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

    select_path(&panel, "src", cx);
    panel.update_in(cx, |panel, window, cx| panel.confirm(&Confirm, window, cx));
    cx.executor().run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            //
            "v src  <== selected",
            "    > test"
        ]
    );
    panel.update_in(cx, |panel, window, cx| {
        panel.new_directory(&NewDirectory, window, cx)
    });
    cx.executor().run_until_parked();
    panel.update_in(cx, |panel, window, cx| {
        assert!(panel.filename_editor.read(cx).is_focused(window));
    });
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            //
            "v src",
            "    > [EDITOR: '']  <== selected",
            "    > test"
        ]
    );

    panel.update_in(cx, |panel, window, cx| {
        panel.cancel(&menu::Cancel, window, cx);
    });
    cx.executor().run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            //
            "v src  <== selected",
            "    > test"
        ]
    );

    panel.update_in(cx, |panel, window, cx| {
        panel.new_directory(&NewDirectory, window, cx)
    });
    cx.executor().run_until_parked();
    panel.update_in(cx, |panel, window, cx| {
        assert!(panel.filename_editor.read(cx).is_focused(window));
    });
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            //
            "v src",
            "    > [EDITOR: '']  <== selected",
            "    > test"
        ]
    );
    workspace.update_in(cx, |_, window, _| window.blur());
    cx.executor().run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            //
            "v src  <== selected",
            "    > test"
        ]
    );
}

#[gpui::test]
async fn test_basic_file_deletion_scenarios(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "dir1": {
                "subdir1": {},
                "file1.txt": "",
                "file2.txt": "",
            },
            "dir2": {
                "subdir2": {},
                "file3.txt": "",
                "file4.txt": "",
            },
            "file5.txt": "",
            "file6.txt": "",
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

    // Test Case 1: Delete middle file in directory
    select_path(&panel, "root/dir1/file1.txt", cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..15, cx),
        &[
            "v root",
            "    v dir1",
            "        > subdir1",
            "          file1.txt  <== selected",
            "          file2.txt",
            "    v dir2",
            "        > subdir2",
            "          file3.txt",
            "          file4.txt",
            "      file5.txt",
            "      file6.txt",
        ],
        "Initial state before deleting middle file"
    );

    submit_deletion(&panel, cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..15, cx),
        &[
            "v root",
            "    v dir1",
            "        > subdir1",
            "          file2.txt  <== selected",
            "    v dir2",
            "        > subdir2",
            "          file3.txt",
            "          file4.txt",
            "      file5.txt",
            "      file6.txt",
        ],
        "Should select next file after deleting middle file"
    );

    // Test Case 2: Delete last file in directory
    submit_deletion(&panel, cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..15, cx),
        &[
            "v root",
            "    v dir1",
            "        > subdir1  <== selected",
            "    v dir2",
            "        > subdir2",
            "          file3.txt",
            "          file4.txt",
            "      file5.txt",
            "      file6.txt",
        ],
        "Should select next directory when last file is deleted"
    );

    // Test Case 3: Delete root level file
    select_path(&panel, "root/file6.txt", cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..15, cx),
        &[
            "v root",
            "    v dir1",
            "        > subdir1",
            "    v dir2",
            "        > subdir2",
            "          file3.txt",
            "          file4.txt",
            "      file5.txt",
            "      file6.txt  <== selected",
        ],
        "Initial state before deleting root level file"
    );

    submit_deletion(&panel, cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..15, cx),
        &[
            "v root",
            "    v dir1",
            "        > subdir1",
            "    v dir2",
            "        > subdir2",
            "          file3.txt",
            "          file4.txt",
            "      file5.txt  <== selected",
        ],
        "Should select prev entry at root level"
    );
}

#[gpui::test]
async fn test_deletion_gitignored(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "aa": "// Testing 1",
            "bb": "// Testing 2",
            "cc": "// Testing 3",
            "dd": "// Testing 4",
            "ee": "// Testing 5",
            "ff": "// Testing 6",
            "gg": "// Testing 7",
            "hh": "// Testing 8",
            "ii": "// Testing 8",
            ".gitignore": "bb\ndd\nee\nff\nii\n'",
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    // Test 1: Auto selection with one gitignored file next to the deleted file
    cx.update(|_, cx| {
        let settings = *ProjectPanelSettings::get_global(cx);
        ProjectPanelSettings::override_global(
            ProjectPanelSettings {
                hide_gitignore: true,
                ..settings
            },
            cx,
        );
    });

    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    select_path(&panel, "root/aa", cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root",
            "      .gitignore",
            "      aa  <== selected",
            "      cc",
            "      gg",
            "      hh"
        ],
        "Initial state should hide files on .gitignore"
    );

    submit_deletion(&panel, cx);

    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root",
            "      .gitignore",
            "      cc  <== selected",
            "      gg",
            "      hh"
        ],
        "Should select next entry not on .gitignore"
    );

    // Test 2: Auto selection with many gitignored files next to the deleted file
    submit_deletion(&panel, cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root",
            "      .gitignore",
            "      gg  <== selected",
            "      hh"
        ],
        "Should select next entry not on .gitignore"
    );

    // Test 3: Auto selection of entry before deleted file
    select_path(&panel, "root/hh", cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root",
            "      .gitignore",
            "      gg",
            "      hh  <== selected"
        ],
        "Should select next entry not on .gitignore"
    );
    submit_deletion(&panel, cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &["v root", "      .gitignore", "      gg  <== selected"],
        "Should select next entry not on .gitignore"
    );
}

#[gpui::test]
async fn test_nested_deletion_gitignore(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "dir1": {
                "file1": "// Testing",
                "file2": "// Testing",
                "file3": "// Testing"
            },
            "aa": "// Testing",
            ".gitignore": "file1\nfile3\n",
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
                hide_gitignore: true,
                ..settings
            },
            cx,
        );
    });

    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    // Test 1: Visible items should exclude files on gitignore
    toggle_expand_dir(&panel, "root/dir1", cx);
    select_path(&panel, "root/dir1/file2", cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root",
            "    v dir1",
            "          file2  <== selected",
            "      .gitignore",
            "      aa"
        ],
        "Initial state should hide files on .gitignore"
    );
    submit_deletion(&panel, cx);

    // Test 2: Auto selection should go to the parent
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root",
            "    v dir1  <== selected",
            "      .gitignore",
            "      aa"
        ],
        "Initial state should hide files on .gitignore"
    );
}

#[gpui::test]
async fn test_complex_selection_scenarios(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "dir1": {
                "subdir1": {
                    "a.txt": "",
                    "b.txt": ""
                },
                "file1.txt": "",
            },
            "dir2": {
                "subdir2": {
                    "c.txt": "",
                    "d.txt": ""
                },
                "file2.txt": "",
            },
            "file3.txt": "",
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
    toggle_expand_dir(&panel, "root/dir1/subdir1", cx);
    toggle_expand_dir(&panel, "root/dir2", cx);
    toggle_expand_dir(&panel, "root/dir2/subdir2", cx);

    // Test Case 1: Select and delete nested directory with parent
    cx.simulate_modifiers_change(gpui::Modifiers {
        control: true,
        ..Default::default()
    });
    select_path_with_mark(&panel, "root/dir1/subdir1", cx);
    select_path_with_mark(&panel, "root/dir1", cx);

    assert_eq!(
        visible_entries_as_strings(&panel, 0..15, cx),
        &[
            "v root",
            "    v dir1  <== selected  <== marked",
            "        v subdir1  <== marked",
            "              a.txt",
            "              b.txt",
            "          file1.txt",
            "    v dir2",
            "        v subdir2",
            "              c.txt",
            "              d.txt",
            "          file2.txt",
            "      file3.txt",
        ],
        "Initial state before deleting nested directory with parent"
    );

    submit_deletion(&panel, cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..15, cx),
        &[
            "v root",
            "    v dir2  <== selected",
            "        v subdir2",
            "              c.txt",
            "              d.txt",
            "          file2.txt",
            "      file3.txt",
        ],
        "Should select next directory after deleting directory with parent"
    );

    // Test Case 2: Select mixed files and directories across levels
    select_path_with_mark(&panel, "root/dir2/subdir2/c.txt", cx);
    select_path_with_mark(&panel, "root/dir2/file2.txt", cx);
    select_path_with_mark(&panel, "root/file3.txt", cx);

    assert_eq!(
        visible_entries_as_strings(&panel, 0..15, cx),
        &[
            "v root",
            "    v dir2",
            "        v subdir2",
            "              c.txt  <== marked",
            "              d.txt",
            "          file2.txt  <== marked",
            "      file3.txt  <== selected  <== marked",
        ],
        "Initial state before deleting"
    );

    submit_deletion(&panel, cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..15, cx),
        &[
            "v root",
            "    v dir2  <== selected",
            "        v subdir2",
            "              d.txt",
        ],
        "Should select sibling directory"
    );
}

#[gpui::test]
async fn test_delete_all_files_and_directories(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "dir1": {
                "subdir1": {
                    "a.txt": "",
                    "b.txt": ""
                },
                "file1.txt": "",
            },
            "dir2": {
                "subdir2": {
                    "c.txt": "",
                    "d.txt": ""
                },
                "file2.txt": "",
            },
            "file3.txt": "",
            "file4.txt": "",
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
    toggle_expand_dir(&panel, "root/dir1/subdir1", cx);
    toggle_expand_dir(&panel, "root/dir2", cx);
    toggle_expand_dir(&panel, "root/dir2/subdir2", cx);

    // Test Case 1: Select all root files and directories
    cx.simulate_modifiers_change(gpui::Modifiers {
        control: true,
        ..Default::default()
    });
    select_path_with_mark(&panel, "root/dir1", cx);
    select_path_with_mark(&panel, "root/dir2", cx);
    select_path_with_mark(&panel, "root/file3.txt", cx);
    select_path_with_mark(&panel, "root/file4.txt", cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root",
            "    v dir1  <== marked",
            "        v subdir1",
            "              a.txt",
            "              b.txt",
            "          file1.txt",
            "    v dir2  <== marked",
            "        v subdir2",
            "              c.txt",
            "              d.txt",
            "          file2.txt",
            "      file3.txt  <== marked",
            "      file4.txt  <== selected  <== marked",
        ],
        "State before deleting all contents"
    );

    submit_deletion(&panel, cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &["v root  <== selected"],
        "Only empty root directory should remain after deleting all contents"
    );
}

#[gpui::test]
async fn test_nested_selection_deletion(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "dir1": {
                "subdir1": {
                    "file_a.txt": "content a",
                    "file_b.txt": "content b",
                },
                "subdir2": {
                    "file_c.txt": "content c",
                },
                "file1.txt": "content 1",
            },
            "dir2": {
                "file2.txt": "content 2",
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
    toggle_expand_dir(&panel, "root/dir1/subdir1", cx);
    toggle_expand_dir(&panel, "root/dir2", cx);
    cx.simulate_modifiers_change(gpui::Modifiers {
        control: true,
        ..Default::default()
    });

    // Test Case 1: Select parent directory, subdirectory, and a file inside the subdirectory
    select_path_with_mark(&panel, "root/dir1", cx);
    select_path_with_mark(&panel, "root/dir1/subdir1", cx);
    select_path_with_mark(&panel, "root/dir1/subdir1/file_a.txt", cx);

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root",
            "    v dir1  <== marked",
            "        v subdir1  <== marked",
            "              file_a.txt  <== selected  <== marked",
            "              file_b.txt",
            "        > subdir2",
            "          file1.txt",
            "    v dir2",
            "          file2.txt",
        ],
        "State with parent dir, subdir, and file selected"
    );
    submit_deletion(&panel, cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &["v root", "    v dir2  <== selected", "          file2.txt",],
        "Only dir2 should remain after deletion"
    );
}

#[gpui::test]
async fn test_multiple_worktrees_deletion(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    // First worktree
    fs.insert_tree(
        "/root1",
        json!({
            "dir1": {
                "file1.txt": "content 1",
                "file2.txt": "content 2",
            },
            "dir2": {
                "file3.txt": "content 3",
            },
        }),
    )
    .await;

    // Second worktree
    fs.insert_tree(
        "/root2",
        json!({
            "dir3": {
                "file4.txt": "content 4",
                "file5.txt": "content 5",
            },
            "file6.txt": "content 6",
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

    // Expand all directories for testing
    toggle_expand_dir(&panel, "root1/dir1", cx);
    toggle_expand_dir(&panel, "root1/dir2", cx);
    toggle_expand_dir(&panel, "root2/dir3", cx);

    // Test Case 1: Delete files across different worktrees
    cx.simulate_modifiers_change(gpui::Modifiers {
        control: true,
        ..Default::default()
    });
    select_path_with_mark(&panel, "root1/dir1/file1.txt", cx);
    select_path_with_mark(&panel, "root2/dir3/file4.txt", cx);

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root1",
            "    v dir1",
            "          file1.txt  <== marked",
            "          file2.txt",
            "    v dir2",
            "          file3.txt",
            "v root2",
            "    v dir3",
            "          file4.txt  <== selected  <== marked",
            "          file5.txt",
            "      file6.txt",
        ],
        "Initial state with files selected from different worktrees"
    );

    submit_deletion(&panel, cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root1",
            "    v dir1",
            "          file2.txt",
            "    v dir2",
            "          file3.txt",
            "v root2",
            "    v dir3",
            "          file5.txt  <== selected",
            "      file6.txt",
        ],
        "Should select next file in the last worktree after deletion"
    );

    // Test Case 2: Delete directories from different worktrees
    select_path_with_mark(&panel, "root1/dir1", cx);
    select_path_with_mark(&panel, "root2/dir3", cx);

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root1",
            "    v dir1  <== marked",
            "          file2.txt",
            "    v dir2",
            "          file3.txt",
            "v root2",
            "    v dir3  <== selected  <== marked",
            "          file5.txt",
            "      file6.txt",
        ],
        "State with directories marked from different worktrees"
    );

    submit_deletion(&panel, cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root1",
            "    v dir2",
            "          file3.txt",
            "v root2",
            "      file6.txt  <== selected",
        ],
        "Should select remaining file in last worktree after directory deletion"
    );

    // Test Case 4: Delete all remaining files except roots
    select_path_with_mark(&panel, "root1/dir2/file3.txt", cx);
    select_path_with_mark(&panel, "root2/file6.txt", cx);

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root1",
            "    v dir2",
            "          file3.txt  <== marked",
            "v root2",
            "      file6.txt  <== selected  <== marked",
        ],
        "State with all remaining files marked"
    );

    submit_deletion(&panel, cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &["v root1", "    v dir2", "v root2  <== selected"],
        "Second parent root should be selected after deleting"
    );
}

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
