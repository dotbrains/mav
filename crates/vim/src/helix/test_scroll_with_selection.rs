use super::*;

#[gpui::test]
async fn test_scroll_with_selection(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();

    // Start with a selection
    cx.set_state(
        indoc! {"
        «lineˇ» one
        line two
        line three
        line four
        line five"},
        Mode::HelixNormal,
    );

    // Scroll down, selection should collapse
    cx.simulate_keystrokes("ctrl-d");
    cx.assert_state(
        indoc! {"
        line one
        line two
        line three
        line four
        line fiveˇ"},
        Mode::HelixNormal,
    );

    // Make a new selection
    cx.simulate_keystroke("b");
    cx.assert_state(
        indoc! {"
        line one
        line two
        line three
        line four
        line «ˇfive»"},
        Mode::HelixNormal,
    );

    // And scroll up, once again collapsing the selection.
    cx.simulate_keystroke("ctrl-u");
    cx.assert_state(
        indoc! {"
        line one
        line two
        line three
        line ˇfour
        line five"},
        Mode::HelixNormal,
    );

    // Enter select mode
    cx.simulate_keystroke("v");
    cx.assert_state(
        indoc! {"
        line one
        line two
        line three
        line «fˇ»our
        line five"},
        Mode::HelixSelect,
    );

    // And now the selection should be kept/expanded.
    cx.simulate_keystroke("ctrl-d");
    cx.assert_state(
        indoc! {"
        line one
        line two
        line three
        line «four
        line fiveˇ»"},
        Mode::HelixSelect,
    );
}
