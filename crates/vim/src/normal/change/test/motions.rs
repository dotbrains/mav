use indoc::indoc;

use crate::state::Mode;
use crate::test::{NeovimBackedTestContext, VimTestContext};
#[gpui::test]
async fn test_change_h(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate("c h", "Teˇst").await.assert_matches();
    cx.simulate("c h", "Tˇest").await.assert_matches();
    cx.simulate("c h", "ˇTest").await.assert_matches();
    cx.simulate(
        "c h",
        indoc! {"
            Test
            ˇtest"},
    )
    .await
    .assert_matches();
}

#[gpui::test]
async fn test_change_backspace(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate("c backspace", "Teˇst").await.assert_matches();
    cx.simulate("c backspace", "Tˇest").await.assert_matches();
    cx.simulate("c backspace", "ˇTest").await.assert_matches();
    cx.simulate(
        "c backspace",
        indoc! {"
            Test
            ˇtest"},
    )
    .await
    .assert_matches();
}

#[gpui::test]
async fn test_change_l(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate("c l", "Teˇst").await.assert_matches();
    cx.simulate("c l", "Tesˇt").await.assert_matches();
}

#[gpui::test]
async fn test_change_w(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate("c w", "Teˇst").await.assert_matches();
    cx.simulate("c w", "Tˇest test").await.assert_matches();
    cx.simulate("c w", "Testˇ  test").await.assert_matches();
    cx.simulate("c w", "Tesˇt  test").await.assert_matches();
    cx.simulate(
        "c w",
        indoc! {"
                Test teˇst
                test"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "c w",
        indoc! {"
                Test tesˇt
                test"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "c w",
        indoc! {"
                Test test
                ˇ
                test"},
    )
    .await
    .assert_matches();

    cx.simulate("c shift-w", "Test teˇst-test test")
        .await
        .assert_matches();

    // on last character of word, `cw` doesn't eat subsequent punctuation
    // see https://github.com/mav-industries/mav/issues/35269
    cx.simulate("c w", "tesˇt-test").await.assert_matches();

    cx.simulate("c 2 w", "ˇTest test test")
        .await
        .assert_matches();
    cx.simulate("c 2 w", "Tˇest test test")
        .await
        .assert_matches();
    cx.simulate("c 2 w", "tesˇt-test").await.assert_matches();

    cx.simulate("c 2 shift-w", "Test teˇst-test test Test")
        .await
        .assert_matches();
}

#[gpui::test]
async fn test_change_e(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate("c e", "Teˇst Test").await.assert_matches();
    cx.simulate("c e", "Tˇest test").await.assert_matches();
    cx.simulate(
        "c e",
        indoc! {"
                Test teˇst
                test"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "c e",
        indoc! {"
                Test tesˇt
                test"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "c e",
        indoc! {"
                Test test
                ˇ
                test"},
    )
    .await
    .assert_matches();

    cx.simulate("c shift-e", "Test teˇst-test test")
        .await
        .assert_matches();
}

#[gpui::test]
async fn test_change_b(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate("c b", "Teˇst Test").await.assert_matches();
    cx.simulate("c b", "Test ˇtest").await.assert_matches();
    cx.simulate("c b", "Test1 test2 ˇtest3")
        .await
        .assert_matches();
    cx.simulate(
        "c b",
        indoc! {"
                Test test
                ˇtest"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "c b",
        indoc! {"
                Test test
                ˇ
                test"},
    )
    .await
    .assert_matches();

    cx.simulate("c shift-b", "Test test-test ˇtest")
        .await
        .assert_matches();
}

#[gpui::test]
async fn test_change_end_of_line(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate(
        "c $",
        indoc! {"
            The qˇuick
            brown fox"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "c $",
        indoc! {"
            The quick
            ˇ
            brown fox"},
    )
    .await
    .assert_matches();
}

#[gpui::test]
async fn test_change_0(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.simulate(
        "c 0",
        indoc! {"
            The qˇuick
            brown fox"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "c 0",
        indoc! {"
            The quick
            ˇ
            brown fox"},
    )
    .await
    .assert_matches();
}

#[gpui::test]
async fn test_change_k(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.simulate(
        "c k",
        indoc! {"
            The quick
            brown ˇfox
            jumps over"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "c k",
        indoc! {"
            The quick
            brown fox
            jumps ˇover"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "c k",
        indoc! {"
            The qˇuick
            brown fox
            jumps over"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "c k",
        indoc! {"
            ˇ
            brown fox
            jumps over"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "c k",
        indoc! {"
            The quick
              brown fox
              ˇjumps over"},
    )
    .await
    .assert_matches();
}

#[gpui::test]
async fn test_change_j(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate(
        "c j",
        indoc! {"
            The quick
            brown ˇfox
            jumps over"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "c j",
        indoc! {"
            The quick
            brown fox
            jumps ˇover"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "c j",
        indoc! {"
            The qˇuick
            brown fox
            jumps over"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "c j",
        indoc! {"
            The quick
            brown fox
            ˇ"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "c j",
        indoc! {"
            The quick
              ˇbrown fox
              jumps over"},
    )
    .await
    .assert_matches();
}

#[gpui::test]
async fn test_change_end_of_document(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate(
        "c shift-g",
        indoc! {"
            The quick
            brownˇ fox
            jumps over
            the lazy"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "c shift-g",
        indoc! {"
            The quick
            brownˇ fox
            jumps over
            the lazy"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "c shift-g",
        indoc! {"
            The quick
            brown fox
            jumps over
            the lˇazy"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "c shift-g",
        indoc! {"
            The quick
            brown fox
            jumps over
            ˇ"},
    )
    .await
    .assert_matches();
}
