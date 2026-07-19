use super::*;
use editor::test::editor_test_context::assert_state_with_diff;
use gpui::{BorrowAppContext, TestAppContext};
use project::{FakeFs, Fs, Project};
use settings::{DiffViewStyle, SettingsStore};
use std::path::PathBuf;
use unindent::unindent;
use util::path;
use workspace::MultiWorkspace;

fn init_test(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.editor.diff_view_style = Some(DiffViewStyle::Unified);
            });
        });
        theme_settings::init(theme::LoadThemes::JustBase, cx);
    });
}

#[gpui::test]
async fn test_diff_view(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/test"),
        serde_json::json!({
            "old_file.txt": "old line 1\nline 2\nold line 3\nline 4\n",
            "new_file.txt": "new line 1\nline 2\nnew line 3\nline 4\n"
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/test").as_ref()], cx).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let diff_view = workspace
        .update_in(cx, |workspace, window, cx| {
            FileDiffView::open(
                path!("/test/old_file.txt").into(),
                path!("/test/new_file.txt").into(),
                workspace.weak_handle(),
                window,
                cx,
            )
        })
        .await
        .unwrap();

    assert_state_with_diff(
        &diff_view.read_with(cx, |diff_view, cx| {
            diff_view.editor.read(cx).rhs_editor().clone()
        }),
        cx,
        &unindent(
            "
            - old line 1
            + ˇnew line 1
              line 2
            - old line 3
            + new line 3
              line 4
            ",
        ),
    );

    fs.save(
        path!("/test/new_file.txt").as_ref(),
        &unindent(
            "
            new line 1
            line 2
            new line 3
            line 4
            new line 5
            ",
        )
        .into(),
        Default::default(),
    )
    .await
    .unwrap();

    cx.executor().advance_clock(RECALCULATE_DIFF_DEBOUNCE);
    assert_state_with_diff(
        &diff_view.read_with(cx, |diff_view, cx| {
            diff_view.editor.read(cx).rhs_editor().clone()
        }),
        cx,
        &unindent(
            "
            - old line 1
            + ˇnew line 1
              line 2
            - old line 3
            + new line 3
              line 4
            + new line 5
            ",
        ),
    );

    fs.save(
        path!("/test/old_file.txt").as_ref(),
        &unindent(
            "
            new line 1
            line 2
            old line 3
            line 4
            ",
        )
        .into(),
        Default::default(),
    )
    .await
    .unwrap();

    cx.executor().advance_clock(RECALCULATE_DIFF_DEBOUNCE);
    assert_state_with_diff(
        &diff_view.read_with(cx, |diff_view, cx| {
            diff_view.editor.read(cx).rhs_editor().clone()
        }),
        cx,
        &unindent(
            "
              ˇnew line 1
              line 2
            - old line 3
            + new line 3
              line 4
            + new line 5
            ",
        ),
    );

    diff_view.read_with(cx, |diff_view, cx| {
        assert_eq!(
            diff_view.tab_content_text(0, cx),
            "old_file.txt ↔ new_file.txt"
        );
        assert_eq!(
            diff_view.tab_tooltip_text(cx).unwrap(),
            format!(
                "{} ↔ {}",
                path!("test/old_file.txt"),
                path!("test/new_file.txt")
            )
        );
    })
}

#[gpui::test]
async fn test_save_changes_in_diff_view(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/test"),
        serde_json::json!({
            "old_file.txt": "old line 1\nline 2\nold line 3\nline 4\n",
            "new_file.txt": "new line 1\nline 2\nnew line 3\nline 4\n"
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/test".as_ref()], cx).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let diff_view = workspace
        .update_in(cx, |workspace, window, cx| {
            FileDiffView::open(
                PathBuf::from(path!("/test/old_file.txt")),
                PathBuf::from(path!("/test/new_file.txt")),
                workspace.weak_handle(),
                window,
                cx,
            )
        })
        .await
        .unwrap();

    diff_view.update_in(cx, |diff_view, window, cx| {
        diff_view.editor.update(cx, |splittable, cx| {
            splittable.rhs_editor().update(cx, |editor, cx| {
                editor.insert("modified ", window, cx);
            });
        });
    });

    diff_view.update_in(cx, |diff_view, _, cx| {
        let buffer = diff_view.new_buffer.read(cx);
        assert!(buffer.is_dirty(), "Buffer should be dirty after edits");
    });

    let save_task = diff_view.update_in(cx, |diff_view, window, cx| {
        workspace::Item::save(
            diff_view,
            workspace::item::SaveOptions::default(),
            project.clone(),
            window,
            cx,
        )
    });

    save_task.await.expect("Save should succeed");

    let saved_content = fs.load(path!("/test/new_file.txt").as_ref()).await.unwrap();
    assert_eq!(
        saved_content,
        "modified new line 1\nline 2\nnew line 3\nline 4\n"
    );

    diff_view.update_in(cx, |diff_view, _, cx| {
        let buffer = diff_view.new_buffer.read(cx);
        assert!(!buffer.is_dirty(), "Buffer should not be dirty after save");
    });
}
