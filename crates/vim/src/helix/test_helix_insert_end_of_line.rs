use super::*;

#[gpui::test]
async fn test_helix_insert_end_of_line(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();

    // Ensure that, when lines are selected using `x`, pressing `shift-a`
    // actually puts the cursor at the end of the selected lines and not at
    // the end of the line below.
    cx.set_state(
        indoc! {"
        line oˇne
        line two"},
        Mode::HelixNormal,
    );

    cx.simulate_keystrokes("x");
    cx.assert_state(
        indoc! {"
        «line one
        ˇ»line two"},
        Mode::HelixNormal,
    );

    cx.simulate_keystrokes("shift-a");
    cx.assert_state(
        indoc! {"
        line oneˇ
        line two"},
        Mode::Insert,
    );

    cx.set_state(
        indoc! {"
        line «one
        lineˇ» two"},
        Mode::HelixNormal,
    );

    cx.simulate_keystrokes("shift-a");
    cx.assert_state(
        indoc! {"
        line one
        line twoˇ"},
        Mode::Insert,
    );
}
