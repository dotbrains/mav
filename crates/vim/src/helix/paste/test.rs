use indoc::indoc;

use gpui::ClipboardItem;

use crate::{state::Mode, test::VimTestContext};

#[gpui::test]
async fn test_system_clipboard_paste(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();
    cx.set_state(
        indoc! {"
        The quiˇck brown
        fox jumps over
        the lazy dog."},
        Mode::HelixNormal,
    );

    cx.write_to_clipboard(ClipboardItem::new_string("clipboard".to_string()));
    cx.simulate_keystrokes("p");
    cx.assert_state(
        indoc! {"
        The quic«clipboardˇ»k brown
        fox jumps over
        the lazy dog."},
        Mode::HelixNormal,
    );

    // Multiple cursors with system clipboard (no metadata) pastes
    // the same text at each cursor.
    cx.set_state(
        indoc! {"
        ˇThe quick brown
        fox ˇjumps over
        the lazy dog."},
        Mode::HelixNormal,
    );
    cx.write_to_clipboard(ClipboardItem::new_string("hi".to_string()));
    cx.simulate_keystrokes("p");
    cx.assert_state(
        indoc! {"
        T«hiˇ»he quick brown
        fox j«hiˇ»umps over
        the lazy dog."},
        Mode::HelixNormal,
    );

    // Multiple cursors on empty lines should paste on those same lines.
    cx.set_state("ˇ\nˇ\nˇ\nend", Mode::HelixNormal);
    cx.write_to_clipboard(ClipboardItem::new_string("X".to_string()));
    cx.simulate_keystrokes("p");
    cx.assert_state("«Xˇ»\n«Xˇ»\n«Xˇ»\nend", Mode::HelixNormal);
}

#[gpui::test]
async fn test_system_clipboard_crlf_paste_at_end_of_buffer(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();
    cx.set_state("ˇ", Mode::HelixNormal);

    cx.write_to_clipboard(ClipboardItem::new_string("a\r\nb".to_string()));
    cx.simulate_keystrokes("p");

    cx.assert_state("«a\nbˇ»", Mode::HelixNormal);
}

#[gpui::test]
async fn test_read_only_paste(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();
    cx.set_state("aˇb", Mode::HelixNormal);
    cx.write_to_clipboard(ClipboardItem::new_string("clipboard".to_string()));
    cx.update_editor(|editor, _window, _cx| editor.set_read_only(true));

    cx.simulate_keystrokes("p");

    cx.assert_state("aˇb", Mode::HelixNormal);
}

#[gpui::test]
async fn test_paste(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();
    cx.set_state(
        indoc! {"
        The «quiˇ»ck brown
        fox jumps over
        the lazy dog."},
        Mode::HelixNormal,
    );

    cx.simulate_keystrokes("y w p");

    cx.assert_state(
        indoc! {"
        The quick «quiˇ»brown
        fox jumps over
        the lazy dog."},
        Mode::HelixNormal,
    );

    // Pasting before the selection:
    cx.set_state(
        indoc! {"
        The quick brown
        fox «jumpsˇ» over
        the lazy dog."},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("shift-p");
    cx.assert_state(
        indoc! {"
        The quick brown
        fox «quiˇ»jumps over
        the lazy dog."},
        Mode::HelixNormal,
    );
}

#[gpui::test]
async fn test_point_selection_paste(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();
    cx.set_state(
        indoc! {"
        The quiˇck brown
        fox jumps over
        the lazy dog."},
        Mode::HelixNormal,
    );

    cx.simulate_keystrokes("y");

    // Pasting before the selection:
    cx.set_state(
        indoc! {"
        The quick brown
        fox jumpsˇ over
        the lazy dog."},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("shift-p");
    cx.assert_state(
        indoc! {"
        The quick brown
        fox jumps«cˇ» over
        the lazy dog."},
        Mode::HelixNormal,
    );

    // Pasting after the selection:
    cx.set_state(
        indoc! {"
        The quick brown
        fox jumpsˇ over
        the lazy dog."},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("p");
    cx.assert_state(
        indoc! {"
        The quick brown
        fox jumps «cˇ»over
        the lazy dog."},
        Mode::HelixNormal,
    );

    // Pasting after the selection at the end of a line:
    cx.set_state(
        indoc! {"
        The quick brown
        fox jumps overˇ
        the lazy dog."},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("p");
    cx.assert_state(
        indoc! {"
        The quick brown
        fox jumps over
        «cˇ»the lazy dog."},
        Mode::HelixNormal,
    );
}

#[gpui::test]
async fn test_multi_cursor_paste(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();
    // Select two blocks of text.
    cx.set_state(
        indoc! {"
        The «quiˇ»ck brown
        fox ju«mpsˇ» over
        the lazy dog."},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("y");

    // Only one cursor: only the first block gets pasted.
    cx.set_state(
        indoc! {"
        ˇThe quick brown
        fox jumps over
        the lazy dog."},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("shift-p");
    cx.assert_state(
        indoc! {"
        «quiˇ»The quick brown
        fox jumps over
        the lazy dog."},
        Mode::HelixNormal,
    );

    // Two cursors: both get pasted.
    cx.set_state(
        indoc! {"
        ˇThe ˇquick brown
        fox jumps over
        the lazy dog."},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("shift-p");
    cx.assert_state(
        indoc! {"
        «quiˇ»The «mpsˇ»quick brown
        fox jumps over
        the lazy dog."},
        Mode::HelixNormal,
    );

    // Three cursors: the second yanked block is duplicated.
    cx.set_state(
        indoc! {"
        ˇThe ˇquick brown
        fox jumpsˇ over
        the lazy dog."},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("shift-p");
    cx.assert_state(
        indoc! {"
        «quiˇ»The «mpsˇ»quick brown
        fox jumps«mpsˇ» over
        the lazy dog."},
        Mode::HelixNormal,
    );

    // Again with three cursors. All three should be pasted twice.
    cx.set_state(
        indoc! {"
        ˇThe ˇquick brown
        fox jumpsˇ over
        the lazy dog."},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("2 shift-p");
    cx.assert_state(
        indoc! {"
        «quiquiˇ»The «mpsmpsˇ»quick brown
        fox jumps«mpsmpsˇ» over
        the lazy dog."},
        Mode::HelixNormal,
    );
}

#[gpui::test]
async fn test_line_mode_paste(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();
    cx.set_state(
        indoc! {"
        The quick brow«n
        ˇ»fox jumps over
        the lazy dog."},
        Mode::HelixNormal,
    );

    cx.simulate_keystrokes("y shift-p");

    cx.assert_state(
        indoc! {"
        «n
        ˇ»The quick brown
        fox jumps over
        the lazy dog."},
        Mode::HelixNormal,
    );

    // In line mode, if we're in the middle of a line then pasting before pastes on
    // the line before.
    cx.set_state(
        indoc! {"
        The quick brown
        fox jumpsˇ over
        the lazy dog."},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("shift-p");
    cx.assert_state(
        indoc! {"
        The quick brown
        «n
        ˇ»fox jumps over
        the lazy dog."},
        Mode::HelixNormal,
    );

    // In line mode, if we're in the middle of a line then pasting after pastes on
    // the line after.
    cx.set_state(
        indoc! {"
        The quick brown
        fox jumpsˇ over
        the lazy dog."},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("p");
    cx.assert_state(
        indoc! {"
        The quick brown
        fox jumps over
        «n
        ˇ»the lazy dog."},
        Mode::HelixNormal,
    );

    // If we're currently at the end of a line, "the line after"
    // means right after the cursor.
    cx.set_state(
        indoc! {"
        The quick brown
        fox jumps overˇ
        the lazy dog."},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("p");
    cx.assert_state(
        indoc! {"
        The quick brown
        fox jumps over
        «n
        ˇ»the lazy dog."},
        Mode::HelixNormal,
    );

    cx.set_state(
        indoc! {"

        The quick brown
        fox jumps overˇ
        the lazy dog."},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("x y up up p");
    cx.assert_state(
        indoc! {"

        «fox jumps over
        ˇ»The quick brown
        fox jumps over
        the lazy dog."},
        Mode::HelixNormal,
    );

    cx.set_state(
        indoc! {"
        «The quick brown
        fox jumps over
        ˇ»the lazy dog."},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("y p p");
    cx.assert_state(
        indoc! {"
        The quick brown
        fox jumps over
        The quick brown
        fox jumps over
        «The quick brown
        fox jumps over
        ˇ»the lazy dog."},
        Mode::HelixNormal,
    );
}
