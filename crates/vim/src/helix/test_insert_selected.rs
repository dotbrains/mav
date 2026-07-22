use super::*;

#[gpui::test]
async fn test_insert_selected(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();
    cx.set_state(
        indoc! {"
        «The ˇ»quick brown
        fox jumps over
        the lazy dog."},
        Mode::HelixNormal,
    );

    cx.simulate_keystrokes("i");

    cx.assert_state(
        indoc! {"
        ˇThe quick brown
        fox jumps over
        the lazy dog."},
        Mode::Insert,
    );
}
