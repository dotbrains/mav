use super::*;

#[gpui::test]
async fn test_append(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();
    // test from the end of the selection
    cx.set_state(
        indoc! {"
        «Theˇ» quick brown
        fox jumps over
        the lazy dog."},
        Mode::HelixNormal,
    );

    cx.simulate_keystrokes("a");

    cx.assert_state(
        indoc! {"
        Theˇ quick brown
        fox jumps over
        the lazy dog."},
        Mode::Insert,
    );

    // test from the beginning of the selection
    cx.set_state(
        indoc! {"
        «ˇThe» quick brown
        fox jumps over
        the lazy dog."},
        Mode::HelixNormal,
    );

    cx.simulate_keystrokes("a");

    cx.assert_state(
        indoc! {"
        Theˇ quick brown
        fox jumps over
        the lazy dog."},
        Mode::Insert,
    );
}
