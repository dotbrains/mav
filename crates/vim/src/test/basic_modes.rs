use super::*;
#[gpui::test]
async fn test_initially_disabled(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, false).await;
    cx.simulate_keystrokes("h j k l");
    cx.assert_editor_state("hjklˇ");
}
#[gpui::test]
async fn test_neovim(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.simulate_shared_keystrokes("i").await;
    cx.shared_state().await.assert_matches();
    cx.simulate_shared_keystrokes("shift-t e s t space t e s t escape 0 d w")
        .await;
    cx.shared_state().await.assert_matches();
    cx.assert_editor_state("ˇtest");
}
#[gpui::test]
async fn test_toggle_through_settings(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.simulate_keystrokes("i");
    assert_eq!(cx.mode(), Mode::Insert);

    // Editor acts as though vim is disabled
    cx.disable_vim();
    cx.simulate_keystrokes("h j k l");
    cx.assert_editor_state("hjklˇ");

    // Selections aren't changed if editor is blurred but vim-mode is still disabled.
    cx.cx.set_state("«hjklˇ»");
    cx.assert_editor_state("«hjklˇ»");
    cx.update_editor(|_, window, _cx| window.blur());
    cx.assert_editor_state("«hjklˇ»");
    cx.update_editor(|_, window, cx| cx.focus_self(window));
    cx.assert_editor_state("«hjklˇ»");

    // Enabling dynamically sets vim mode again and restores normal mode
    cx.enable_vim();
    assert_eq!(cx.mode(), Mode::Normal);
    cx.simulate_keystrokes("h h h l");
    assert_eq!(cx.buffer_text(), "hjkl".to_owned());
    cx.assert_editor_state("hˇjkl");
    cx.simulate_keystrokes("i T e s t");
    cx.assert_editor_state("hTestˇjkl");

    // Disabling and enabling resets to normal mode
    assert_eq!(cx.mode(), Mode::Insert);
    cx.disable_vim();
    cx.enable_vim();
    assert_eq!(cx.mode(), Mode::Normal);
}

#[perf]
#[gpui::test]
async fn test_vim_linked_edits_delete_x(app_cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new_html(app_cx).await;

    cx.set_state("<diˇv></div>", Mode::Normal);
    cx.update_editor(|editor, _window, cx| {
        editor
            .set_linked_edit_ranges_for_testing(
                vec![(
                    Point::new(0, 1)..Point::new(0, 4),
                    vec![Point::new(0, 7)..Point::new(0, 10)],
                )],
                cx,
            )
            .expect("linked edit ranges should be set");
    });

    cx.simulate_keystrokes("x");
    cx.assert_editor_state("<diˇ></di>");
}

#[perf]
#[gpui::test]
async fn test_vim_linked_edits_change_iw(app_cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new_html(app_cx).await;

    cx.set_state("<diˇv></div>", Mode::Normal);
    cx.update_editor(|editor, _window, cx| {
        editor
            .set_linked_edit_ranges_for_testing(
                vec![(
                    Point::new(0, 1)..Point::new(0, 4),
                    vec![Point::new(0, 7)..Point::new(0, 10)],
                )],
                cx,
            )
            .expect("linked edit ranges should be set");
    });

    cx.simulate_keystrokes("c i w s p a n escape");
    cx.assert_editor_state("<spaˇn></span>");
}

#[perf]
#[gpui::test]
async fn test_vim_linked_edits_substitute_s(app_cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new_html(app_cx).await;

    cx.set_state("<diˇv></div>", Mode::Normal);
    cx.update_editor(|editor, _window, cx| {
        editor
            .set_linked_edit_ranges_for_testing(
                vec![(
                    Point::new(0, 1)..Point::new(0, 4),
                    vec![Point::new(0, 7)..Point::new(0, 10)],
                )],
                cx,
            )
            .expect("linked edit ranges should be set");
    });

    cx.simulate_keystrokes("s s p a n escape");
    cx.assert_editor_state("<dispaˇn></dispan>");
}

#[perf]
#[gpui::test]
async fn test_vim_linked_edits_visual_change(app_cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new_html(app_cx).await;

    cx.set_state("<diˇv></div>", Mode::Normal);
    cx.update_editor(|editor, _window, cx| {
        editor
            .set_linked_edit_ranges_for_testing(
                vec![(
                    Point::new(0, 1)..Point::new(0, 4),
                    vec![Point::new(0, 7)..Point::new(0, 10)],
                )],
                cx,
            )
            .expect("linked edit ranges should be set");
    });

    // Visual change routes through substitute; visual `s` shares this path.
    cx.simulate_keystrokes("v i w c s p a n escape");
    cx.assert_editor_state("<spaˇn></span>");
}

#[perf]
#[gpui::test]
async fn test_vim_linked_edits_visual_substitute_s(app_cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new_html(app_cx).await;

    cx.set_state("<diˇv></div>", Mode::Normal);
    cx.update_editor(|editor, _window, cx| {
        editor
            .set_linked_edit_ranges_for_testing(
                vec![(
                    Point::new(0, 1)..Point::new(0, 4),
                    vec![Point::new(0, 7)..Point::new(0, 10)],
                )],
                cx,
            )
            .expect("linked edit ranges should be set");
    });

    cx.simulate_keystrokes("v i w s s p a n escape");
    cx.assert_editor_state("<spaˇn></span>");
}

#[perf]
#[gpui::test]
async fn test_cancel_selection(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state(
        indoc! {"The quick brown fox juˇmps over the lazy dog"},
        Mode::Normal,
    );
    // jumps
    cx.simulate_keystrokes("v l l");
    cx.assert_editor_state("The quick brown fox ju«mpsˇ» over the lazy dog");

    cx.simulate_keystrokes("escape");
    cx.assert_editor_state("The quick brown fox jumpˇs over the lazy dog");

    // go back to the same selection state
    cx.simulate_keystrokes("v h h");
    cx.assert_editor_state("The quick brown fox ju«ˇmps» over the lazy dog");

    // Ctrl-[ should behave like Esc
    cx.simulate_keystrokes("ctrl-[");
    cx.assert_editor_state("The quick brown fox juˇmps over the lazy dog");
}
