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
async fn test_h(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate_at_each_offset(
        "h",
        indoc! {"
        ˇThe qˇuick
        ˇbrown"
        },
    )
    .await
    .assert_matches();
}

#[gpui::test]
async fn test_backspace(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate_at_each_offset(
        "backspace",
        indoc! {"
        ˇThe qˇuick
        ˇbrown"
        },
    )
    .await
    .assert_matches();
}

#[gpui::test]
async fn test_j(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {"
        aaˇaa
        😃😃"
    })
    .await;
    cx.simulate_shared_keystrokes("j").await;
    cx.shared_state().await.assert_eq(indoc! {"
        aaaa
        😃ˇ😃"
    });

    cx.simulate_at_each_offset(
        "j",
        indoc! {"
            ˇThe qˇuick broˇwn
            ˇfox jumps"
        },
    )
    .await
    .assert_matches();
}

#[gpui::test]
async fn test_enter(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate_at_each_offset(
        "enter",
        indoc! {"
        ˇThe qˇuick broˇwn
        ˇfox jumps"
        },
    )
    .await
    .assert_matches();
}

#[gpui::test]
async fn test_k(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate_at_each_offset(
        "k",
        indoc! {"
        ˇThe qˇuick
        ˇbrown fˇox jumˇps"
        },
    )
    .await
    .assert_matches();
}

#[gpui::test]
async fn test_l(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate_at_each_offset(
        "l",
        indoc! {"
        ˇThe qˇuicˇk
        ˇbrowˇn"},
    )
    .await
    .assert_matches();
}

#[gpui::test]
async fn test_jump_to_line_boundaries(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate_at_each_offset(
        "$",
        indoc! {"
        ˇThe qˇuicˇk
        ˇbrowˇn"},
    )
    .await
    .assert_matches();
    cx.simulate_at_each_offset(
        "0",
        indoc! {"
            ˇThe qˇuicˇk
            ˇbrowˇn"},
    )
    .await
    .assert_matches();
}

#[gpui::test]
async fn test_jump_to_end(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.simulate_at_each_offset(
        "shift-g",
        indoc! {"
            The ˇquick

            brown fox jumps
            overˇ the lazy doˇg"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "shift-g",
        indoc! {"
        The quiˇck

        brown"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "shift-g",
        indoc! {"
        The quiˇck

        "},
    )
    .await
    .assert_matches();
}

#[gpui::test]
async fn test_w(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate_at_each_offset(
        "w",
        indoc! {"
        The ˇquickˇ-ˇbrown
        ˇ
        ˇ
        ˇfox_jumps ˇover
        ˇthˇe"},
    )
    .await
    .assert_matches();
    cx.simulate_at_each_offset(
        "shift-w",
        indoc! {"
        The ˇquickˇ-ˇbrown
        ˇ
        ˇ
        ˇfox_jumps ˇover
        ˇthˇe"},
    )
    .await
    .assert_matches();
}

#[gpui::test]
async fn test_end_of_word(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate_at_each_offset(
        "e",
        indoc! {"
        Thˇe quicˇkˇ-browˇn


        fox_jumpˇs oveˇr
        thˇe"},
    )
    .await
    .assert_matches();
    cx.simulate_at_each_offset(
        "shift-e",
        indoc! {"
        Thˇe quicˇkˇ-browˇn


        fox_jumpˇs oveˇr
        thˇe"},
    )
    .await
    .assert_matches();
}

#[gpui::test]
async fn test_b(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate_at_each_offset(
        "b",
        indoc! {"
        ˇThe ˇquickˇ-ˇbrown
        ˇ
        ˇ
        ˇfox_jumps ˇover
        ˇthe"},
    )
    .await
    .assert_matches();
    cx.simulate_at_each_offset(
        "shift-b",
        indoc! {"
        ˇThe ˇquickˇ-ˇbrown
        ˇ
        ˇ
        ˇfox_jumps ˇover
        ˇthe"},
    )
    .await
    .assert_matches();
}

#[gpui::test]
async fn test_gg(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate_at_each_offset(
        "g g",
        indoc! {"
            The qˇuick

            brown fox jumps
            over ˇthe laˇzy dog"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "g g",
        indoc! {"


            brown fox jumps
            over the laˇzy dog"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "2 g g",
        indoc! {"
            ˇ

            brown fox jumps
            over the lazydog"},
    )
    .await
    .assert_matches();
}

#[gpui::test]
async fn test_end_of_document(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate_at_each_offset(
        "shift-g",
        indoc! {"
            The qˇuick

            brown fox jumps
            over ˇthe laˇzy dog"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "shift-g",
        indoc! {"


            brown fox jumps
            over the laˇzy dog"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "2 shift-g",
        indoc! {"
            ˇ

            brown fox jumps
            over the lazydog"},
    )
    .await
    .assert_matches();
}
