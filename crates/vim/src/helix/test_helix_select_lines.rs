use super::*;

#[gpui::test]
async fn test_helix_select_lines(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.set_state(
        "line one\nline ˇtwo\nline three\nline four",
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("2 x");
    cx.assert_state(
        "line one\n«line two\nline three\nˇ»line four",
        Mode::HelixNormal,
    );

    // Test extending existing line selection
    cx.set_state(
        indoc! {"
        li«ˇne one
        li»ne two
        line three
        line four"},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("x");
    cx.assert_state(
        indoc! {"
        «line one
        line two
        ˇ»line three
        line four"},
        Mode::HelixNormal,
    );

    // Pressing x in empty line, select next line (because helix considers cursor a selection)
    cx.set_state(
        indoc! {"
        line one
        ˇ
        line three
        line four
        line five
        line six"},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("x");
    cx.assert_state(
        indoc! {"
        line one
        «
        line three
        ˇ»line four
        line five
        line six"},
        Mode::HelixNormal,
    );

    // Another x should only select the next line
    cx.simulate_keystrokes("x");
    cx.assert_state(
        indoc! {"
        line one
        «
        line three
        line four
        ˇ»line five
        line six"},
        Mode::HelixNormal,
    );

    // Empty line with count selects extra + count lines
    cx.set_state(
        indoc! {"
        line one
        ˇ
        line three
        line four
        line five"},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("2 x");
    cx.assert_state(
        indoc! {"
        line one
        «
        line three
        line four
        ˇ»line five"},
        Mode::HelixNormal,
    );

    // Compare empty vs non-empty line behavior
    cx.set_state(
        indoc! {"
        ˇnon-empty line
        line two
        line three"},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("x");
    cx.assert_state(
        indoc! {"
        «non-empty line
        ˇ»line two
        line three"},
        Mode::HelixNormal,
    );

    // Same test but with empty line - should select one extra
    cx.set_state(
        indoc! {"
        ˇ
        line two
        line three"},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("x");
    cx.assert_state(
        indoc! {"
        «
        line two
        ˇ»line three"},
        Mode::HelixNormal,
    );

    // Test selecting multiple lines with count
    cx.set_state(
        indoc! {"
        ˇline one
        line two
        line threeˇ
        line four
        line five"},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("x");
    cx.assert_state(
        indoc! {"
        «line one
        ˇ»line two
        «line three
        ˇ»line four
        line five"},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("x");
    // Adjacent line selections stay separate (not merged)
    cx.assert_state(
        indoc! {"
        «line one
        line two
        ˇ»«line three
        line four
        ˇ»line five"},
        Mode::HelixNormal,
    );

    // Test selecting with an empty line below the current line
    cx.set_state(
        indoc! {"
        line one
        line twoˇ

        line four
        line five"},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("x");
    cx.assert_state(
        indoc! {"
        line one
        «line two
        ˇ»
        line four
        line five"},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("x");
    cx.assert_state(
        indoc! {"
        line one
        «line two

        ˇ»line four
        line five"},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("x");
    cx.assert_state(
        indoc! {"
        line one
        «line two

        line four
        ˇ»line five"},
        Mode::HelixNormal,
    );

    cx.set_state("oneˇ\ntwo\nthree", Mode::HelixNormal);
    cx.simulate_keystrokes("d u x");
    cx.assert_state("«one\nˇ»two\nthree", Mode::HelixNormal);
}
