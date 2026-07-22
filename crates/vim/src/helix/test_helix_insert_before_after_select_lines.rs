use super::*;

#[gpui::test]
async fn test_helix_insert_before_after_select_lines(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state(
        "line one\nline ˇtwo\nline three\nline four",
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("2 x");
    cx.assert_state(
        "line one\n«line two\nline three\nˇ»line four",
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("o");
    cx.assert_state("line one\nline two\nline three\nˇ\nline four", Mode::Insert);

    cx.set_state(
        "line one\nline ˇtwo\nline three\nline four",
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("2 x");
    cx.assert_state(
        "line one\n«line two\nline three\nˇ»line four",
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("shift-o");
    cx.assert_state("line one\nˇ\nline two\nline three\nline four", Mode::Insert);
}
