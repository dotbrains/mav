use super::*;

#[gpui::test]
async fn test_save_after_restore(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({
            ".git": {},
            "foo.txt": "FOO\n",
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/project").as_ref()], cx).await;

    fs.set_head_for_repo(
        path!("/project/.git").as_ref(),
        &[("foo.txt", "foo\n".into())],
        "deadbeef",
    );
    fs.set_index_for_repo(
        path!("/project/.git").as_ref(),
        &[("foo.txt", "foo\n".into())],
    );

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    let diff =
        cx.new_window_entity(|window, cx| ProjectDiff::new(project.clone(), workspace, window, cx));
    cx.run_until_parked();

    let editor = diff.read_with(cx, |diff, cx| diff.editor.read(cx).rhs_editor().clone());
    assert_state_with_diff(
        &editor,
        cx,
        &"
                - ˇfoo
                + FOO
            "
        .unindent(),
    );

    editor
        .update_in(cx, |editor, window, cx| {
            editor.git_restore(&Default::default(), window, cx);
            editor.save(SaveOptions::default(), project.clone(), window, cx)
        })
        .await
        .unwrap();
    cx.run_until_parked();

    assert_state_with_diff(&editor, cx, &"ˇ".unindent());

    let text = String::from_utf8(fs.read_file_sync("/project/foo.txt").unwrap()).unwrap();
    assert_eq!(text, "foo\n");
}

#[gpui::test]
async fn test_scroll_to_beginning_with_deletion(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({
            ".git": {},
            "bar": "BAR\n",
            "foo": "FOO\n",
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/project").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    let diff =
        cx.new_window_entity(|window, cx| ProjectDiff::new(project.clone(), workspace, window, cx));
    cx.run_until_parked();

    fs.set_head_and_index_for_repo(
        path!("/project/.git").as_ref(),
        &[("bar", "bar\n".into()), ("foo", "foo\n".into())],
    );
    cx.run_until_parked();

    let editor = cx.update_window_entity(&diff, |diff, window, cx| {
        diff.move_to_path(
            PathKey::with_sort_prefix(2, rel_path("foo").into_arc()),
            window,
            cx,
        );
        diff.editor.read(cx).rhs_editor().clone()
    });
    assert_state_with_diff(
        &editor,
        cx,
        &"
                - bar
                + BAR

                - ˇfoo
                + FOO
            "
        .unindent(),
    );

    let editor = cx.update_window_entity(&diff, |diff, window, cx| {
        diff.move_to_path(
            PathKey::with_sort_prefix(2, rel_path("bar").into_arc()),
            window,
            cx,
        );
        diff.editor.read(cx).rhs_editor().clone()
    });
    assert_state_with_diff(
        &editor,
        cx,
        &"
                - ˇbar
                + BAR

                - foo
                + FOO
            "
        .unindent(),
    );
}

#[gpui::test]
async fn test_hunks_after_restore_then_modify(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({
            ".git": {},
            "foo": "modified\n",
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/project").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    fs.set_head_for_repo(
        path!("/project/.git").as_ref(),
        &[("foo", "original\n".into())],
        "deadbeef",
    );

    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/project/foo"), cx)
        })
        .await
        .unwrap();
    let buffer_editor = cx.new_window_entity(|window, cx| {
        Editor::for_buffer(buffer, Some(project.clone()), window, cx)
    });
    let diff =
        cx.new_window_entity(|window, cx| ProjectDiff::new(project.clone(), workspace, window, cx));
    cx.run_until_parked();

    let diff_editor = diff.read_with(cx, |diff, cx| diff.editor.read(cx).rhs_editor().clone());

    assert_state_with_diff(
        &diff_editor,
        cx,
        &"
                - ˇoriginal
                + modified
            "
        .unindent(),
    );

    let prev_buffer_hunks = cx.update_window_entity(&buffer_editor, |buffer_editor, window, cx| {
        let snapshot = buffer_editor.snapshot(window, cx);
        let snapshot = &snapshot.buffer_snapshot();
        let prev_buffer_hunks = buffer_editor
            .diff_hunks_in_ranges(&[editor::Anchor::Min..editor::Anchor::Max], snapshot)
            .collect::<Vec<_>>();
        buffer_editor.git_restore(&Default::default(), window, cx);
        prev_buffer_hunks
    });
    assert_eq!(prev_buffer_hunks.len(), 1);
    cx.run_until_parked();

    let new_buffer_hunks = cx.update_window_entity(&buffer_editor, |buffer_editor, window, cx| {
        let snapshot = buffer_editor.snapshot(window, cx);
        let snapshot = &snapshot.buffer_snapshot();
        buffer_editor
            .diff_hunks_in_ranges(&[editor::Anchor::Min..editor::Anchor::Max], snapshot)
            .collect::<Vec<_>>()
    });
    assert_eq!(new_buffer_hunks.as_slice(), &[]);

    cx.update_window_entity(&buffer_editor, |buffer_editor, window, cx| {
        buffer_editor.set_text("different\n", window, cx);
        buffer_editor.save(
            SaveOptions {
                format: false,
                force_format: false,
                autosave: false,
            },
            project.clone(),
            window,
            cx,
        )
    })
    .await
    .unwrap();

    cx.run_until_parked();

    cx.update_window_entity(&buffer_editor, |buffer_editor, window, cx| {
        buffer_editor.expand_all_diff_hunks(&Default::default(), window, cx);
    });

    assert_state_with_diff(
        &buffer_editor,
        cx,
        &"
                - original
                + different
                  ˇ"
        .unindent(),
    );

    assert_state_with_diff(
        &diff_editor,
        cx,
        &"
                - ˇoriginal
                + different
            "
        .unindent(),
    );
}

use crate::project_diff::{self, ProjectDiff};
