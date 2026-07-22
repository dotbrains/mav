use super::support::*;
use super::*;

struct EditorWithRightOccluders {
    editor: Entity<crate::Editor>,
    editor_width: Pixels,
    right_dock_width: Option<Pixels>,
    right_sidebar_width: Option<Pixels>,
}

impl Render for EditorWithRightOccluders {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .size_full()
            .child(
                div()
                    .h_full()
                    .w(self.editor_width)
                    .overflow_hidden()
                    .child(self.editor.clone()),
            )
            .when_some(self.right_dock_width, |this, width| {
                this.child(
                    div()
                        .h_full()
                        .w(width)
                        .flex_shrink_0()
                        .occlude()
                        .debug_selector(|| "right_dock".into()),
                )
            })
            .when_some(self.right_sidebar_width, |this, width| {
                this.child(
                    div()
                        .h_full()
                        .w(width)
                        .flex_shrink_0()
                        .occlude()
                        .debug_selector(|| "right_sidebar".into()),
                )
            })
    }
}

async fn assert_edit_prediction_diff_popover_avoids_right_occluders(
    cx: &mut gpui::TestAppContext,
    right_dock_width: Option<Pixels>,
    right_sidebar_width: Option<Pixels>,
) {
    init_test(cx, |_| {});

    let editor_width = px(700.);
    let window_width = editor_width
        + right_dock_width.unwrap_or_default()
        + right_sidebar_width.unwrap_or_default();
    let buffer = cx.update(|cx| MultiBuffer::build_simple("", cx));
    let window = cx.add_window(|window, cx| {
        let editor = cx.new(|cx| build_editor(buffer, window, cx));
        window.focus(&editor.focus_handle(cx), cx);
        EditorWithRightOccluders {
            editor,
            editor_width,
            right_dock_width,
            right_sidebar_width,
        }
    });
    let editor = window
        .read_with(cx, |root, _| root.editor.clone())
        .expect("test window should contain editor");
    let mut cx = gpui::VisualTestContext::from_window(*window, cx);
    cx.simulate_resize(size(window_width, px(500.)));
    cx.run_until_parked();

    let mut cx = EditorTestContext::for_editor_in(editor, &mut cx).await;
    let provider = cx.new(|_| FakeEditPredictionDelegate::default());
    assign_editor_completion_provider(provider.clone(), &mut cx);
    cx.update_editor(|editor, _, _| {
        editor.set_menu_edit_predictions_policy(MenuEditPredictionsPolicy::Never);
    });
    cx.set_state("abcdefghijklmnopqrstuvwxyzabcdefghijklmnˇopqrstuvwxyzabcdef");

    propose_edits(&provider, vec![(40..41, "REPLACEMENT")], &mut cx);
    cx.update_editor(|editor, window, cx| editor.update_visible_edit_prediction(window, cx));
    cx.editor(|editor, _, _| {
        assert!(!editor.edit_prediction_visible_in_cursor_popover(true));
        assert!(matches!(
            editor
                .active_edit_prediction
                .as_ref()
                .map(|state| &state.completion),
            Some(EditPrediction::Edit {
                display_mode: crate::EditDisplayMode::DiffPopover,
                ..
            })
        ));
    });
    cx.cx.update(|window, cx| {
        window.refresh();
        let _ = window.draw(cx);
    });

    cx.editor(|editor, _, _| {
        assert!(
            editor.last_position_map.is_some(),
            "editor should have rendered a position map"
        );
    });

    let popover_bounds = cx
        .cx
        .debug_bounds("edit_prediction_diff_popover")
        .expect("diff popover should render");

    for selector in ["right_dock", "right_sidebar"] {
        if let Some(occluder_bounds) = cx.cx.debug_bounds(selector) {
            assert!(
                !popover_bounds.intersects(&occluder_bounds),
                "diff popover {popover_bounds:?} should not overlap {selector} {occluder_bounds:?}"
            );
        }
    }
}

#[gpui::test]
async fn test_edit_prediction_diff_popover_avoids_right_sidebar(cx: &mut gpui::TestAppContext) {
    assert_edit_prediction_diff_popover_avoids_right_occluders(cx, None, Some(px(300.))).await;
}

#[gpui::test]
async fn test_edit_prediction_diff_popover_avoids_right_dock(cx: &mut gpui::TestAppContext) {
    assert_edit_prediction_diff_popover_avoids_right_occluders(cx, Some(px(300.)), None).await;
}

#[gpui::test]
async fn test_edit_prediction_diff_popover_avoids_right_dock_and_sidebar(
    cx: &mut gpui::TestAppContext,
) {
    assert_edit_prediction_diff_popover_avoids_right_occluders(cx, Some(px(300.)), Some(px(300.)))
        .await;
}
