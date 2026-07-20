use super::*;

struct EmptyModalView {
    focus_handle: gpui::FocusHandle,
}

impl EventEmitter<DismissEvent> for EmptyModalView {}

impl Render for EmptyModalView {
    fn render(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
        div()
    }
}

impl Focusable for EmptyModalView {
    fn focus_handle(&self, _cx: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl workspace::ModalView for EmptyModalView {}

impl EmptyModalView {
    fn new(cx: &App) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
        }
    }
}

#[gpui::test]
async fn test_hide_mouse_context_menu_on_modal_opened(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let buffer = cx.update(|cx| MultiBuffer::build_simple("hello world!", cx));
    let cx = &mut VisualTestContext::from_window(*window, cx);
    let editor = cx.new_window_entity(|window, cx| {
        Editor::new(
            EditorMode::full(),
            buffer,
            Some(project.clone()),
            window,
            cx,
        )
    });
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item_to_active_pane(Box::new(editor.clone()), None, true, window, cx);
    });

    editor.update_in(cx, |editor, window, cx| {
        editor.open_context_menu(&OpenContextMenu, window, cx);
        assert!(editor.mouse_context_menu.is_some());
    });
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.toggle_modal(window, cx, |_, cx| EmptyModalView::new(cx));
    });

    cx.read(|cx| {
        assert!(editor.read(cx).mouse_context_menu.is_none());
    });
}

#[gpui::test]
async fn test_hide_pending_blame_popover_when_modal_opens(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |multi_workspace, _| multi_workspace.workspace().clone())
        .unwrap();
    let multi_buffer = cx.update(|cx| MultiBuffer::build_simple("Buffer Contents!", cx));
    let buffer_id = multi_buffer.read_with(cx, |multi_buffer, cx| {
        multi_buffer
            .all_buffers_iter()
            .next()
            .expect("Should have at least one buffer")
            .read(cx)
            .remote_id()
    });
    let cx = &mut VisualTestContext::from_window(*window, cx);
    let editor = cx.new_window_entity(|window, cx| {
        Editor::new(
            EditorMode::full(),
            multi_buffer,
            Some(project.clone()),
            window,
            cx,
        )
    });

    workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item_to_active_pane(Box::new(editor.clone()), None, true, window, cx);
    });

    editor.update_in(cx, |editor, _, cx| {
        editor.blame = Some(
            cx.new(|cx| GitBlame::new(editor.buffer.clone(), project.clone(), false, true, cx)),
        );
        editor.show_blame_popover(
            buffer_id,
            &::git::blame::BlameEntry {
                sha: "1b1b1b".parse().unwrap(),
                range: 0..1,
                original_line_number: 0,
                author: None,
                author_mail: None,
                author_time: None,
                author_tz: None,
                committer_name: None,
                committer_email: None,
                committer_time: None,
                committer_tz: None,
                summary: None,
                previous: None,
                filename: String::new(),
            },
            gpui::point(gpui::px(0.), gpui::px(0.)),
            false,
            cx,
        );

        assert!(editor.inline_blame_popover_show_task.is_some());
        assert!(editor.inline_blame_popover.is_none());
    });

    workspace.update_in(cx, |workspace, window, cx| {
        workspace.toggle_modal(window, cx, |_, cx| EmptyModalView::new(cx));
    });

    // Toggling a modal while the blame popover task is still pending should
    // clear both the task and any rendered popover.
    editor.update_in(cx, |editor, _, _| {
        assert!(editor.inline_blame_popover.is_none());
        assert!(editor.inline_blame_popover_show_task.is_none());
    });
}
