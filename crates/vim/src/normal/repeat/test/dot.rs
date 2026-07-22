
use editor::test::editor_lsp_test_context::EditorLspTestContext;
use futures::StreamExt;
use indoc::indoc;

use gpui::EntityInputHandler;

use crate::{
    VimGlobals,
    state::Mode,
    test::{NeovimBackedTestContext, VimTestContext},
};
#[gpui::test]
async fn test_dot_repeat(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    // "o"
    cx.set_shared_state("ˇhello").await;
    cx.simulate_shared_keystrokes("o w o r l d escape").await;
    cx.shared_state().await.assert_eq("hello\nworlˇd");
    cx.simulate_shared_keystrokes(".").await;
    cx.shared_state().await.assert_eq("hello\nworld\nworlˇd");

    // "d"
    cx.simulate_shared_keystrokes("^ d f o").await;
    cx.simulate_shared_keystrokes("g g .").await;
    cx.shared_state().await.assert_eq("ˇ\nworld\nrld");

    // "p" (note that it pastes the current clipboard)
    cx.simulate_shared_keystrokes("j y y p").await;
    cx.simulate_shared_keystrokes("shift-g y y .").await;
    cx.shared_state()
        .await
        .assert_eq("\nworld\nworld\nrld\nˇrld");

    // "~" (note that counts apply to the action taken, not . itself)
    cx.set_shared_state("ˇthe quick brown fox").await;
    cx.simulate_shared_keystrokes("2 ~ .").await;
    cx.set_shared_state("THE ˇquick brown fox").await;
    cx.simulate_shared_keystrokes("3 .").await;
    cx.set_shared_state("THE QUIˇck brown fox").await;
    cx.run_until_parked();
    cx.simulate_shared_keystrokes(".").await;
    cx.shared_state().await.assert_eq("THE QUICK ˇbrown fox");

    // "q l" (note after macro should be used last change made by macro)
    cx.set_shared_state("ˇ").await;
    cx.simulate_shared_keystrokes("q l shift-o h e l l o space w o r l d escape q")
        .await;
    cx.simulate_shared_keystrokes("@ l").await;
    cx.shared_state()
        .await
        .assert_eq("hello worlˇd\nhello world\n");
    cx.simulate_shared_keystrokes(".").await;
    cx.shared_state()
        .await
        .assert_eq("hello worlˇd\nhello world\nhello world\n");
}

#[gpui::test]
async fn test_dot_repeat_after_macro_change_motion(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state("ˇfoo foo", Mode::Normal);
    cx.simulate_keystrokes("q l c f o x escape q");
    cx.assert_state("ˇxo foo", Mode::Normal);

    cx.simulate_keystrokes("w @ l");
    cx.assert_state("xo ˇxo", Mode::Normal);

    cx.simulate_keystrokes(".");
    cx.assert_state("xo ˇx", Mode::Normal);
}

#[gpui::test]
async fn test_dot_repeat_registers_paste(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    // basic paste repeat uses the unnamed register
    cx.set_shared_state("ˇhello\n").await;
    cx.simulate_shared_keystrokes("y y p").await;
    cx.shared_state().await.assert_eq("hello\nˇhello\n");
    cx.simulate_shared_keystrokes(".").await;
    cx.shared_state().await.assert_eq("hello\nhello\nˇhello\n");

    // "_ (blackhole) is recorded and replayed, so the pasted text is still
    // the original yanked line.
    cx.set_shared_state(indoc! {"
            ˇone
            two
            three
            four
        "})
        .await;
    cx.simulate_shared_keystrokes("y y j \" _ d d . p").await;
    cx.shared_state().await.assert_eq(indoc! {"
            one
            four
            ˇone
        "});

    // the recorded register is replayed, not whatever is in the unnamed register
    cx.set_shared_state(indoc! {"
            ˇone
            two
        "})
        .await;
    cx.simulate_shared_keystrokes("y y j \" a y y \" a p .")
        .await;
    cx.shared_state().await.assert_eq(indoc! {"
            one
            two
            two
            ˇtwo
        "});

    // `"X.` ignores the override and always uses the recorded register.
    // Both `dd` calls go into register `a`, so register `b` is empty and
    // `"bp` pastes nothing.
    cx.set_shared_state(indoc! {"
            ˇone
            two
            three
        "})
        .await;
    cx.simulate_shared_keystrokes("\" a d d \" b .").await;
    cx.shared_state().await.assert_eq(indoc! {"
            ˇthree
        "});
    cx.simulate_shared_keystrokes("\" a p \" b p").await;
    cx.shared_state().await.assert_eq(indoc! {"
            three
            ˇtwo
        "});

    // numbered registers cycle on each dot repeat: "1p . . uses registers 2, 3, …
    // Since the cycling behavior caps at register 9, the first line to be
    // deleted `1`, is no longer in any of the registers.
    cx.set_shared_state(indoc! {"
            ˇone
            two
            three
            four
            five
            six
            seven
            eight
            nine
            ten
        "})
        .await;
    cx.simulate_shared_keystrokes("d d . . . . . . . . .").await;
    cx.shared_state().await.assert_eq(indoc! {"ˇ"});
    cx.simulate_shared_keystrokes("\" 1 p . . . . . . . . .")
        .await;
    cx.shared_state().await.assert_eq(indoc! {"

            ten
            nine
            eight
            seven
            six
            five
            four
            three
            two
            ˇtwo"});

    // unnamed register repeat: dd records None, so . pastes the same
    // deleted text
    cx.set_shared_state(indoc! {"
            ˇone
            two
            three
        "})
        .await;
    cx.simulate_shared_keystrokes("d d p .").await;
    cx.shared_state().await.assert_eq(indoc! {"
            two
            one
            ˇone
            three
        "});

    // After `"1p` cycles to `2`, using `"ap` resets recorded_register to `a`,
    // so the next `.` uses `a` and not 3.
    cx.set_shared_state(indoc! {"
            one
            two
            ˇthree
        "})
        .await;
    cx.simulate_shared_keystrokes("\" 2 y y k k \" a y y j \" 1 y y k \" 1 p . \" a p .")
        .await;
    cx.shared_state().await.assert_eq(indoc! {"
            one
            two
            three
            one
            ˇone
            two
            three
        "});
}

// This needs to be a separate test from `test_dot_repeat_registers_paste`
// as Neovim doesn't have support for using registers in replace operations
// by default.
#[gpui::test]
async fn test_dot_repeat_registers_replace(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state(
        indoc! {"
            line ˇone
            line two
            line three
        "},
        Mode::Normal,
    );

    // 1. Yank `one` into register `a`
    // 2. Move down and yank `two` into the default register
    // 3. Replace `two` with the contents of register `a`
    cx.simulate_keystrokes("\" a y w j y w \" a g R w");
    cx.assert_state(
        indoc! {"
            line one
            line onˇe
            line three
        "},
        Mode::Normal,
    );

    // 1. Move down to `three`
    // 2. Repeat the replace operation
    cx.simulate_keystrokes("j .");
    cx.assert_state(
        indoc! {"
            line one
            line one
            line onˇe
        "},
        Mode::Normal,
    );

    // Similar test, but this time using numbered registers, as those should
    // automatically increase on successive uses of `.` .
    cx.set_state(
        indoc! {"
            line ˇone
            line two
            line three
            line four
        "},
        Mode::Normal,
    );

    // 1. Yank `one` into register `1`
    // 2. Yank `two` into register `2`
    // 3. Move down and yank `three` into the default register
    // 4. Replace `three` with the contents of register `1`
    // 5. Move down and repeat
    cx.simulate_keystrokes("\" 1 y w j \" 2 y w j y w \" 1 g R w j .");
    cx.assert_state(
        indoc! {"
            line one
            line two
            line one
            line twˇo
        "},
        Mode::Normal,
    );
}
