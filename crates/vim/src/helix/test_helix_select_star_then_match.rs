use super::*;

#[gpui::test]
async fn test_helix_select_star_then_match(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();

    // Repro attempts for #52852: `*` searches for word under cursor,
    // `v` enters select, `n` accumulates matches, `m` triggers match mode.
    // Try multiple cursor positions and match counts.

    // Cursor on first occurrence, 3 more occurrences to select through
    cx.set_state(
        indoc! {"
            ˇone two one three one four one
        "},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("*");
    cx.simulate_keystrokes("v");
    cx.simulate_keystrokes("n n n");
    // Should not panic on wrapping `n`.

    // Cursor in the middle of text before matches
    cx.set_state(
        indoc! {"
            heˇllo one two one three one
        "},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("*");
    cx.simulate_keystrokes("v");
    cx.simulate_keystrokes("n");
    // Should not panic.

    // The original #52852 sequence: * v n n n then m m
    cx.set_state(
        indoc! {"
            fn ˇfoo() { bar(foo()) }
            fn baz() { foo() }
        "},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("*");
    cx.simulate_keystrokes("v");
    cx.simulate_keystrokes("n n n");
    cx.simulate_keystrokes("m m");
    // Should not panic.
}
