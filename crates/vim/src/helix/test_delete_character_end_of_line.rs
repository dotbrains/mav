use super::*;

#[gpui::test]
async fn test_delete_character_end_of_line(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state(
        indoc! {"
        The quick brownˇ
        fox jumps over
        the lazy dog."},
        Mode::HelixNormal,
    );

    cx.simulate_keystrokes("d");

    cx.assert_state(
        indoc! {"
        The quick brownˇfox jumps over
        the lazy dog."},
        Mode::HelixNormal,
    );
}
