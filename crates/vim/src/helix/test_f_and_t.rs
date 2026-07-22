use super::*;

#[gpui::test]
async fn test_f_and_t(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();

    cx.set_state(
        indoc! {"
        The quˇick brown
        fox jumps over
        the lazy dog."},
        Mode::HelixNormal,
    );

    cx.simulate_keystrokes("f z");

    cx.assert_state(
        indoc! {"
            The qu«ick brown
            fox jumps over
            the lazˇ»y dog."},
        Mode::HelixNormal,
    );

    cx.simulate_keystrokes("F e F e");

    cx.assert_state(
        indoc! {"
            The quick brown
            fox jumps ov«ˇer
            the» lazy dog."},
        Mode::HelixNormal,
    );

    cx.simulate_keystrokes("e 2 F e");

    cx.assert_state(
        indoc! {"
            Th«ˇe quick brown
            fox jumps over»
            the lazy dog."},
        Mode::HelixNormal,
    );

    cx.simulate_keystrokes("t r t r");

    cx.assert_state(
        indoc! {"
            The quick «brown
            fox jumps oveˇ»r
            the lazy dog."},
        Mode::HelixNormal,
    );
}
