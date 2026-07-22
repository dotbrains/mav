use super::*;

#[gpui::test]
async fn test_tags(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new_html(cx).await;

    cx.set_state("<html><head></head><body><b>hˇi!</b></body>", Mode::Normal);
    cx.simulate_keystrokes("v i t");
    cx.assert_state(
        "<html><head></head><body><b>«hi!ˇ»</b></body>",
        Mode::Visual,
    );
    cx.simulate_keystrokes("a t");
    cx.assert_state(
        "<html><head></head><body>«<b>hi!</b>ˇ»</body>",
        Mode::Visual,
    );
    cx.simulate_keystrokes("a t");
    cx.assert_state(
        "<html><head></head>«<body><b>hi!</b></body>ˇ»",
        Mode::Visual,
    );

    // The cursor is before the tag
    cx.set_state(
        "<html><head></head><body> ˇ  <b>hi!</b></body>",
        Mode::Normal,
    );
    cx.simulate_keystrokes("v i t");
    cx.assert_state(
        "<html><head></head><body>   <b>«hi!ˇ»</b></body>",
        Mode::Visual,
    );
    cx.simulate_keystrokes("a t");
    cx.assert_state(
        "<html><head></head><body>   «<b>hi!</b>ˇ»</body>",
        Mode::Visual,
    );

    // The cursor is in the open tag
    cx.set_state(
        "<html><head></head><body><bˇ>hi!</b><b>hello!</b></body>",
        Mode::Normal,
    );
    cx.simulate_keystrokes("v a t");
    cx.assert_state(
        "<html><head></head><body>«<b>hi!</b>ˇ»<b>hello!</b></body>",
        Mode::Visual,
    );
    cx.simulate_keystrokes("i t");
    cx.assert_state(
        "<html><head></head><body>«<b>hi!</b><b>hello!</b>ˇ»</body>",
        Mode::Visual,
    );

    // current selection length greater than 1
    cx.set_state(
        "<html><head></head><body><«b>hi!ˇ»</b></body>",
        Mode::Visual,
    );
    cx.simulate_keystrokes("i t");
    cx.assert_state(
        "<html><head></head><body><b>«hi!ˇ»</b></body>",
        Mode::Visual,
    );
    cx.simulate_keystrokes("a t");
    cx.assert_state(
        "<html><head></head><body>«<b>hi!</b>ˇ»</body>",
        Mode::Visual,
    );

    cx.set_state(
        "<html><head></head><body><«b>hi!</ˇ»b></body>",
        Mode::Visual,
    );
    cx.simulate_keystrokes("a t");
    cx.assert_state(
        "<html><head></head>«<body><b>hi!</b></body>ˇ»",
        Mode::Visual,
    );
}
#[gpui::test]
async fn test_around_containing_word_indent(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("    ˇconst f = (x: unknown) => {")
        .await;
    cx.simulate_shared_keystrokes("v a w").await;
    cx.shared_state()
        .await
        .assert_eq("    «const ˇ»f = (x: unknown) => {");

    cx.set_shared_state("    ˇconst f = (x: unknown) => {")
        .await;
    cx.simulate_shared_keystrokes("y a w").await;
    cx.shared_clipboard().await.assert_eq("const ");

    cx.set_shared_state("    ˇconst f = (x: unknown) => {")
        .await;
    cx.simulate_shared_keystrokes("d a w").await;
    cx.shared_state()
        .await
        .assert_eq("    ˇf = (x: unknown) => {");
    cx.shared_clipboard().await.assert_eq("const ");

    cx.set_shared_state("    ˇconst f = (x: unknown) => {")
        .await;
    cx.simulate_shared_keystrokes("c a w").await;
    cx.shared_state()
        .await
        .assert_eq("    ˇf = (x: unknown) => {");
    cx.shared_clipboard().await.assert_eq("const ");
}
