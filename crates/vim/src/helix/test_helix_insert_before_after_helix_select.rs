use super::*;

#[gpui::test]
async fn test_helix_insert_before_after_helix_select(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();

    // Test new line in selection direction
    cx.set_state(
        "ˇline one\nline two\nline three\nline four",
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("v j j");
    cx.assert_state(
        "«line one\nline two\nlˇ»ine three\nline four",
        Mode::HelixSelect,
    );
    cx.simulate_keystrokes("o");
    cx.assert_state("line one\nline two\nline three\nˇ\nline four", Mode::Insert);

    cx.set_state(
        "line one\nline two\nˇline three\nline four",
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("v k k");
    cx.assert_state(
        "«ˇline one\nline two\nl»ine three\nline four",
        Mode::HelixSelect,
    );
    cx.simulate_keystrokes("shift-o");
    cx.assert_state("ˇ\nline one\nline two\nline three\nline four", Mode::Insert);

    // Test new line in opposite selection direction
    cx.set_state(
        "ˇline one\nline two\nline three\nline four",
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("v j j");
    cx.assert_state(
        "«line one\nline two\nlˇ»ine three\nline four",
        Mode::HelixSelect,
    );
    cx.simulate_keystrokes("shift-o");
    cx.assert_state("ˇ\nline one\nline two\nline three\nline four", Mode::Insert);

    cx.set_state(
        "line one\nline two\nˇline three\nline four",
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("v k k");
    cx.assert_state(
        "«ˇline one\nline two\nl»ine three\nline four",
        Mode::HelixSelect,
    );
    cx.simulate_keystrokes("o");
    cx.assert_state("line one\nline two\nline three\nˇ\nline four", Mode::Insert);
}
