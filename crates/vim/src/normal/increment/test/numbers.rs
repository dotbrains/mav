
use indoc::indoc;

use crate::{
    state::Mode,
    test::{NeovimBackedTestContext, VimTestContext},
};
#[gpui::test]
async fn test_increment(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {"
            1ˇ2
            "})
        .await;

    cx.simulate_shared_keystrokes("ctrl-a").await;
    cx.shared_state().await.assert_eq(indoc! {"
            1ˇ3
            "});
    cx.simulate_shared_keystrokes("ctrl-x").await;
    cx.shared_state().await.assert_eq(indoc! {"
            1ˇ2
            "});

    cx.simulate_shared_keystrokes("9 9 ctrl-a").await;
    cx.shared_state().await.assert_eq(indoc! {"
            11ˇ1
            "});
    cx.simulate_shared_keystrokes("1 1 1 ctrl-x").await;
    cx.shared_state().await.assert_eq(indoc! {"
            ˇ0
            "});
    cx.simulate_shared_keystrokes(".").await;
    cx.shared_state().await.assert_eq(indoc! {"
            -11ˇ1
            "});
}

#[gpui::test]
async fn test_increment_with_dot(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {"
            1ˇ.2
            "})
        .await;

    cx.simulate_shared_keystrokes("ctrl-a").await;
    cx.shared_state().await.assert_eq(indoc! {"
            1.ˇ3
            "});
    cx.simulate_shared_keystrokes("ctrl-x").await;
    cx.shared_state().await.assert_eq(indoc! {"
            1.ˇ2
            "});

    // '.' is a separator, not a decimal point, so the number the cursor is
    // on is incremented even without surrounding whitespace.
    cx.simulate("ctrl-a", "0.8ˇ1.46").await.assert_matches();
    cx.simulate("ctrl-x", "0.8ˇ1.46").await.assert_matches();
}

#[gpui::test]
async fn test_increment_with_leading_zeros(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {"
            000ˇ9
            "})
        .await;

    cx.simulate_shared_keystrokes("ctrl-a").await;
    cx.shared_state().await.assert_eq(indoc! {"
            001ˇ0
            "});
    cx.simulate_shared_keystrokes("2 ctrl-x").await;
    cx.shared_state().await.assert_eq(indoc! {"
            000ˇ8
            "});
}

#[gpui::test]
async fn test_increment_with_leading_zeros_and_zero(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {"
            01ˇ1
            "})
        .await;

    cx.simulate_shared_keystrokes("ctrl-a").await;
    cx.shared_state().await.assert_eq(indoc! {"
            01ˇ2
            "});
    cx.simulate_shared_keystrokes("1 2 ctrl-x").await;
    cx.shared_state().await.assert_eq(indoc! {"
            00ˇ0
            "});
}

#[gpui::test]
async fn test_increment_with_changing_leading_zeros(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {"
            099ˇ9
            "})
        .await;

    cx.simulate_shared_keystrokes("ctrl-a").await;
    cx.shared_state().await.assert_eq(indoc! {"
            100ˇ0
            "});
    cx.simulate_shared_keystrokes("2 ctrl-x").await;
    cx.shared_state().await.assert_eq(indoc! {"
            99ˇ8
            "});
}

#[gpui::test]
async fn test_increment_with_two_dots(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {"
            111.ˇ.2
            "})
        .await;

    cx.simulate_shared_keystrokes("ctrl-a").await;
    cx.shared_state().await.assert_eq(indoc! {"
            111..ˇ3
            "});
    cx.simulate_shared_keystrokes("ctrl-x").await;
    cx.shared_state().await.assert_eq(indoc! {"
            111..ˇ2
            "});
}

#[gpui::test]
async fn test_increment_sign_change(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.set_shared_state(indoc! {"
                ˇ0
                "})
        .await;
    cx.simulate_shared_keystrokes("ctrl-x").await;
    cx.shared_state().await.assert_eq(indoc! {"
                -ˇ1
                "});
    cx.simulate_shared_keystrokes("2 ctrl-a").await;
    cx.shared_state().await.assert_eq(indoc! {"
                ˇ1
                "});
}

#[gpui::test]
async fn test_increment_sign_change_with_leading_zeros(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.set_shared_state(indoc! {"
                00ˇ1
                "})
        .await;
    cx.simulate_shared_keystrokes("ctrl-x").await;
    cx.shared_state().await.assert_eq(indoc! {"
                00ˇ0
                "});
    cx.simulate_shared_keystrokes("ctrl-x").await;
    cx.shared_state().await.assert_eq(indoc! {"
                -00ˇ1
                "});
    cx.simulate_shared_keystrokes("2 ctrl-a").await;
    cx.shared_state().await.assert_eq(indoc! {"
                00ˇ1
                "});
}

#[gpui::test]
async fn test_increment_bin_wrapping_and_padding(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.set_shared_state(indoc! {"
                    0b111111111111111111111111111111111111111111111111111111111111111111111ˇ1
                    "})
        .await;

    cx.simulate_shared_keystrokes("ctrl-a").await;
    cx.shared_state().await.assert_eq(indoc! {"
                    0b000000111111111111111111111111111111111111111111111111111111111111111ˇ1
                    "});
    cx.simulate_shared_keystrokes("ctrl-a").await;
    cx.shared_state().await.assert_eq(indoc! {"
                    0b000000000000000000000000000000000000000000000000000000000000000000000ˇ0
                    "});

    cx.simulate_shared_keystrokes("ctrl-a").await;
    cx.shared_state().await.assert_eq(indoc! {"
                    0b000000000000000000000000000000000000000000000000000000000000000000000ˇ1
                    "});
    cx.simulate_shared_keystrokes("2 ctrl-x").await;
    cx.shared_state().await.assert_eq(indoc! {"
                    0b000000111111111111111111111111111111111111111111111111111111111111111ˇ1
                    "});
}

#[gpui::test]
async fn test_increment_hex_wrapping_and_padding(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.set_shared_state(indoc! {"
                    0xfffffffffffffffffffˇf
                    "})
        .await;

    cx.simulate_shared_keystrokes("ctrl-a").await;
    cx.shared_state().await.assert_eq(indoc! {"
                    0x0000fffffffffffffffˇf
                    "});
    cx.simulate_shared_keystrokes("ctrl-a").await;
    cx.shared_state().await.assert_eq(indoc! {"
                    0x0000000000000000000ˇ0
                    "});
    cx.simulate_shared_keystrokes("ctrl-a").await;
    cx.shared_state().await.assert_eq(indoc! {"
                    0x0000000000000000000ˇ1
                    "});
    cx.simulate_shared_keystrokes("2 ctrl-x").await;
    cx.shared_state().await.assert_eq(indoc! {"
                    0x0000fffffffffffffffˇf
                    "});
}

#[gpui::test]
async fn test_increment_wrapping(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.set_shared_state(indoc! {"
                    1844674407370955161ˇ9
                    "})
        .await;

    cx.simulate_shared_keystrokes("ctrl-a").await;
    cx.shared_state().await.assert_eq(indoc! {"
                    1844674407370955161ˇ5
                    "});
    cx.simulate_shared_keystrokes("ctrl-a").await;
    cx.shared_state().await.assert_eq(indoc! {"
                    -1844674407370955161ˇ5
                    "});
    cx.simulate_shared_keystrokes("ctrl-a").await;
    cx.shared_state().await.assert_eq(indoc! {"
                    -1844674407370955161ˇ4
                    "});
    cx.simulate_shared_keystrokes("3 ctrl-x").await;
    cx.shared_state().await.assert_eq(indoc! {"
                    1844674407370955161ˇ4
                    "});
    cx.simulate_shared_keystrokes("2 ctrl-a").await;
    cx.shared_state().await.assert_eq(indoc! {"
                    -1844674407370955161ˇ5
                    "});
}

#[gpui::test]
async fn test_increment_inline(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.set_shared_state(indoc! {"
                    inline0x3ˇ9u32
                    "})
        .await;

    cx.simulate_shared_keystrokes("ctrl-a").await;
    cx.shared_state().await.assert_eq(indoc! {"
                    inline0x3ˇau32
                    "});
    cx.simulate_shared_keystrokes("ctrl-a").await;
    cx.shared_state().await.assert_eq(indoc! {"
                    inline0x3ˇbu32
                    "});
    cx.simulate_shared_keystrokes("l l l ctrl-a").await;
    cx.shared_state().await.assert_eq(indoc! {"
                    inline0x3bu3ˇ3
                    "});
}

#[gpui::test]
async fn test_increment_hex_casing(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.set_shared_state(indoc! {"
                        0xFˇa
                    "})
        .await;

    cx.simulate_shared_keystrokes("ctrl-a").await;
    cx.shared_state().await.assert_eq(indoc! {"
                    0xfˇb
                    "});
    cx.simulate_shared_keystrokes("ctrl-a").await;
    cx.shared_state().await.assert_eq(indoc! {"
                    0xfˇc
                    "});
}

#[gpui::test]
async fn test_increment_radix(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.simulate("ctrl-a", "ˇ total: 0xff")
        .await
        .assert_matches();
    cx.simulate("ctrl-x", "ˇ total: 0xff")
        .await
        .assert_matches();
    cx.simulate("ctrl-x", "ˇ total: 0xFF")
        .await
        .assert_matches();
    cx.simulate("ctrl-a", "(ˇ0b10f)").await.assert_matches();
    cx.simulate("ctrl-a", "ˇ-1").await.assert_matches();
    cx.simulate("ctrl-a", "-ˇ1").await.assert_matches();
    cx.simulate("ctrl-a", "banˇana").await.assert_matches();
}
