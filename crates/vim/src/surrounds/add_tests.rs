use gpui::KeyBinding;
use indoc::indoc;

use crate::{
    PushAddSurrounds,
    object::{AnyBrackets, AnyQuotes, MiniBrackets, MiniQuotes},
    state::Mode,
    test::VimTestContext,
};

#[gpui::test]
async fn test_add_surrounds(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    // test add surrounds with around
    cx.set_state(
        indoc! {"
            The quˇick brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("y s i w {");
    cx.assert_state(
        indoc! {"
            The ˇ{ quick } brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );

    // test add surrounds not with around
    cx.set_state(
        indoc! {"
            The quˇick brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("y s i w }");
    cx.assert_state(
        indoc! {"
            The ˇ{quick} brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );

    // test add surrounds with motion
    cx.set_state(
        indoc! {"
            The quˇick brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("y s $ }");
    cx.assert_state(
        indoc! {"
            The quˇ{ick brown}
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );

    // test add surrounds with multi cursor
    cx.set_state(
        indoc! {"
            The quˇick brown
            fox jumps over
            the laˇzy dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("y s i w '");
    cx.assert_state(
        indoc! {"
            The ˇ'quick' brown
            fox jumps over
            the ˇ'lazy' dog."},
        Mode::Normal,
    );

    // test multi cursor add surrounds with motion
    cx.set_state(
        indoc! {"
            The quˇick brown
            fox jumps over
            the laˇzy dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("y s $ '");
    cx.assert_state(
        indoc! {"
            The quˇ'ick brown'
            fox jumps over
            the laˇ'zy dog.'"},
        Mode::Normal,
    );

    // test multi cursor add surrounds with motion and custom string
    cx.set_state(
        indoc! {"
            The quˇick brown
            fox jumps over
            the laˇzy dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("y s $ 1");
    cx.assert_state(
        indoc! {"
            The quˇ1ick brown1
            fox jumps over
            the laˇ1zy dog.1"},
        Mode::Normal,
    );

    // test add surrounds with motion current line
    cx.set_state(
        indoc! {"
            The quˇick brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("y s s {");
    cx.assert_state(
        indoc! {"
            ˇ{ The quick brown }
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );

    cx.set_state(
        indoc! {"
                The quˇick brown•
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("y s s {");
    cx.assert_state(
        indoc! {"
                ˇ{ The quick brown }•
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("2 y s s )");
    cx.assert_state(
        indoc! {"
                ˇ({ The quick brown }•
            fox jumps over)
            the lazy dog."},
        Mode::Normal,
    );

    // test add surrounds around object
    cx.set_state(
        indoc! {"
            The [quˇick] brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("y s a ] )");
    cx.assert_state(
        indoc! {"
            The ˇ([quick]) brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );

    // test add surrounds inside object
    cx.set_state(
        indoc! {"
            The [quˇick] brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("y s i ] )");
    cx.assert_state(
        indoc! {"
            The [ˇ(quick)] brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );
}

#[gpui::test]
async fn test_add_surrounds_visual(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.update(|_, cx| {
        cx.bind_keys([KeyBinding::new(
            "shift-s",
            PushAddSurrounds {},
            Some("vim_mode == visual"),
        )])
    });

    // test add surrounds with around
    cx.set_state(
        indoc! {"
            The quˇick brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("v i w shift-s {");
    cx.assert_state(
        indoc! {"
            The ˇ{ quick } brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );

    // test add surrounds not with around
    cx.set_state(
        indoc! {"
            The quˇick brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("v i w shift-s }");
    cx.assert_state(
        indoc! {"
            The ˇ{quick} brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );

    // test add surrounds with motion
    cx.set_state(
        indoc! {"
            The quˇick brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("v e shift-s }");
    cx.assert_state(
        indoc! {"
            The quˇ{ick} brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );

    // test add surrounds with multi cursor
    cx.set_state(
        indoc! {"
            The quˇick brown
            fox jumps over
            the laˇzy dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("v i w shift-s '");
    cx.assert_state(
        indoc! {"
            The ˇ'quick' brown
            fox jumps over
            the ˇ'lazy' dog."},
        Mode::Normal,
    );

    // test add surrounds with visual block
    cx.set_state(
        indoc! {"
            The quˇick brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("ctrl-v i w j j shift-s '");
    cx.assert_state(
        indoc! {"
            The ˇ'quick' brown
            fox 'jumps' over
            the 'lazy 'dog."},
        Mode::Normal,
    );

    // test add surrounds with visual line
    cx.set_state(
        indoc! {"
            The quˇick brown
            fox jumps over
            the lazy dog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("j shift-v shift-s '");
    cx.assert_state(
        indoc! {"
            The quick brown
            ˇ'
            fox jumps over
            '
            the lazy dog."},
        Mode::Normal,
    );
}
