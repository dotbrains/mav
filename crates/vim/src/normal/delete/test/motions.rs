use indoc::indoc;

use crate::{
    state::Mode,
    test::{NeovimBackedTestContext, VimTestContext},
};
#[gpui::test]
async fn test_delete_h(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate("d h", "Teˇst").await.assert_matches();
    cx.simulate("d h", "Tˇest").await.assert_matches();
    cx.simulate("d h", "ˇTest").await.assert_matches();
    cx.simulate(
        "d h",
        indoc! {"
            Test
            ˇtest"},
    )
    .await
    .assert_matches();
}

#[gpui::test]
async fn test_delete_l(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate("d l", "ˇTest").await.assert_matches();
    cx.simulate("d l", "Teˇst").await.assert_matches();
    cx.simulate("d l", "Tesˇt").await.assert_matches();
    cx.simulate(
        "d l",
        indoc! {"
                Tesˇt
                test"},
    )
    .await
    .assert_matches();
}

#[gpui::test]
async fn test_delete_w(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate(
        "d w",
        indoc! {"
            Test tesˇt
                test"},
    )
    .await
    .assert_matches();

    cx.simulate("d w", "Teˇst").await.assert_matches();
    cx.simulate("d w", "Tˇest test").await.assert_matches();
    cx.simulate(
        "d w",
        indoc! {"
            Test teˇst
            test"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "d w",
        indoc! {"
            Test tesˇt
            test"},
    )
    .await
    .assert_matches();

    cx.simulate(
        "d w",
        indoc! {"
            Test test
            ˇ
            test"},
    )
    .await
    .assert_matches();

    cx.simulate("d shift-w", "Test teˇst-test test")
        .await
        .assert_matches();
}

#[gpui::test]
async fn test_delete_next_word_end(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate("d e", "Teˇst Test\n").await.assert_matches();
    cx.simulate("d e", "Tˇest test\n").await.assert_matches();
    cx.simulate(
        "d e",
        indoc! {"
            Test teˇst
            test"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "d e",
        indoc! {"
            Test tesˇt
            test"},
    )
    .await
    .assert_matches();

    cx.simulate("d e", "Test teˇst-test test")
        .await
        .assert_matches();
}

#[gpui::test]
async fn test_delete_b(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate("d b", "Teˇst Test").await.assert_matches();
    cx.simulate("d b", "Test ˇtest").await.assert_matches();
    cx.simulate("d b", "Test1 test2 ˇtest3")
        .await
        .assert_matches();
    cx.simulate(
        "d b",
        indoc! {"
            Test test
            ˇtest"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "d b",
        indoc! {"
            Test test
            ˇ
            test"},
    )
    .await
    .assert_matches();

    cx.simulate("d shift-b", "Test test-test ˇtest")
        .await
        .assert_matches();
}

#[gpui::test]
async fn test_delete_end_of_line(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate(
        "d $",
        indoc! {"
            The qˇuick
            brown fox"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "d $",
        indoc! {"
            The quick
            ˇ
            brown fox"},
    )
    .await
    .assert_matches();
}

#[gpui::test]
async fn test_delete_end_of_paragraph(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate(
        "d }",
        indoc! {"
            ˇhello world.

            hello world."},
    )
    .await
    .assert_matches();

    cx.simulate(
        "d }",
        indoc! {"
            ˇhello world.
            hello world."},
    )
    .await
    .assert_matches();
}

#[gpui::test]
async fn test_delete_0(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate(
        "d 0",
        indoc! {"
            The qˇuick
            brown fox"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "d 0",
        indoc! {"
            The quick
            ˇ
            brown fox"},
    )
    .await
    .assert_matches();
}

#[gpui::test]
async fn test_delete_k(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate(
        "d k",
        indoc! {"
            The quick
            brown ˇfox
            jumps over"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "d k",
        indoc! {"
            The quick
            brown fox
            jumps ˇover"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "d k",
        indoc! {"
            The qˇuick
            brown fox
            jumps over"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "d k",
        indoc! {"
            ˇbrown fox
            jumps over"},
    )
    .await
    .assert_matches();
}

#[gpui::test]
async fn test_delete_j(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate(
        "d j",
        indoc! {"
            The quick
            brown ˇfox
            jumps over"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "d j",
        indoc! {"
            The quick
            brown fox
            jumps ˇover"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "d j",
        indoc! {"
            The qˇuick
            brown fox
            jumps over"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "d j",
        indoc! {"
            The quick
            brown fox
            ˇ"},
    )
    .await
    .assert_matches();
}

#[gpui::test]
async fn test_delete_end_of_document(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate(
        "d shift-g",
        indoc! {"
            The quick
            brownˇ fox
            jumps over
            the lazy"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "d shift-g",
        indoc! {"
            The quick
            brownˇ fox
            jumps over
            the lazy"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "d shift-g",
        indoc! {"
            The quick
            brown fox
            jumps over
            the lˇazy"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "d shift-g",
        indoc! {"
            The quick
            brown fox
            jumps over
            ˇ"},
    )
    .await
    .assert_matches();
}
