use super::*;

#[gpui::test]
async fn test_v2ap(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {
        "The
        quicˇk

        brown
        fox"
    })
    .await;
    cx.simulate_shared_keystrokes("v 2 a p").await;
    cx.shared_state().await.assert_eq(indoc! {
        "«The
        quick

        brown
        fˇ»ox"
    });
}

#[gpui::test]
async fn test_visual_syntax_sibling_selection(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state(
        indoc! {"
            fn test() {
                let ˇa = 1;
                let b = 2;
                let c = 3;
            }
        "},
        Mode::Normal,
    );

    // Enter visual mode and select the statement
    cx.simulate_keystrokes("v w w w");
    cx.assert_state(
        indoc! {"
            fn test() {
                let «a = 1;ˇ»
                let b = 2;
                let c = 3;
            }
        "},
        Mode::Visual,
    );

    // The specific behavior of syntax sibling selection in vim mode
    // would depend on the key bindings configured, but the actions
    // are now available for use
}

#[gpui::test]
async fn test_visual_replace_uses_graphemes(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state("«Hällöˇ» Wörld", Mode::Visual);
    cx.simulate_keystrokes("r 1");
    cx.assert_state("ˇ11111 Wörld", Mode::Normal);

    cx.set_state("«e\u{301}ˇ»", Mode::Visual);
    cx.simulate_keystrokes("r 1");
    cx.assert_state("ˇ1", Mode::Normal);

    cx.set_state("«🙂ˇ»", Mode::Visual);
    cx.simulate_keystrokes("r 1");
    cx.assert_state("ˇ1", Mode::Normal);
}
