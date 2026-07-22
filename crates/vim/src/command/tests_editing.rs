use crate::{state::Mode, test::VimTestContext};
use editor::EditorSettings;
use gpui::TestAppContext;
use indoc::indoc;
use settings::Settings;

#[gpui::test]
async fn test_ignorecase_command(cx: &mut TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.read(|cx| {
        assert_eq!(
            EditorSettings::get_global(cx).search.case_sensitive,
            false,
            "The `case_sensitive` setting should be `false` by default."
        );
    });
    cx.simulate_keystrokes(": set space noignorecase");
    cx.simulate_keystrokes("enter");
    cx.read(|cx| {
        assert_eq!(
            EditorSettings::get_global(cx).search.case_sensitive,
            true,
            "The `case_sensitive` setting should have been enabled with `:set noignorecase`."
        );
    });
    cx.simulate_keystrokes(": set space ignorecase");
    cx.simulate_keystrokes("enter");
    cx.read(|cx| {
        assert_eq!(
            EditorSettings::get_global(cx).search.case_sensitive,
            false,
            "The `case_sensitive` setting should have been disabled with `:set ignorecase`."
        );
    });
    cx.simulate_keystrokes(": set space noic");
    cx.simulate_keystrokes("enter");
    cx.read(|cx| {
        assert_eq!(
            EditorSettings::get_global(cx).search.case_sensitive,
            true,
            "The `case_sensitive` setting should have been enabled with `:set noic`."
        );
    });
    cx.simulate_keystrokes(": set space ic");
    cx.simulate_keystrokes("enter");
    cx.read(|cx| {
        assert_eq!(
            EditorSettings::get_global(cx).search.case_sensitive,
            false,
            "The `case_sensitive` setting should have been disabled with `:set ic`."
        );
    });
}

#[gpui::test]
async fn test_sort_commands(cx: &mut TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state(
        indoc! {"
                «hornet
                quirrel
                elderbug
                cornifer
                idaˇ»
            "},
        Mode::Visual,
    );

    cx.simulate_keystrokes(": sort");
    cx.simulate_keystrokes("enter");

    cx.assert_state(
        indoc! {"
                ˇcornifer
                elderbug
                hornet
                ida
                quirrel
            "},
        Mode::Normal,
    );

    // Assert that, by default, `:sort` takes case into consideration.
    cx.set_state(
        indoc! {"
                «hornet
                quirrel
                Elderbug
                cornifer
                idaˇ»
            "},
        Mode::Visual,
    );

    cx.simulate_keystrokes(": sort");
    cx.simulate_keystrokes("enter");

    cx.assert_state(
        indoc! {"
                ˇElderbug
                cornifer
                hornet
                ida
                quirrel
            "},
        Mode::Normal,
    );

    // Assert that, if the `i` option is passed, `:sort` ignores case.
    cx.set_state(
        indoc! {"
                «hornet
                quirrel
                Elderbug
                cornifer
                idaˇ»
            "},
        Mode::Visual,
    );

    cx.simulate_keystrokes(": sort space i");
    cx.simulate_keystrokes("enter");

    cx.assert_state(
        indoc! {"
                ˇcornifer
                Elderbug
                hornet
                ida
                quirrel
            "},
        Mode::Normal,
    );

    // When no range is provided, sorts the whole buffer.
    cx.set_state(
        indoc! {"
                ˇhornet
                quirrel
                elderbug
                cornifer
                ida
            "},
        Mode::Normal,
    );

    cx.simulate_keystrokes(": sort");
    cx.simulate_keystrokes("enter");

    cx.assert_state(
        indoc! {"
                ˇcornifer
                elderbug
                hornet
                ida
                quirrel
            "},
        Mode::Normal,
    );
}

#[gpui::test]
async fn test_reflow(cx: &mut TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.update_editor(|editor, _window, cx| {
        editor.set_hard_wrap(Some(10), cx);
    });

    cx.set_state(
        indoc! {"
                ˇ0123456789 0123456789
            "},
        Mode::Normal,
    );

    cx.simulate_keystrokes(": reflow");
    cx.simulate_keystrokes("enter");

    cx.assert_state(
        indoc! {"
                0123456789
                ˇ0123456789
            "},
        Mode::Normal,
    );

    cx.set_state(
        indoc! {"
                ˇ0123456789 0123456789
            "},
        Mode::VisualLine,
    );

    cx.simulate_keystrokes("shift-v : reflow");
    cx.simulate_keystrokes("enter");

    cx.assert_state(
        indoc! {"
                0123456789
                ˇ0123456789
            "},
        Mode::Normal,
    );

    cx.set_state(
        indoc! {"
                ˇ0123 4567 0123 4567
            "},
        Mode::VisualLine,
    );

    cx.simulate_keystrokes(": reflow space 7");
    cx.simulate_keystrokes("enter");

    cx.assert_state(
        indoc! {"
                ˇ0123
                4567
                0123
                4567
            "},
        Mode::Normal,
    );

    // Assert that, if `:reflow` is invoked with an invalid argument, it
    // does not actually have any effect in the buffer's contents.
    cx.set_state(
        indoc! {"
                ˇ0123 4567 0123 4567
            "},
        Mode::VisualLine,
    );

    cx.simulate_keystrokes(": reflow space a");
    cx.simulate_keystrokes("enter");

    cx.assert_state(
        indoc! {"
                ˇ0123 4567 0123 4567
            "},
        Mode::VisualLine,
    );
}
