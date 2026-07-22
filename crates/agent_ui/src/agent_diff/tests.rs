use super::*;
use crate::Keep;
use acp_thread::AgentConnection as _;
use agent_settings::AgentSettings;
use editor::EditorSettings;
use gpui::{TestAppContext, UpdateGlobal, VisualTestContext};
use project::{FakeFs, Project};
use serde_json::json;
use settings::{DiffViewStyle, SettingsStore};
use std::{path::Path, rc::Rc};
use util::path;
use workspace::{MultiWorkspace, PathList};

use workspace::{MultiWorkspace, PathList};

#[gpui::test]
async fn test_multibuffer_agent_diff(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.editor.diff_view_style = Some(DiffViewStyle::Unified);
            });
        });
        prompt_store::init(cx);
        theme_settings::init(theme::LoadThemes::JustBase, cx);
        language_model::init(cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/test"),
        json!({"file1": "abc\ndef\nghi\njkl\nmno\npqr\nstu\nvwx\nyz"}),
    )
    .await;
    let project = Project::test(fs, [path!("/test").as_ref()], cx).await;
    let buffer_path = project
        .read_with(cx, |project, cx| {
            project.find_project_path("test/file1", cx)
        })
        .unwrap();

    let connection = Rc::new(acp_thread::StubAgentConnection::new());
    let thread = cx
        .update(|cx| {
            connection.clone().new_session(
                project.clone(),
                PathList::new(&[Path::new(path!("/test"))]),
                cx,
            )
        })
        .await
        .unwrap();

    let action_log = cx.read(|cx| thread.read(cx).action_log().clone());

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    let agent_diff = cx.new_window_entity(|window, cx| {
        AgentDiffPane::new(thread.clone(), workspace.downgrade(), window, cx)
    });
    let editor = agent_diff.read_with(cx, |diff, cx| diff.editor.read(cx).rhs_editor().clone());

    let buffer = project
        .update(cx, |project, cx| project.open_buffer(buffer_path, cx))
        .await
        .unwrap();
    cx.update(|_, cx| {
        action_log.update(cx, |log, cx| log.buffer_read(buffer.clone(), cx));
        buffer.update(cx, |buffer, cx| {
            buffer
                .edit(
                    [
                        (Point::new(1, 1)..Point::new(1, 2), "E"),
                        (Point::new(3, 2)..Point::new(3, 3), "L"),
                        (Point::new(5, 0)..Point::new(5, 1), "P"),
                        (Point::new(7, 1)..Point::new(7, 2), "W"),
                    ],
                    None,
                    cx,
                )
                .unwrap()
        });
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    cx.run_until_parked();

    // When opening the assistant diff, the cursor is positioned on the first hunk.
    assert_eq!(
        editor.read_with(cx, |editor, cx| editor.text(cx)),
        "abc\ndef\ndEf\nghi\njkl\njkL\nmno\npqr\nPqr\nstu\nvwx\nvWx\nyz"
    );
    assert_eq!(
        editor
            .update(cx, |editor, cx| editor
                .selections
                .newest::<Point>(&editor.display_snapshot(cx)))
            .range(),
        Point::new(1, 0)..Point::new(1, 0)
    );

    // After keeping a hunk, the cursor should be positioned on the second hunk.
    agent_diff.update_in(cx, |diff, window, cx| diff.keep(&Keep, window, cx));
    cx.run_until_parked();
    assert_eq!(
        editor.read_with(cx, |editor, cx| editor.text(cx)),
        "abc\ndEf\nghi\njkl\njkL\nmno\npqr\nPqr\nstu\nvwx\nvWx\nyz"
    );
    assert_eq!(
        editor
            .update(cx, |editor, cx| editor
                .selections
                .newest::<Point>(&editor.display_snapshot(cx)))
            .range(),
        Point::new(3, 0)..Point::new(3, 0)
    );

    // Rejecting a hunk also moves the cursor to the next hunk, possibly cycling if it's at the end.
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
            selections.select_ranges([Point::new(10, 0)..Point::new(10, 0)])
        });
    });
    agent_diff.update_in(cx, |diff, window, cx| {
        diff.reject(&crate::Reject, window, cx)
    });
    cx.run_until_parked();
    assert_eq!(
        editor.read_with(cx, |editor, cx| editor.text(cx)),
        "abc\ndEf\nghi\njkl\njkL\nmno\npqr\nPqr\nstu\nvwx\nyz"
    );
    assert_eq!(
        editor
            .update(cx, |editor, cx| editor
                .selections
                .newest::<Point>(&editor.display_snapshot(cx)))
            .range(),
        Point::new(3, 0)..Point::new(3, 0)
    );

    // Keeping a range that doesn't intersect the current selection doesn't move it.
    agent_diff.update_in(cx, |_diff, window, cx| {
        let position = editor
            .read(cx)
            .buffer()
            .read(cx)
            .read(cx)
            .anchor_before(Point::new(7, 0));
        editor.update(cx, |editor, cx| {
            let snapshot = editor.buffer().read(cx).snapshot(cx);
            keep_edits_in_ranges(
                editor,
                &snapshot,
                &thread,
                vec![position..position],
                window,
                cx,
            )
        });
    });
    cx.run_until_parked();
    assert_eq!(
        editor.read_with(cx, |editor, cx| editor.text(cx)),
        "abc\ndEf\nghi\njkl\njkL\nmno\nPqr\nstu\nvwx\nyz"
    );
    assert_eq!(
        editor
            .update(cx, |editor, cx| editor
                .selections
                .newest::<Point>(&editor.display_snapshot(cx)))
            .range(),
        Point::new(3, 0)..Point::new(3, 0)
    );
}

#[gpui::test]
async fn test_single_file_review_diff(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
        prompt_store::init(cx);
        theme_settings::init(theme::LoadThemes::JustBase, cx);
        language_model::init(cx);
        workspace::register_project_item::<Editor>(cx);
    });

    cx.update(|cx| {
        SettingsStore::update_global(cx, |store, _cx| {
            let mut agent_settings = store.get::<AgentSettings>(None).clone();
            agent_settings.single_file_review = true;
            store.override_global(agent_settings);
        });
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/test"),
        json!({"file1": "abc\ndef\nghi\njkl\nmno\npqr\nstu\nvwx\nyz"}),
    )
    .await;
    fs.insert_tree(path!("/test"), json!({"file2": "abc\ndef\nghi"}))
        .await;

    let project = Project::test(fs, [path!("/test").as_ref()], cx).await;
    let buffer_path1 = project
        .read_with(cx, |project, cx| {
            project.find_project_path("test/file1", cx)
        })
        .unwrap();
    let buffer_path2 = project
        .read_with(cx, |project, cx| {
            project.find_project_path("test/file2", cx)
        })
        .unwrap();

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    // Add the diff toolbar to the active pane
    let diff_toolbar = cx.new_window_entity(|_, cx| AgentDiffToolbar::new(cx));

    workspace.update_in(cx, {
        let diff_toolbar = diff_toolbar.clone();

        move |workspace, window, cx| {
            workspace.active_pane().update(cx, |pane, cx| {
                pane.toolbar().update(cx, |toolbar, cx| {
                    toolbar.add_item(diff_toolbar, window, cx);
                });
            })
        }
    });

    let connection = Rc::new(acp_thread::StubAgentConnection::new());
    let thread = cx
        .update(|_, cx| {
            connection.clone().new_session(
                project.clone(),
                PathList::new(&[Path::new(path!("/test"))]),
                cx,
            )
        })
        .await
        .unwrap();
    let action_log = thread.read_with(cx, |thread, _| thread.action_log().clone());

    // Set the active thread
    cx.update(|window, cx| {
        AgentDiff::set_active_thread(&workspace.downgrade(), thread.clone(), window, cx)
    });

    let buffer1 = project
        .update(cx, |project, cx| {
            project.open_buffer(buffer_path1.clone(), cx)
        })
        .await
        .unwrap();
    let buffer2 = project
        .update(cx, |project, cx| {
            project.open_buffer(buffer_path2.clone(), cx)
        })
        .await
        .unwrap();

    // Open an editor for buffer1
    let editor1 = cx.new_window_entity(|window, cx| {
        Editor::for_buffer(buffer1.clone(), Some(project.clone()), window, cx)
    });

    workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item_to_active_pane(Box::new(editor1.clone()), None, true, window, cx);
    });
    cx.run_until_parked();

    // Toolbar knows about the current editor, but it's hidden since there are no changes yet
    assert!(diff_toolbar.read_with(cx, |toolbar, _cx| matches!(
        toolbar.active_item,
        Some(AgentDiffToolbarItem::Editor {
            state: EditorState::Idle,
            ..
        })
    )));
    assert_eq!(
        diff_toolbar.read_with(cx, |toolbar, cx| toolbar.location(cx)),
        ToolbarItemLocation::Hidden
    );

    // Make changes
    cx.update(|_, cx| {
        action_log.update(cx, |log, cx| log.buffer_read(buffer1.clone(), cx));
        buffer1.update(cx, |buffer, cx| {
            buffer
                .edit(
                    [
                        (Point::new(1, 1)..Point::new(1, 2), "E"),
                        (Point::new(3, 2)..Point::new(3, 3), "L"),
                        (Point::new(5, 0)..Point::new(5, 1), "P"),
                        (Point::new(7, 1)..Point::new(7, 2), "W"),
                    ],
                    None,
                    cx,
                )
                .unwrap()
        });
        action_log.update(cx, |log, cx| log.buffer_edited(buffer1.clone(), cx));

        action_log.update(cx, |log, cx| log.buffer_read(buffer2.clone(), cx));
        buffer2.update(cx, |buffer, cx| {
            buffer
                .edit(
                    [
                        (Point::new(0, 0)..Point::new(0, 1), "A"),
                        (Point::new(2, 1)..Point::new(2, 2), "H"),
                    ],
                    None,
                    cx,
                )
                .unwrap();
        });
        action_log.update(cx, |log, cx| log.buffer_edited(buffer2.clone(), cx));
    });
    cx.run_until_parked();

    // The already opened editor displays the diff and the cursor is at the first hunk
    assert_eq!(
        editor1.read_with(cx, |editor, cx| editor.text(cx)),
        "abc\ndef\ndEf\nghi\njkl\njkL\nmno\npqr\nPqr\nstu\nvwx\nvWx\nyz"
    );
    assert_eq!(
        editor1
            .update(cx, |editor, cx| editor
                .selections
                .newest::<Point>(&editor.display_snapshot(cx)))
            .range(),
        Point::new(1, 0)..Point::new(1, 0)
    );

    // The toolbar is displayed in the right state
    assert_eq!(
        diff_toolbar.read_with(cx, |toolbar, cx| toolbar.location(cx)),
        ToolbarItemLocation::PrimaryRight
    );
    assert!(diff_toolbar.read_with(cx, |toolbar, _cx| matches!(
        toolbar.active_item,
        Some(AgentDiffToolbarItem::Editor {
            state: EditorState::Reviewing,
            ..
        })
    )));

    // The toolbar respects its setting
    override_toolbar_agent_review_setting(false, cx);
    assert_eq!(
        diff_toolbar.read_with(cx, |toolbar, cx| toolbar.location(cx)),
        ToolbarItemLocation::Hidden
    );
    override_toolbar_agent_review_setting(true, cx);
    assert_eq!(
        diff_toolbar.read_with(cx, |toolbar, cx| toolbar.location(cx)),
        ToolbarItemLocation::PrimaryRight
    );

    // After keeping a hunk, the cursor should be positioned on the second hunk.
    workspace.update(cx, |_, cx| {
        cx.dispatch_action(&Keep);
    });
    cx.run_until_parked();
    assert_eq!(
        editor1.read_with(cx, |editor, cx| editor.text(cx)),
        "abc\ndEf\nghi\njkl\njkL\nmno\npqr\nPqr\nstu\nvwx\nvWx\nyz"
    );
    assert_eq!(
        editor1
            .update(cx, |editor, cx| editor
                .selections
                .newest::<Point>(&editor.display_snapshot(cx)))
            .range(),
        Point::new(3, 0)..Point::new(3, 0)
    );

    // Rejecting a hunk also moves the cursor to the next hunk, possibly cycling if it's at the end.
    editor1.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
            selections.select_ranges([Point::new(10, 0)..Point::new(10, 0)])
        });
    });
    workspace.update(cx, |_, cx| {
        cx.dispatch_action(&Reject);
    });
    cx.run_until_parked();
    assert_eq!(
        editor1.read_with(cx, |editor, cx| editor.text(cx)),
        "abc\ndEf\nghi\njkl\njkL\nmno\npqr\nPqr\nstu\nvwx\nyz"
    );
    assert_eq!(
        editor1
            .update(cx, |editor, cx| editor
                .selections
                .newest::<Point>(&editor.display_snapshot(cx)))
            .range(),
        Point::new(3, 0)..Point::new(3, 0)
    );

    // Keeping a range that doesn't intersect the current selection doesn't move it.
    editor1.update_in(cx, |editor, window, cx| {
        let buffer = editor.buffer().read(cx);
        let position = buffer.read(cx).anchor_before(Point::new(7, 0));
        let snapshot = buffer.snapshot(cx);
        keep_edits_in_ranges(
            editor,
            &snapshot,
            &thread,
            vec![position..position],
            window,
            cx,
        )
    });
    cx.run_until_parked();
    assert_eq!(
        editor1.read_with(cx, |editor, cx| editor.text(cx)),
        "abc\ndEf\nghi\njkl\njkL\nmno\nPqr\nstu\nvwx\nyz"
    );
    assert_eq!(
        editor1
            .update(cx, |editor, cx| editor
                .selections
                .newest::<Point>(&editor.display_snapshot(cx)))
            .range(),
        Point::new(3, 0)..Point::new(3, 0)
    );

    // Reviewing the last change opens the next changed buffer
    workspace
        .update_in(cx, |workspace, window, cx| {
            AgentDiff::global(cx).update(cx, |agent_diff, cx| {
                agent_diff.review_in_active_editor(workspace, AgentDiff::keep, window, cx)
            })
        })
        .unwrap()
        .await
        .unwrap();

    cx.run_until_parked();

    let editor2 = workspace.update(cx, |workspace, cx| {
        workspace.active_item_as::<Editor>(cx).unwrap()
    });

    let editor2_path = editor2
        .read_with(cx, |editor, cx| editor.active_project_path(cx))
        .unwrap();
    assert_eq!(editor2_path, buffer_path2);

    assert_eq!(
        editor2.read_with(cx, |editor, cx| editor.text(cx)),
        "abc\nAbc\ndef\nghi\ngHi"
    );
    assert_eq!(
        editor2
            .update(cx, |editor, cx| editor
                .selections
                .newest::<Point>(&editor.display_snapshot(cx)))
            .range(),
        Point::new(0, 0)..Point::new(0, 0)
    );

    // Editor 1 toolbar is hidden since all changes have been reviewed
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.activate_item(&editor1, true, true, window, cx)
    });

    assert!(diff_toolbar.read_with(cx, |toolbar, _cx| matches!(
        toolbar.active_item,
        Some(AgentDiffToolbarItem::Editor {
            state: EditorState::Idle,
            ..
        })
    )));
    assert_eq!(
        diff_toolbar.read_with(cx, |toolbar, cx| toolbar.location(cx)),
        ToolbarItemLocation::Hidden
    );
}

fn override_toolbar_agent_review_setting(active: bool, cx: &mut VisualTestContext) {
    cx.update(|_window, cx| {
        SettingsStore::update_global(cx, |store, _cx| {
            let mut editor_settings = store.get::<EditorSettings>(None).clone();
            editor_settings.toolbar.agent_review = active;
            store.override_global(editor_settings);
        })
    });
    cx.run_until_parked();
}
