use super::*;

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
