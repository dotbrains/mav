use super::support::*;
use super::*;

#[gpui::test]
async fn test_cancel_clears_stale_edit_prediction_in_menu(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});
    load_default_keymap(cx);

    let mut cx = EditorTestContext::new(cx).await;
    let provider = cx.new(|_| FakeEditPredictionDelegate::default());
    assign_editor_completion_provider(provider.clone(), &mut cx);
    cx.set_state("let x = ˇ;");

    propose_edits(&provider, vec![(8..8, "42")], &mut cx);
    cx.update_editor(|editor, window, cx| editor.update_visible_edit_prediction(window, cx));

    cx.update_editor(|editor, _window, _cx| {
        assert!(editor.active_edit_prediction.is_some());
        assert!(editor.stale_edit_prediction_in_menu.is_none());
    });

    cx.simulate_keystroke("escape");
    cx.run_until_parked();

    cx.update_editor(|editor, _window, _cx| {
        assert!(editor.active_edit_prediction.is_none());
        assert!(editor.stale_edit_prediction_in_menu.is_none());
    });
}

#[gpui::test]
async fn test_discard_clears_delegate_completion(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});
    load_default_keymap(cx);

    let mut cx = EditorTestContext::new(cx).await;
    let provider = cx.new(|_| FakeEditPredictionDelegate::default());
    assign_editor_completion_provider(provider.clone(), &mut cx);
    cx.set_state("let x = ˇ;");

    propose_edits(&provider, vec![(8..8, "42")], &mut cx);
    cx.update_editor(|editor, window, cx| editor.update_visible_edit_prediction(window, cx));

    cx.update_editor(|editor, _window, _cx| {
        assert!(editor.active_edit_prediction.is_some());
    });

    // Dismiss the prediction — this must call discard() on the delegate,
    // which should clear self.completion.
    cx.simulate_keystroke("escape");
    cx.run_until_parked();

    cx.update_editor(|editor, _window, _cx| {
        assert!(editor.active_edit_prediction.is_none());
    });

    // update_visible_edit_prediction must NOT bring the prediction back,
    // because discard() cleared self.completion in the delegate.
    cx.update_editor(|editor, window, cx| editor.update_visible_edit_prediction(window, cx));

    cx.update_editor(|editor, _window, _cx| {
        assert!(
            editor.active_edit_prediction.is_none(),
            "prediction must not resurface after discard()"
        );
    });
}
