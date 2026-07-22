use super::*;

#[gpui::test]
async fn test_helix_end_of_document(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();

    // ge lands at column 0 of the last line, regardless of current column
    cx.set_state(
        indoc! {"
          fooˇbar
        baz"},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("g e");
    cx.assert_state(
        indoc! {"
          foobar
        ˇbaz"},
        Mode::HelixNormal,
    );

    // ge with an active selection collapses to column 0 of the last line
    cx.set_state(
        indoc! {"
        «foo barˇ»
        baz"},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("g e");
    cx.assert_state(
        indoc! {"
        foo bar
        ˇbaz"},
        Mode::HelixNormal,
    );

    // a count is ignored; ge always goes to the last line
    cx.set_state(
        indoc! {"
          line oneˇ
        line two
        line three"},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("2 g e");
    cx.assert_state(
        indoc! {"
          line one
        line two
        ˇline three"},
        Mode::HelixNormal,
    );

    // v ge extends the selection to col 0 of the last line
    cx.set_state(
        indoc! {"
        ˇline one
        line two
        line three"},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("v g e");
    cx.assert_state(
        indoc! {"
        «line one
        line two
        lˇ»ine three"},
        Mode::HelixSelect,
    );

    // ge in select mode with a reversed selection extends forward to the last line
    cx.set_state(
        indoc! {"
        line one
        line «ˇtwo»
        line three"},
        Mode::HelixSelect,
    );
    cx.simulate_keystrokes("g e");
    cx.assert_state(
        indoc! {"
        line one
        line tw«o
        lˇ»ine three"},
        Mode::HelixSelect,
    );
}
