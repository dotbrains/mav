use super::*;

#[gpui::test]
async fn test_helix_start_of_document(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();

    // gg lands at column 0 of the first line, regardless of current column
    cx.set_state(
        indoc! {"
        foo
          barˇbaz"},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("g g");
    cx.assert_state(
        indoc! {"
        ˇfoo
          barbaz"},
        Mode::HelixNormal,
    );

    // gg with an active selection collapses to column 0 of the first line
    cx.set_state(
        indoc! {"
        foo
        «bar bazˇ»"},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("g g");
    cx.assert_state(
        indoc! {"
        ˇfoo
        bar baz"},
        Mode::HelixNormal,
    );

    // a count goes to that line number at column 0
    cx.set_state(
        indoc! {"
        line one
        line two
          line threeˇ"},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("2 g g");
    cx.assert_state(
        indoc! {"
        line one
        ˇline two
          line three"},
        Mode::HelixNormal,
    );

    // a count larger than the number of lines clips to the last line
    cx.set_state(
        indoc! {"
        line one
        line two
        ˇline three"},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("9 9 9 g g");
    cx.assert_state(
        indoc! {"
        line one
        line two
        ˇline three"},
        Mode::HelixNormal,
    );

    // v gg extends the selection backward to col 0 of the first line
    cx.set_state(
        indoc! {"
        line one
        ˇline two
        line three"},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("v g g");
    cx.assert_state(
        indoc! {"
        «ˇline one
        l»ine two
        line three"},
        Mode::HelixSelect,
    );

    // gg in select mode with a reversed selection extends further backward
    cx.set_state(
        indoc! {"
        line one
        line «ˇtwo»
        line three"},
        Mode::HelixSelect,
    );
    cx.simulate_keystrokes("g g");
    cx.assert_state(
        indoc! {"
        «ˇline one
        line two»
        line three"},
        Mode::HelixSelect,
    );
}
