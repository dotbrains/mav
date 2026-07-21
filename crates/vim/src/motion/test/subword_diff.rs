use super::*;

#[gpui::test]
async fn test_next_subword_start(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    // Setup custom keybindings for subword motions so we can use the bindings
    // in `simulate_keystrokes`.
    cx.update(|_window, cx| {
        cx.bind_keys([KeyBinding::new(
            "w",
            super::NextSubwordStart {
                ignore_punctuation: false,
            },
            None,
        )]);
    });

    cx.set_state("ˇfoo.bar", Mode::Normal);
    cx.simulate_keystrokes("w");
    cx.assert_state("foo.ˇbar", Mode::Normal);

    cx.set_state("ˇfoo(bar)", Mode::Normal);
    cx.simulate_keystrokes("w");
    cx.assert_state("fooˇ(bar)", Mode::Normal);
    cx.simulate_keystrokes("w");
    cx.assert_state("foo(ˇbar)", Mode::Normal);
    cx.simulate_keystrokes("w");
    cx.assert_state("foo(barˇ)", Mode::Normal);

    cx.set_state("ˇfoo_bar_baz", Mode::Normal);
    cx.simulate_keystrokes("w");
    cx.assert_state("foo_ˇbar_baz", Mode::Normal);
    cx.simulate_keystrokes("w");
    cx.assert_state("foo_bar_ˇbaz", Mode::Normal);

    cx.set_state("ˇfooBarBaz", Mode::Normal);
    cx.simulate_keystrokes("w");
    cx.assert_state("fooˇBarBaz", Mode::Normal);
    cx.simulate_keystrokes("w");
    cx.assert_state("fooBarˇBaz", Mode::Normal);

    cx.set_state("ˇfoo;bar", Mode::Normal);
    cx.simulate_keystrokes("w");
    cx.assert_state("foo;ˇbar", Mode::Normal);

    cx.set_state("ˇ<?php\n\n$someVariable = 2;", Mode::Normal);
    cx.simulate_keystrokes("w");
    cx.assert_state("<?ˇphp\n\n$someVariable = 2;", Mode::Normal);
    cx.simulate_keystrokes("w");
    cx.assert_state("<?php\nˇ\n$someVariable = 2;", Mode::Normal);
    cx.simulate_keystrokes("w");
    cx.assert_state("<?php\n\nˇ$someVariable = 2;", Mode::Normal);
    cx.simulate_keystrokes("w");
    cx.assert_state("<?php\n\n$ˇsomeVariable = 2;", Mode::Normal);
    cx.simulate_keystrokes("w");
    cx.assert_state("<?php\n\n$someˇVariable = 2;", Mode::Normal);
    cx.simulate_keystrokes("w");
    cx.assert_state("<?php\n\n$someVariable ˇ= 2;", Mode::Normal);
    cx.simulate_keystrokes("w");
    cx.assert_state("<?php\n\n$someVariable = ˇ2;", Mode::Normal);
}

#[gpui::test]
async fn test_next_subword_end(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    // Setup custom keybindings for subword motions so we can use the bindings
    // in `simulate_keystrokes`.
    cx.update(|_window, cx| {
        cx.bind_keys([KeyBinding::new(
            "e",
            super::NextSubwordEnd {
                ignore_punctuation: false,
            },
            None,
        )]);
    });

    cx.set_state("ˇfoo.bar", Mode::Normal);
    cx.simulate_keystrokes("e");
    cx.assert_state("foˇo.bar", Mode::Normal);
    cx.simulate_keystrokes("e");
    cx.assert_state("fooˇ.bar", Mode::Normal);
    cx.simulate_keystrokes("e");
    cx.assert_state("foo.baˇr", Mode::Normal);

    cx.set_state("ˇfoo(bar)", Mode::Normal);
    cx.simulate_keystrokes("e");
    cx.assert_state("foˇo(bar)", Mode::Normal);
    cx.simulate_keystrokes("e");
    cx.assert_state("fooˇ(bar)", Mode::Normal);
    cx.simulate_keystrokes("e");
    cx.assert_state("foo(baˇr)", Mode::Normal);
    cx.simulate_keystrokes("e");
    cx.assert_state("foo(barˇ)", Mode::Normal);

    cx.set_state("ˇfoo_bar_baz", Mode::Normal);
    cx.simulate_keystrokes("e");
    cx.assert_state("foˇo_bar_baz", Mode::Normal);
    cx.simulate_keystrokes("e");
    cx.assert_state("foo_baˇr_baz", Mode::Normal);
    cx.simulate_keystrokes("e");
    cx.assert_state("foo_bar_baˇz", Mode::Normal);

    cx.set_state("ˇfooBarBaz", Mode::Normal);
    cx.simulate_keystrokes("e");
    cx.set_state("foˇoBarBaz", Mode::Normal);
    cx.simulate_keystrokes("e");
    cx.set_state("fooBaˇrBaz", Mode::Normal);
    cx.simulate_keystrokes("e");
    cx.set_state("fooBarBaˇz", Mode::Normal);

    cx.set_state("ˇfoo;bar", Mode::Normal);
    cx.simulate_keystrokes("e");
    cx.set_state("foˇo;bar", Mode::Normal);
    cx.simulate_keystrokes("e");
    cx.set_state("fooˇ;bar", Mode::Normal);
    cx.simulate_keystrokes("e");
    cx.set_state("foo;baˇr", Mode::Normal);
}

#[gpui::test]
async fn test_previous_subword_start(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    // Setup custom keybindings for subword motions so we can use the bindings
    // in `simulate_keystrokes`.
    cx.update(|_window, cx| {
        cx.bind_keys([KeyBinding::new(
            "b",
            super::PreviousSubwordStart {
                ignore_punctuation: false,
            },
            None,
        )]);
    });

    cx.set_state("foo.barˇ", Mode::Normal);
    cx.simulate_keystrokes("b");
    cx.assert_state("foo.ˇbar", Mode::Normal);
    cx.simulate_keystrokes("b");
    cx.assert_state("fooˇ.bar", Mode::Normal);
    cx.simulate_keystrokes("b");
    cx.assert_state("ˇfoo.bar", Mode::Normal);

    cx.set_state("foo(barˇ)", Mode::Normal);
    cx.simulate_keystrokes("b");
    cx.assert_state("foo(ˇbar)", Mode::Normal);
    cx.simulate_keystrokes("b");
    cx.assert_state("fooˇ(bar)", Mode::Normal);
    cx.simulate_keystrokes("b");
    cx.assert_state("ˇfoo(bar)", Mode::Normal);

    cx.set_state("foo_bar_bazˇ", Mode::Normal);
    cx.simulate_keystrokes("b");
    cx.assert_state("foo_bar_ˇbaz", Mode::Normal);
    cx.simulate_keystrokes("b");
    cx.assert_state("foo_ˇbar_baz", Mode::Normal);
    cx.simulate_keystrokes("b");
    cx.assert_state("ˇfoo_bar_baz", Mode::Normal);

    cx.set_state("fooBarBazˇ", Mode::Normal);
    cx.simulate_keystrokes("b");
    cx.assert_state("fooBarˇBaz", Mode::Normal);
    cx.simulate_keystrokes("b");
    cx.assert_state("fooˇBarBaz", Mode::Normal);
    cx.simulate_keystrokes("b");
    cx.assert_state("ˇfooBarBaz", Mode::Normal);

    cx.set_state("foo;barˇ", Mode::Normal);
    cx.simulate_keystrokes("b");
    cx.assert_state("foo;ˇbar", Mode::Normal);
    cx.simulate_keystrokes("b");
    cx.assert_state("ˇfoo;bar", Mode::Normal);

    cx.set_state("<?php\n\n$someVariable = 2ˇ;", Mode::Normal);
    cx.simulate_keystrokes("b");
    cx.assert_state("<?php\n\n$someVariable = ˇ2;", Mode::Normal);
    cx.simulate_keystrokes("b");
    cx.assert_state("<?php\n\n$someVariable ˇ= 2;", Mode::Normal);
    cx.simulate_keystrokes("b");
    cx.assert_state("<?php\n\n$someˇVariable = 2;", Mode::Normal);
    cx.simulate_keystrokes("b");
    cx.assert_state("<?php\n\n$ˇsomeVariable = 2;", Mode::Normal);
    cx.simulate_keystrokes("b");
    cx.assert_state("<?php\n\nˇ$someVariable = 2;", Mode::Normal);
    cx.simulate_keystrokes("b");
    cx.assert_state("<?php\nˇ\n$someVariable = 2;", Mode::Normal);
    cx.simulate_keystrokes("b");
    cx.assert_state("<?ˇphp\n\n$someVariable = 2;", Mode::Normal);
    cx.simulate_keystrokes("b");
    cx.assert_state("ˇ<?php\n\n$someVariable = 2;", Mode::Normal);
}

#[gpui::test]
async fn test_previous_subword_end(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    // Setup custom keybindings for subword motions so we can use the bindings
    // in `simulate_keystrokes`.
    cx.update(|_window, cx| {
        cx.bind_keys([KeyBinding::new(
            "g e",
            super::PreviousSubwordEnd {
                ignore_punctuation: false,
            },
            None,
        )]);
    });

    cx.set_state("foo.baˇr", Mode::Normal);
    cx.simulate_keystrokes("g e");
    cx.assert_state("fooˇ.bar", Mode::Normal);
    cx.simulate_keystrokes("g e");
    cx.assert_state("foˇo.bar", Mode::Normal);

    cx.set_state("foo(barˇ)", Mode::Normal);
    cx.simulate_keystrokes("g e");
    cx.assert_state("foo(baˇr)", Mode::Normal);
    cx.simulate_keystrokes("g e");
    cx.assert_state("fooˇ(bar)", Mode::Normal);
    cx.simulate_keystrokes("g e");
    cx.assert_state("foˇo(bar)", Mode::Normal);

    cx.set_state("foo_bar_baˇz", Mode::Normal);
    cx.simulate_keystrokes("g e");
    cx.assert_state("foo_baˇr_baz", Mode::Normal);
    cx.simulate_keystrokes("g e");
    cx.assert_state("foˇo_bar_baz", Mode::Normal);

    cx.set_state("fooBarBaˇz", Mode::Normal);
    cx.simulate_keystrokes("g e");
    cx.assert_state("fooBaˇrBaz", Mode::Normal);
    cx.simulate_keystrokes("g e");
    cx.assert_state("foˇoBarBaz", Mode::Normal);

    cx.set_state("foo;baˇr", Mode::Normal);
    cx.simulate_keystrokes("g e");
    cx.assert_state("fooˇ;bar", Mode::Normal);
    cx.simulate_keystrokes("g e");
    cx.assert_state("foˇo;bar", Mode::Normal);
}

#[gpui::test]
async fn test_method_motion_with_expanded_diff_hunks(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    let diff_base = indoc! {r#"
            fn first() {
                println!("first");
                println!("removed line");
            }

            fn second() {
                println!("second");
            }

            fn third() {
                println!("third");
            }
        "#};

    let current_text = indoc! {r#"
            fn first() {
                println!("first");
            }

            fn second() {
                println!("second");
            }

            fn third() {
                println!("third");
            }
        "#};

    cx.set_state(&format!("ˇ{}", current_text), Mode::Normal);
    cx.set_head_text(diff_base);
    cx.update_editor(|editor, window, cx| {
        editor.expand_all_diff_hunks(&editor::actions::ExpandAllDiffHunks, window, cx);
    });

    // When diff hunks are expanded, the deleted line from the diff base
    // appears in the MultiBuffer. The method motion should correctly
    // navigate to the second function even with this extra content.
    cx.simulate_keystrokes("] m");
    cx.assert_editor_state(indoc! {r#"
            fn first() {
                println!("first");
                println!("removed line");
            }

            ˇfn second() {
                println!("second");
            }

            fn third() {
                println!("third");
            }
        "#});

    cx.simulate_keystrokes("] m");
    cx.assert_editor_state(indoc! {r#"
            fn first() {
                println!("first");
                println!("removed line");
            }

            fn second() {
                println!("second");
            }

            ˇfn third() {
                println!("third");
            }
        "#});

    cx.simulate_keystrokes("[ m");
    cx.assert_editor_state(indoc! {r#"
            fn first() {
                println!("first");
                println!("removed line");
            }

            ˇfn second() {
                println!("second");
            }

            fn third() {
                println!("third");
            }
        "#});

    cx.simulate_keystrokes("[ m");
    cx.assert_editor_state(indoc! {r#"
            ˇfn first() {
                println!("first");
                println!("removed line");
            }

            fn second() {
                println!("second");
            }

            fn third() {
                println!("third");
            }
        "#});
}

#[gpui::test]
async fn test_comment_motion_with_expanded_diff_hunks(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    let diff_base = indoc! {r#"
            // first comment
            fn first() {
                // removed comment
                println!("first");
            }

            // second comment
            fn second() { println!("second"); }
        "#};

    let current_text = indoc! {r#"
            // first comment
            fn first() {
                println!("first");
            }

            // second comment
            fn second() { println!("second"); }
        "#};

    cx.set_state(&format!("ˇ{}", current_text), Mode::Normal);
    cx.set_head_text(diff_base);
    cx.update_editor(|editor, window, cx| {
        editor.expand_all_diff_hunks(&editor::actions::ExpandAllDiffHunks, window, cx);
    });

    // The first `] /` (vim::NextComment) should go to the end of the first
    // comment.
    cx.simulate_keystrokes("] /");
    cx.assert_editor_state(indoc! {r#"
            // first commenˇt
            fn first() {
                // removed comment
                println!("first");
            }

            // second comment
            fn second() { println!("second"); }
        "#});

    // The next `] /` (vim::NextComment) should go to the end of the second
    // comment, skipping over the removed comment, since it's not in the
    // actual buffer.
    cx.simulate_keystrokes("] /");
    cx.assert_editor_state(indoc! {r#"
            // first comment
            fn first() {
                // removed comment
                println!("first");
            }

            // second commenˇt
            fn second() { println!("second"); }
        "#});

    // Going back to previous comment with `[ /` (vim::PreviousComment)
    // should go back to the start of the second comment.
    cx.simulate_keystrokes("[ /");
    cx.assert_editor_state(indoc! {r#"
            // first comment
            fn first() {
                // removed comment
                println!("first");
            }

            ˇ// second comment
            fn second() { println!("second"); }
        "#});

    // Going back again with `[ /` (vim::PreviousComment) should finally put
    // the cursor at the start of the first comment.
    cx.simulate_keystrokes("[ /");
    cx.assert_editor_state(indoc! {r#"
            ˇ// first comment
            fn first() {
                // removed comment
                println!("first");
            }

            // second comment
            fn second() { println!("second"); }
        "#});
}
