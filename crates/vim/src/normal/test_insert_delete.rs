use gpui::{KeyBinding, TestAppContext, UpdateGlobal};
use indoc::indoc;
use settings::SettingsStore;

use crate::{
    motion,
    state::Mode::{self},
    test::{NeovimBackedTestContext, VimTestContext},
};
use language;

#[gpui::test]
async fn test_a(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate_at_each_offset("a", "The qˇuicˇk")
        .await
        .assert_matches();
}

#[gpui::test]
async fn test_insert_end_of_line(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate_at_each_offset(
        "shift-a",
        indoc! {"
        ˇ
        The qˇuick
        brown ˇfox "},
    )
    .await
    .assert_matches();
}

#[gpui::test]
async fn test_jump_to_first_non_whitespace(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate("^", "The qˇuick").await.assert_matches();
    cx.simulate("^", " The qˇuick").await.assert_matches();
    cx.simulate("^", "ˇ").await.assert_matches();
    cx.simulate(
        "^",
        indoc! {"
            The qˇuick
            brown fox"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "^",
        indoc! {"
            ˇ
            The quick"},
    )
    .await
    .assert_matches();
    // Indoc disallows trailing whitespace.
    cx.simulate("^", "   ˇ \nThe quick").await.assert_matches();
}

#[gpui::test]
async fn test_insert_first_non_whitespace(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate("shift-i", "The qˇuick").await.assert_matches();
    cx.simulate("shift-i", " The qˇuick").await.assert_matches();
    cx.simulate("shift-i", "ˇ").await.assert_matches();
    cx.simulate(
        "shift-i",
        indoc! {"
            The qˇuick
            brown fox"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "shift-i",
        indoc! {"
            ˇ
            The quick"},
    )
    .await
    .assert_matches();
}

#[gpui::test]
async fn test_delete_to_end_of_line(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate(
        "shift-d",
        indoc! {"
            The qˇuick
            brown fox"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "shift-d",
        indoc! {"
            The quick
            ˇ
            brown fox"},
    )
    .await
    .assert_matches();
}

#[gpui::test]
async fn test_x(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate_at_each_offset("x", "ˇTeˇsˇt")
        .await
        .assert_matches();
    cx.simulate(
        "x",
        indoc! {"
            Tesˇt
            test"},
    )
    .await
    .assert_matches();
}

#[gpui::test]
async fn test_delete_left(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate_at_each_offset("shift-x", "ˇTˇeˇsˇt")
        .await
        .assert_matches();
    cx.simulate(
        "shift-x",
        indoc! {"
            Test
            ˇtest"},
    )
    .await
    .assert_matches();
}

#[gpui::test]
async fn test_o(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate("o", "ˇ").await.assert_matches();
    cx.simulate("o", "The ˇquick").await.assert_matches();
    cx.simulate_at_each_offset(
        "o",
        indoc! {"
            The qˇuick
            brown ˇfox
            jumps ˇover"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "o",
        indoc! {"
            The quick
            ˇ
            brown fox"},
    )
    .await
    .assert_matches();

    cx.assert_binding(
        "o",
        indoc! {"
            fn test() {
                println!(ˇ);
            }"},
        Mode::Normal,
        indoc! {"
            fn test() {
                println!();
                ˇ
            }"},
        Mode::Insert,
    );

    cx.assert_binding(
        "o",
        indoc! {"
            fn test(ˇ) {
                println!();
            }"},
        Mode::Normal,
        indoc! {"
            fn test() {
                ˇ
                println!();
            }"},
        Mode::Insert,
    );
}

#[gpui::test]
async fn test_insert_line_above(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate("shift-o", "ˇ").await.assert_matches();
    cx.simulate("shift-o", "The ˇquick").await.assert_matches();
    cx.simulate_at_each_offset(
        "shift-o",
        indoc! {"
        The qˇuick
        brown ˇfox
        jumps ˇover"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "shift-o",
        indoc! {"
        The quick
        ˇ
        brown fox"},
    )
    .await
    .assert_matches();

    // Our indentation is smarter than vims. So we don't match here
    cx.assert_binding(
        "shift-o",
        indoc! {"
            fn test() {
                println!(ˇ);
            }"},
        Mode::Normal,
        indoc! {"
            fn test() {
                ˇ
                println!();
            }"},
        Mode::Insert,
    );
    cx.assert_binding(
        "shift-o",
        indoc! {"
            fn test(ˇ) {
                println!();
            }"},
        Mode::Normal,
        indoc! {"
            ˇ
            fn test() {
                println!();
            }"},
        Mode::Insert,
    );
    cx.assert_binding(
        "shift-o",
        indoc! {"
            fn test() {
                println!();
            ˇ}"},
        Mode::Normal,
        indoc! {"
            fn test() {
                println!();
                ˇ
            }"},
        Mode::Insert,
    );
}

#[gpui::test]
async fn test_insert_empty_line(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate("[ space", "ˇ").await.assert_matches();
    cx.simulate("[ space", "The ˇquick").await.assert_matches();
    cx.simulate_at_each_offset(
        "3 [ space",
        indoc! {"
        The qˇuick
        brown ˇfox
        jumps ˇover"},
    )
    .await
    .assert_matches();
    cx.simulate_at_each_offset(
        "[ space",
        indoc! {"
        The qˇuick
        brown ˇfox
        jumps ˇover"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "[ space",
        indoc! {"
        The quick
        ˇ
        brown fox"},
    )
    .await
    .assert_matches();

    cx.simulate("] space", "ˇ").await.assert_matches();
    cx.simulate("] space", "The ˇquick").await.assert_matches();
    cx.simulate_at_each_offset(
        "3 ] space",
        indoc! {"
        The qˇuick
        brown ˇfox
        jumps ˇover"},
    )
    .await
    .assert_matches();
    cx.simulate_at_each_offset(
        "] space",
        indoc! {"
        The qˇuick
        brown ˇfox
        jumps ˇover"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "] space",
        indoc! {"
        The quick
        ˇ
        brown fox"},
    )
    .await
    .assert_matches();
}

#[gpui::test]
async fn test_dd(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate("d d", "ˇ").await.assert_matches();
    cx.simulate("d d", "The ˇquick").await.assert_matches();
    cx.simulate_at_each_offset(
        "d d",
        indoc! {"
        The qˇuick
        brown ˇfox
        jumps ˇover"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "d d",
        indoc! {"
            The quick
            ˇ
            brown fox"},
    )
    .await
    .assert_matches();
}

#[gpui::test]
async fn test_cc(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate("c c", "ˇ").await.assert_matches();
    cx.simulate("c c", "The ˇquick").await.assert_matches();
    cx.simulate_at_each_offset(
        "c c",
        indoc! {"
            The quˇick
            brown ˇfox
            jumps ˇover"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "c c",
        indoc! {"
            The quick
            ˇ
            brown fox"},
    )
    .await
    .assert_matches();
}
