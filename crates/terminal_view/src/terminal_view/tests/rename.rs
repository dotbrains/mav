use super::*;
use common::*;

// Terminal rename tests

#[gpui::test]
async fn test_custom_title_initially_none(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let (project, workspace) = init_test(cx).await;

    let terminal = project
        .update(cx, |project, cx| project.create_terminal_shell(None, cx))
        .await
        .unwrap();

    let terminal_view = cx
        .add_window(|window, cx| {
            TerminalView::new(
                terminal,
                workspace.downgrade(),
                None,
                project.downgrade(),
                window,
                cx,
            )
        })
        .root(cx)
        .unwrap();

    terminal_view.update(cx, |view, _cx| {
        assert!(view.custom_title().is_none());
    });
}

#[gpui::test]
async fn test_set_custom_title(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let (project, workspace) = init_test(cx).await;

    let terminal = project
        .update(cx, |project, cx| project.create_terminal_shell(None, cx))
        .await
        .unwrap();

    let terminal_view = cx
        .add_window(|window, cx| {
            TerminalView::new(
                terminal,
                workspace.downgrade(),
                None,
                project.downgrade(),
                window,
                cx,
            )
        })
        .root(cx)
        .unwrap();

    terminal_view.update(cx, |view, cx| {
        view.set_custom_title(Some("frontend".to_string()), cx);
        assert_eq!(view.custom_title(), Some("frontend"));
    });
}

#[gpui::test]
async fn test_set_custom_title_empty_becomes_none(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let (project, workspace) = init_test(cx).await;

    let terminal = project
        .update(cx, |project, cx| project.create_terminal_shell(None, cx))
        .await
        .unwrap();

    let terminal_view = cx
        .add_window(|window, cx| {
            TerminalView::new(
                terminal,
                workspace.downgrade(),
                None,
                project.downgrade(),
                window,
                cx,
            )
        })
        .root(cx)
        .unwrap();

    terminal_view.update(cx, |view, cx| {
        view.set_custom_title(Some("test".to_string()), cx);
        assert_eq!(view.custom_title(), Some("test"));

        view.set_custom_title(Some("".to_string()), cx);
        assert!(view.custom_title().is_none());

        view.set_custom_title(Some("  ".to_string()), cx);
        assert!(view.custom_title().is_none());
    });
}

#[gpui::test]
async fn test_custom_title_marks_needs_serialize(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let (project, workspace) = init_test(cx).await;

    let terminal = project
        .update(cx, |project, cx| project.create_terminal_shell(None, cx))
        .await
        .unwrap();

    let terminal_view = cx
        .add_window(|window, cx| {
            TerminalView::new(
                terminal,
                workspace.downgrade(),
                None,
                project.downgrade(),
                window,
                cx,
            )
        })
        .root(cx)
        .unwrap();

    terminal_view.update(cx, |view, cx| {
        view.needs_serialize = false;
        view.set_custom_title(Some("new_label".to_string()), cx);
        assert!(view.needs_serialize);
    });
}

#[gpui::test]
async fn test_tab_content_uses_custom_title(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let (project, workspace) = init_test(cx).await;

    let terminal = project
        .update(cx, |project, cx| project.create_terminal_shell(None, cx))
        .await
        .unwrap();

    let terminal_view = cx
        .add_window(|window, cx| {
            TerminalView::new(
                terminal,
                workspace.downgrade(),
                None,
                project.downgrade(),
                window,
                cx,
            )
        })
        .root(cx)
        .unwrap();

    terminal_view.update(cx, |view, cx| {
        view.set_custom_title(Some("my-server".to_string()), cx);
        let text = view.tab_content_text(0, cx);
        assert_eq!(text.as_ref(), "my-server");
    });

    terminal_view.update(cx, |view, cx| {
        view.set_custom_title(None, cx);
        let text = view.tab_content_text(0, cx);
        assert_ne!(text.as_ref(), "my-server");
    });
}

#[gpui::test]
async fn test_tab_content_shows_terminal_title_when_custom_title_directly_set_empty(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();

    let (project, workspace) = init_test(cx).await;

    let terminal = project
        .update(cx, |project, cx| project.create_terminal_shell(None, cx))
        .await
        .unwrap();

    let terminal_view = cx
        .add_window(|window, cx| {
            TerminalView::new(
                terminal,
                workspace.downgrade(),
                None,
                project.downgrade(),
                window,
                cx,
            )
        })
        .root(cx)
        .unwrap();

    terminal_view.update(cx, |view, cx| {
        view.custom_title = Some("".to_string());
        let text = view.tab_content_text(0, cx);
        assert!(
            !text.is_empty(),
            "Tab should show terminal title, not empty string; got: '{}'",
            text
        );
    });

    terminal_view.update(cx, |view, cx| {
        view.custom_title = Some("   ".to_string());
        let text = view.tab_content_text(0, cx);
        assert!(
            !text.is_empty() && text.as_ref() != "   ",
            "Tab should show terminal title, not whitespace; got: '{}'",
            text
        );
    });
}
