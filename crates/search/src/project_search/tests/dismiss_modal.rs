use super::*;

#[gpui::test]
async fn test_search_dismisses_modal(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "one.rs": "const ONE: usize = 1;",
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    struct EmptyModalView {
        focus_handle: gpui::FocusHandle,
    }
    impl EventEmitter<gpui::DismissEvent> for EmptyModalView {}
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

    workspace.update_in(cx, |workspace, window, cx| {
        workspace.toggle_modal(window, cx, |_, cx| EmptyModalView {
            focus_handle: cx.focus_handle(),
        });
        assert!(workspace.has_active_modal(window, cx));
    });

    cx.dispatch_action(Deploy::find());

    workspace.update_in(cx, |workspace, window, cx| {
        assert!(!workspace.has_active_modal(window, cx));
        workspace.toggle_modal(window, cx, |_, cx| EmptyModalView {
            focus_handle: cx.focus_handle(),
        });
        assert!(workspace.has_active_modal(window, cx));
    });

    cx.dispatch_action(DeploySearch::default());

    workspace.update_in(cx, |workspace, window, cx| {
        assert!(!workspace.has_active_modal(window, cx));
    });
}
