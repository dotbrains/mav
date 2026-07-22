use super::*;
use gpui::TestAppContext;

#[gpui::test]
async fn test_filter_sessions_by_query(cx: &mut TestAppContext) {
    let alpha = SessionMatch {
        session_id: acp::SessionId::new("session-alpha"),
        title: "Alpha Session".into(),
    };
    let beta = SessionMatch {
        session_id: acp::SessionId::new("session-beta"),
        title: "Beta Session".into(),
    };

    let sessions = vec![alpha.clone(), beta];

    let task = {
        let mut app = cx.app.borrow_mut();
        filter_sessions_by_query(
            "Alpha".into(),
            Arc::new(AtomicBool::default()),
            sessions,
            &mut app,
        )
    };

    let results = task.await;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].session_id, alpha.session_id);
}

#[gpui::test]
async fn test_search_files_path_distance_ordering(cx: &mut TestAppContext) {
    use project::Project;
    use serde_json::json;
    use util::{path, rel_path::rel_path};
    use workspace::{AppState, MultiWorkspace};

    let app_state = cx.update(|cx| {
        let state = AppState::test(cx);
        theme_settings::init(theme::LoadThemes::JustBase, cx);
        editor::init(cx);
        state
    });

    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/root"),
            json!({
                "dir1": { "a.txt": "" },
                "dir2": {
                    "a.txt": "",
                    "b.txt": ""
                }
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/root").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let worktree_id = cx.read(|cx| {
        let worktrees = workspace.read(cx).worktrees(cx).collect::<Vec<_>>();
        assert_eq!(worktrees.len(), 1);
        worktrees[0].read(cx).id()
    });

    // Open a file in dir2 to create navigation history.
    // When searching for "a.txt", dir2/a.txt should be sorted first because
    // it is closer to the most recently opened file (dir2/b.txt).
    let b_path = ProjectPath {
        worktree_id,
        path: rel_path("dir2/b.txt").into(),
    };
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path(b_path, None, true, window, cx)
        })
        .await
        .unwrap();

    let results = cx
        .update(|_window, cx| {
            search_files(
                "a.txt".into(),
                Arc::new(AtomicBool::default()),
                &workspace,
                cx,
            )
        })
        .await;

    assert_eq!(results.len(), 2, "expected 2 matching files");
    assert_eq!(
        results[0].mat.path.as_ref(),
        rel_path("dir2/a.txt"),
        "dir2/a.txt should be first because it's closer to the recently opened dir2/b.txt"
    );
    assert_eq!(
        results[1].mat.path.as_ref(),
        rel_path("dir1/a.txt"),
        "dir1/a.txt should be second"
    );
}

#[gpui::test]
async fn test_source_read_selection_editor_whole_line(cx: &mut TestAppContext) {
    use editor::Editor;
    use project::Project;
    use serde_json::json;
    use text::ToOffset as _;
    use util::path;
    use workspace::{AppState, MultiWorkspace};

    crate::conversation_view::tests::init_test(cx);

    let app_state = cx.update(AppState::test);

    app_state
        .fs
        .as_fake()
        .insert_tree(path!("/root"), json!({ "a.txt": "" }))
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/root").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let buffer = cx.new(|cx| language::Buffer::local("abc\ndef\nghi", cx));
    let editor =
        cx.new_window_entity(|window, cx| Editor::for_buffer(buffer.clone(), None, window, cx));

    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(Default::default(), window, cx, |selections| {
            selections.select_ranges([text::Point::new(1, 1)..text::Point::new(1, 1)]);
        });
    });

    let source = AgentContextSource::Editor(editor.downgrade());

    workspace.update(cx, |workspace, cx| {
        let selection = source
            .read_selection(workspace, true, cx)
            .expect("editor source with cursor on a line should yield a selection");
        assert!(
            matches!(selection, AgentContextSelection::Editor(_)),
            "expected Editor variant"
        );
        if let AgentContextSelection::Editor(ranges) = selection {
            assert_eq!(
                ranges.len(),
                1,
                "expected exactly one range for whole-line fallback"
            );
            let (range_buffer, range) = &ranges[0];
            let snapshot = range_buffer.read(cx).snapshot();
            let start_offset = range.start.to_offset(&snapshot);
            let end_offset = range.end.to_offset(&snapshot);
            assert_eq!(
                &snapshot.text()[start_offset..end_offset],
                "def",
                "whole-line fallback should capture the current row"
            );
        }

        // With include_current_line = false and no non-empty selection, the
        // fallback is suppressed and read_selection should return None.
        assert!(source.read_selection(workspace, false, cx).is_none());
    });
}
