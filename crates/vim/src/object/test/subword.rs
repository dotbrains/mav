use super::*;

#[gpui::test]
async fn test_subword_object(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    // Setup custom keybindings for subword object so we can use the
    // bindings in `simulate_keystrokes`.
    cx.update(|_window, cx| {
        cx.bind_keys([KeyBinding::new(
            "w",
            super::Subword {
                ignore_punctuation: false,
            },
            Some("vim_operator"),
        )]);
    });

    cx.set_state("foo_ˇbar_baz", Mode::Normal);
    cx.simulate_keystrokes("c i w");
    cx.assert_state("foo_ˇ_baz", Mode::Insert);

    cx.set_state("ˇfoo_bar_baz", Mode::Normal);
    cx.simulate_keystrokes("c i w");
    cx.assert_state("ˇ_bar_baz", Mode::Insert);

    cx.set_state("foo_bar_baˇz", Mode::Normal);
    cx.simulate_keystrokes("c i w");
    cx.assert_state("foo_bar_ˇ", Mode::Insert);

    cx.set_state("fooˇBarBaz", Mode::Normal);
    cx.simulate_keystrokes("c i w");
    cx.assert_state("fooˇBaz", Mode::Insert);

    cx.set_state("ˇfooBarBaz", Mode::Normal);
    cx.simulate_keystrokes("c i w");
    cx.assert_state("ˇBarBaz", Mode::Insert);

    cx.set_state("fooBarBaˇz", Mode::Normal);
    cx.simulate_keystrokes("c i w");
    cx.assert_state("fooBarˇ", Mode::Insert);

    cx.set_state("foo.ˇbar.baz", Mode::Normal);
    cx.simulate_keystrokes("c i w");
    cx.assert_state("foo.ˇ.baz", Mode::Insert);

    cx.set_state("foo_ˇbar_baz", Mode::Normal);
    cx.simulate_keystrokes("d i w");
    cx.assert_state("foo_ˇ_baz", Mode::Normal);

    cx.set_state("fooˇBarBaz", Mode::Normal);
    cx.simulate_keystrokes("d i w");
    cx.assert_state("fooˇBaz", Mode::Normal);

    cx.set_state("foo_ˇbar_baz", Mode::Normal);
    cx.simulate_keystrokes("c a w");
    cx.assert_state("foo_ˇ_baz", Mode::Insert);

    cx.set_state("fooˇBarBaz", Mode::Normal);
    cx.simulate_keystrokes("c a w");
    cx.assert_state("fooˇBaz", Mode::Insert);

    cx.set_state("foo_ˇbar_baz", Mode::Normal);
    cx.simulate_keystrokes("d a w");
    cx.assert_state("foo_ˇ_baz", Mode::Normal);

    cx.set_state("fooˇBarBaz", Mode::Normal);
    cx.simulate_keystrokes("d a w");
    cx.assert_state("fooˇBaz", Mode::Normal);
}
