
use indoc::indoc;

use crate::{
    state::Mode,
    test::{NeovimBackedTestContext, VimTestContext},
};
#[gpui::test]
async fn test_increment_steps(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {"
            ˇ1
            1
            1  2
            1
            1"})
        .await;

    cx.simulate_shared_keystrokes("j v shift-g g ctrl-a").await;
    cx.shared_state().await.assert_eq(indoc! {"
            1
            ˇ2
            3  2
            4
            5"});

    cx.simulate_shared_keystrokes("shift-g ctrl-v g g").await;
    cx.shared_state().await.assert_eq(indoc! {"
            «1ˇ»
            «2ˇ»
            «3ˇ»  2
            «4ˇ»
            «5ˇ»"});

    cx.simulate_shared_keystrokes("g ctrl-x").await;
    cx.shared_state().await.assert_eq(indoc! {"
            ˇ0
            0
            0  2
            0
            0"});
    cx.simulate_shared_keystrokes("v shift-g g ctrl-a").await;
    cx.simulate_shared_keystrokes("v shift-g 5 g ctrl-a").await;
    cx.shared_state().await.assert_eq(indoc! {"
            ˇ6
            12
            18  2
            24
            30"});
}

#[gpui::test]
async fn test_increment_negative_numbers(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    // vim folds a leading '-' into the number, so ctrl-a on the `05` here
    // operates on `-05` and decrements the visible digits to `04`.
    cx.simulate("ctrl-a", "2025-0ˇ5-10").await.assert_matches();

    // Cursor on or just before a trailing '-' (with or without a following
    // number) must not scan past the '-' into the earlier number.
    cx.simulate("ctrl-a", "2025-05ˇ-").await.assert_matches();
    cx.simulate("ctrl-a", "2025-05ˇ- 345")
        .await
        .assert_matches();
}

#[gpui::test]
async fn test_increment_toggle(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state("let enabled = trˇue;", Mode::Normal);
    cx.simulate_keystrokes("ctrl-a");
    cx.assert_state("let enabled = falsˇe;", Mode::Normal);

    cx.simulate_keystrokes("0 ctrl-a");
    cx.assert_state("let enabled = truˇe;", Mode::Normal);

    cx.set_state(
        indoc! {"
                ˇlet enabled = TRUE;
                let enabled = TRUE;
                let enabled = TRUE;
            "},
        Mode::Normal,
    );
    cx.simulate_keystrokes("shift-v j j ctrl-x");
    cx.assert_state(
        indoc! {"
                ˇlet enabled = FALSE;
                let enabled = FALSE;
                let enabled = FALSE;
            "},
        Mode::Normal,
    );

    cx.set_state(
        indoc! {"
                let enabled = ˇYes;
                let enabled = Yes;
                let enabled = Yes;
            "},
        Mode::Normal,
    );
    cx.simulate_keystrokes("ctrl-v j j e ctrl-x");
    cx.assert_state(
        indoc! {"
                let enabled = ˇNo;
                let enabled = No;
                let enabled = No;
            "},
        Mode::Normal,
    );

    cx.set_state("ˇlet enabled = True;", Mode::Normal);
    cx.simulate_keystrokes("ctrl-a");
    cx.assert_state("let enabled = Falsˇe;", Mode::Normal);

    cx.simulate_keystrokes("ctrl-a");
    cx.assert_state("let enabled = Truˇe;", Mode::Normal);

    cx.set_state("let enabled = Onˇ;", Mode::Normal);
    cx.simulate_keystrokes("v b ctrl-a");
    cx.assert_state("let enabled = ˇOff;", Mode::Normal);
}

#[gpui::test]
async fn test_increment_order(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state("aaˇa false 1 2 3", Mode::Normal);
    cx.simulate_keystrokes("ctrl-a");
    cx.assert_state("aaa truˇe 1 2 3", Mode::Normal);

    cx.set_state("aaˇa 1 false 2 3", Mode::Normal);
    cx.simulate_keystrokes("ctrl-a");
    cx.assert_state("aaa ˇ2 false 2 3", Mode::Normal);

    cx.set_state("trueˇ 1 2 3", Mode::Normal);
    cx.simulate_keystrokes("ctrl-a");
    cx.assert_state("true ˇ2 2 3", Mode::Normal);

    cx.set_state("falseˇ", Mode::Normal);
    cx.simulate_keystrokes("ctrl-a");
    cx.assert_state("truˇe", Mode::Normal);

    cx.set_state("⚡️ˇ⚡️", Mode::Normal);
    cx.simulate_keystrokes("ctrl-a");
    cx.assert_state("⚡️ˇ⚡️", Mode::Normal);
}

#[gpui::test]
async fn test_increment_visual_partial_number(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("ˇ123").await;
    cx.simulate_shared_keystrokes("v l ctrl-a").await;
    cx.shared_state().await.assert_eq(indoc! {"ˇ133"});
    cx.simulate_shared_keystrokes("l v l ctrl-a").await;
    cx.shared_state().await.assert_eq(indoc! {"1ˇ34"});
    cx.simulate_shared_keystrokes("shift-v y p p ctrl-v k k l ctrl-a")
        .await;
    cx.shared_state().await.assert_eq(indoc! {"ˇ144\n144\n144"});
}

#[gpui::test]
async fn test_increment_markdown_list_markers_multiline(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("# Title\nˇ1. item\n2. item\n3. item")
        .await;
    cx.simulate_shared_keystrokes("ctrl-a").await;
    cx.shared_state()
        .await
        .assert_eq("# Title\nˇ2. item\n2. item\n3. item");
    cx.simulate_shared_keystrokes("j").await;
    cx.shared_state()
        .await
        .assert_eq("# Title\n2. item\nˇ2. item\n3. item");
    cx.simulate_shared_keystrokes("ctrl-a").await;
    cx.shared_state()
        .await
        .assert_eq("# Title\n2. item\nˇ3. item\n3. item");
    cx.simulate_shared_keystrokes("ctrl-x").await;
    cx.shared_state()
        .await
        .assert_eq("# Title\n2. item\nˇ2. item\n3. item");
}

#[gpui::test]
async fn test_increment_with_multibyte_characters(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    // Test cursor after a multibyte character - this would panic before the fix
    // because the backward scan would land in the middle of the Korean character
    cx.set_state("지ˇ1", Mode::Normal);
    cx.simulate_keystrokes("ctrl-a");
    cx.assert_state("지ˇ2", Mode::Normal);
}
