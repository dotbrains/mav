use crate::{
    state::Mode,
    test::{NeovimBackedTestContext, VimTestContext},
};
use editor::ScrollBeyondLastLine;
use gpui::{AppContext as _, point, px, size};
use indoc::indoc;
use language::Point;
use settings::SettingsStore;

pub fn sample_text(rows: usize, cols: usize, start_char: char) -> String {
    let mut text = String::new();
    for row in 0..rows {
        let c: char = (start_char as u32 + row as u32) as u8 as char;
        let mut line = c.to_string().repeat(cols);
        if row < rows - 1 {
            line.push('\n');
        }
        text += &line;
    }
    text
}

#[gpui::test]
async fn test_scroll(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    let (line_height, visible_line_count) = cx.update_editor(|editor, window, cx| {
        (
            editor
                .style(cx)
                .text
                .line_height_in_pixels(window.rem_size()),
            editor.visible_line_count().unwrap(),
        )
    });

    let window = cx.window;
    let margin = cx
        .update_window(window, |_, window, _cx| {
            window.viewport_size().height - line_height * visible_line_count as f32
        })
        .unwrap();
    cx.simulate_window_resize(
        cx.window,
        size(px(1000.), margin + 8. * line_height - px(1.0)),
    );

    cx.set_state(
        indoc!(
            "ˇone
            two
            three
            four
            five
            six
            seven
            eight
            nine
            ten
            eleven
            twelve
        "
        ),
        Mode::Normal,
    );

    cx.update_editor(|editor, window, cx| {
        assert_eq!(editor.snapshot(window, cx).scroll_position(), point(0., 0.))
    });
    cx.simulate_keystrokes("ctrl-e");
    cx.update_editor(|editor, window, cx| {
        assert_eq!(editor.snapshot(window, cx).scroll_position(), point(0., 1.))
    });
    cx.simulate_keystrokes("2 ctrl-e");
    cx.update_editor(|editor, window, cx| {
        assert_eq!(editor.snapshot(window, cx).scroll_position(), point(0., 3.))
    });
    cx.simulate_keystrokes("ctrl-y");
    cx.update_editor(|editor, window, cx| {
        assert_eq!(editor.snapshot(window, cx).scroll_position(), point(0., 2.))
    });

    cx.simulate_keystrokes("g g");
    cx.update_editor(|editor, window, cx| {
        assert_eq!(editor.snapshot(window, cx).scroll_position(), point(0., 0.))
    });
    cx.simulate_keystrokes("ctrl-d");
    cx.update_editor(|editor, window, cx| {
        assert_eq!(
            editor.snapshot(window, cx).scroll_position(),
            point(0., 3.0)
        );
        assert_eq!(
            editor
                .selections
                .newest(&editor.display_snapshot(cx))
                .range(),
            Point::new(6, 0)..Point::new(6, 0)
        )
    });

    cx.simulate_keystrokes("g g");
    cx.update_editor(|editor, window, cx| {
        assert_eq!(editor.snapshot(window, cx).scroll_position(), point(0., 0.))
    });
    cx.simulate_keystrokes("v ctrl-d");
    cx.update_editor(|editor, window, cx| {
        assert_eq!(
            editor.snapshot(window, cx).scroll_position(),
            point(0., 3.0)
        );
        assert_eq!(
            editor
                .selections
                .newest(&editor.display_snapshot(cx))
                .range(),
            Point::new(0, 0)..Point::new(6, 1)
        )
    });
}

#[gpui::test]
async fn test_ctrl_d_u(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_scroll_height(10).await;

    let content = "ˇ".to_owned() + &sample_text(26, 2, 'a');
    cx.set_shared_state(&content).await;

    cx.simulate_shared_keystrokes("4 j ctrl-d").await;
    cx.shared_state().await.assert_matches();
    cx.simulate_shared_keystrokes("ctrl-d").await;
    cx.shared_state().await.assert_matches();
    cx.simulate_shared_keystrokes("g g ctrl-d").await;
    cx.shared_state().await.assert_matches();

    cx.simulate_shared_keystrokes("ctrl-u").await;
    cx.shared_state().await.assert_matches();
    cx.simulate_shared_keystrokes("ctrl-d ctrl-d 4 j ctrl-u ctrl-u")
        .await;
    cx.shared_state().await.assert_matches();

    cx.simulate_shared_keystrokes("g g ctrl-d ctrl-u ctrl-u")
        .await;
    cx.shared_state().await.assert_matches();
}

#[gpui::test]
async fn test_ctrl_f_b(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    let visible_lines = 10;
    cx.set_scroll_height(visible_lines).await;

    cx.neovim.set_option(&format!("scrolloff={}", 0)).await;
    cx.update_global(|store: &mut SettingsStore, cx| {
        store.update_user_settings(cx, |s| s.editor.vertical_scroll_margin = Some(0.0));
    });

    let content = "ˇ".to_owned() + &sample_text(26, 2, 'a');
    cx.set_shared_state(&content).await;

    cx.simulate_shared_keystrokes("ctrl-f").await;
    cx.shared_state().await.assert_matches();

    cx.simulate_shared_keystrokes("ctrl-f").await;
    cx.shared_state().await.assert_matches();

    cx.simulate_shared_keystrokes("ctrl-b").await;
    cx.shared_state().await.assert_matches();

    cx.simulate_shared_keystrokes("ctrl-b").await;
    cx.shared_state().await.assert_matches();

    cx.simulate_shared_keystrokes("g g").await;
    cx.shared_state().await.assert_matches();

    cx.neovim.set_option(&format!("scrolloff={}", 3)).await;
    cx.update_global(|store: &mut SettingsStore, cx| {
        store.update_user_settings(cx, |s| s.editor.vertical_scroll_margin = Some(3.0));
    });

    cx.simulate_shared_keystrokes("ctrl-f").await;
    cx.shared_state().await.assert_matches();

    cx.simulate_shared_keystrokes("ctrl-f").await;
    cx.shared_state().await.assert_matches();

    cx.simulate_shared_keystrokes("ctrl-b").await;
    cx.shared_state().await.assert_matches();

    cx.simulate_shared_keystrokes("ctrl-b").await;
    cx.shared_state().await.assert_matches();
}

#[gpui::test]
async fn test_scroll_beyond_last_line(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_scroll_height(10).await;

    let content = "ˇ".to_owned() + &sample_text(26, 2, 'a');
    cx.set_shared_state(&content).await;

    cx.update_global(|store: &mut SettingsStore, cx| {
        store.update_user_settings(cx, |s| {
            s.editor.scroll_beyond_last_line = Some(ScrollBeyondLastLine::Off);
        });
    });

    cx.simulate_shared_keystrokes("shift-g k").await;
    cx.shared_state().await.assert_matches();
    cx.simulate_shared_keystrokes("ctrl-d").await;
    cx.shared_state().await.assert_matches();

    cx.simulate_shared_keystrokes("shift-g").await;
    cx.shared_state().await.assert_matches();
    cx.simulate_shared_keystrokes("ctrl-u").await;
    cx.shared_state().await.assert_matches();
}

#[gpui::test]
async fn test_ctrl_y_e(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_scroll_height(10).await;

    let content = "ˇ".to_owned() + &sample_text(26, 2, 'a');
    cx.set_shared_state(&content).await;

    for _ in 0..8 {
        cx.simulate_shared_keystrokes("ctrl-e").await;
        cx.shared_state().await.assert_matches();
    }

    for _ in 0..8 {
        cx.simulate_shared_keystrokes("ctrl-y").await;
        cx.shared_state().await.assert_matches();
    }
}

#[gpui::test]
async fn test_scroll_jumps(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_scroll_height(20).await;

    let content = "ˇ".to_owned() + &sample_text(52, 2, 'a');
    cx.set_shared_state(&content).await;

    cx.simulate_shared_keystrokes("shift-g g g").await;
    cx.simulate_shared_keystrokes("ctrl-d ctrl-d ctrl-o").await;
    cx.shared_state().await.assert_matches();
    cx.simulate_shared_keystrokes("ctrl-o").await;
    cx.shared_state().await.assert_matches();
}

#[gpui::test]
async fn test_horizontal_scroll(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_scroll_height(20).await;
    cx.set_shared_wrap(12).await;
    cx.set_neovim_option("nowrap").await;

    let content = "ˇ01234567890123456789";
    cx.set_shared_state(content).await;

    cx.simulate_shared_keystrokes("z shift-l").await;
    cx.shared_state().await.assert_eq("012345ˇ67890123456789");

    cx.simulate_shared_keystrokes("z h").await;
    cx.shared_state().await.assert_eq("012345ˇ67890123456789");

    let content = "ˇ01234567890123456789";
    cx.set_shared_state(content).await;

    cx.simulate_shared_keystrokes("z l").await;
    cx.shared_state().await.assert_eq("0ˇ1234567890123456789");
}
