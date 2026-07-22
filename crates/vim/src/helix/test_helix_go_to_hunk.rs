use super::*;

#[gpui::test]
async fn test_helix_go_to_hunk(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();

    cx.set_state(
        indoc! {"
        ˇone
        two
        three"},
        Mode::HelixNormal,
    );
    cx.set_head_text(indoc! {"
        one
        CHANGED
        three"});
    cx.run_until_parked();

    cx.simulate_keystrokes("]");
    assert_eq!(
        cx.active_operator(),
        Some(Operator::HelixNext { around: true })
    );

    cx.simulate_keystrokes("g");
    cx.assert_state(
        indoc! {"
        one
        ˇtwo
        three"},
        Mode::HelixNormal,
    );
    assert_eq!(cx.active_operator(), None);

    cx.set_state(
        indoc! {"
        one
        two
        ˇthree"},
        Mode::HelixNormal,
    );
    cx.set_head_text(indoc! {"
        one
        CHANGED
        three"});
    cx.run_until_parked();

    cx.simulate_keystrokes("[");
    assert_eq!(
        cx.active_operator(),
        Some(Operator::HelixPrevious { around: true })
    );

    cx.simulate_keystrokes("g");
    cx.assert_state(
        indoc! {"
        one
        ˇtwo
        three"},
        Mode::HelixNormal,
    );
    assert_eq!(cx.active_operator(), None);
}
