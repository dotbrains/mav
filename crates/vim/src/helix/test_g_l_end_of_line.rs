use super::*;

#[gpui::test]
async fn test_g_l_end_of_line(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();

    // Test g l moves to last character, not after it
    cx.set_state("hello ˇworld!", Mode::HelixNormal);
    cx.simulate_keystrokes("g l");
    cx.assert_state("hello worldˇ!", Mode::HelixNormal);

    // Test with Chinese characters, test if work with UTF-8?
    cx.set_state("ˇ你好世界", Mode::HelixNormal);
    cx.simulate_keystrokes("g l");
    cx.assert_state("你好世ˇ界", Mode::HelixNormal);

    // Test with end of line
    cx.set_state("endˇ", Mode::HelixNormal);
    cx.simulate_keystrokes("g l");
    cx.assert_state("enˇd", Mode::HelixNormal);

    // Test with empty line
    cx.set_state(
        indoc! {"
            hello
            ˇ
            world"},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("g l");
    cx.assert_state(
        indoc! {"
            hello
            ˇ
            world"},
        Mode::HelixNormal,
    );

    // Test with multiple lines
    cx.set_state(
        indoc! {"
            ˇfirst line
            second line
            third line"},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("g l");
    cx.assert_state(
        indoc! {"
            first linˇe
            second line
            third line"},
        Mode::HelixNormal,
    );
}
