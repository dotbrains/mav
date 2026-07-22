use super::*;

#[gpui::test]
async fn test_helix_substitute(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state("ˇone two", Mode::HelixNormal);
    cx.simulate_keystrokes("c");
    cx.assert_state("ˇne two", Mode::Insert);

    cx.set_state("«oneˇ» two", Mode::HelixNormal);
    cx.simulate_keystrokes("c");
    cx.assert_state("ˇ two", Mode::Insert);

    cx.set_state(
        indoc! {"
        oneˇ two
        three
        "},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("x c");
    cx.assert_state(
        indoc! {"
        ˇ
        three
        "},
        Mode::Insert,
    );

    cx.set_state(
        indoc! {"
        one twoˇ
        three
        "},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("c");
    cx.assert_state(
        indoc! {"
        one twoˇthree
        "},
        Mode::Insert,
    );

    // Helix doesn't set the cursor to the first non-blank one when
    // replacing lines: it uses language-dependent indent queries instead.
    cx.set_state(
        indoc! {"
        one two
        «    indented
        three not indentedˇ»
        "},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("c");
    cx.set_state(
        indoc! {"
        one two
        ˇ
        "},
        Mode::Insert,
    );
}
