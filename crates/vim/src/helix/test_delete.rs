use super::*;

#[gpui::test]
async fn test_delete(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();

    // test delete a selection
    cx.set_state(
        indoc! {"
        The qu«ick ˇ»brown
        fox jumps over
        the lazy dog."},
        Mode::HelixNormal,
    );

    cx.simulate_keystrokes("d");

    cx.assert_state(
        indoc! {"
        The quˇbrown
        fox jumps over
        the lazy dog."},
        Mode::HelixNormal,
    );

    // test deleting a single character
    cx.simulate_keystrokes("d");

    cx.assert_state(
        indoc! {"
        The quˇrown
        fox jumps over
        the lazy dog."},
        Mode::HelixNormal,
    );
}
