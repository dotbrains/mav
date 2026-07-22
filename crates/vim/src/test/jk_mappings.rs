use super::*;

#[perf]
#[gpui::test]
async fn test_jk(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.update(|_, cx| {
        cx.bind_keys([KeyBinding::new(
            "j k",
            NormalBefore,
            Some("vim_mode == insert"),
        )])
    });
    cx.neovim.exec("imap jk <esc>").await;

    cx.set_shared_state("ˇhello").await;
    cx.simulate_shared_keystrokes("i j o j k").await;
    cx.shared_state().await.assert_eq("jˇohello");
}

fn assert_pending_input(cx: &mut VimTestContext, expected: &str) {
    cx.update_editor(|editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        let highlights = editor
            .text_highlights(editor::HighlightKey::PendingInput, cx)
            .unwrap()
            .1;
        let (_, ranges) = marked_text_ranges(expected, false);

        assert_eq!(
            highlights
                .iter()
                .map(|highlight| highlight.to_offset(&snapshot.buffer_snapshot()))
                .collect::<Vec<_>>(),
            ranges
                .iter()
                .map(|range| MultiBufferOffset(range.start)..MultiBufferOffset(range.end))
                .collect::<Vec<_>>()
        )
    });
}

#[perf]
#[gpui::test]
async fn test_jk_multi(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.update(|_, cx| {
        cx.bind_keys([KeyBinding::new(
            "j k l",
            NormalBefore,
            Some("vim_mode == insert"),
        )])
    });

    cx.set_state("ˇone ˇone ˇone", Mode::Normal);
    cx.simulate_keystrokes("i j");
    cx.simulate_keystrokes("k");
    cx.assert_state("ˇjkone ˇjkone ˇjkone", Mode::Insert);
    assert_pending_input(&mut cx, "«jk»one «jk»one «jk»one");
    cx.simulate_keystrokes("o j k");
    cx.assert_state("jkoˇjkone jkoˇjkone jkoˇjkone", Mode::Insert);
    assert_pending_input(&mut cx, "jko«jk»one jko«jk»one jko«jk»one");
    cx.simulate_keystrokes("l");
    cx.assert_state("jkˇoone jkˇoone jkˇoone", Mode::Normal);
}

#[perf]
#[gpui::test]
async fn test_jk_delay(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.update(|_, cx| {
        cx.bind_keys([KeyBinding::new(
            "j k",
            NormalBefore,
            Some("vim_mode == insert"),
        )])
    });

    cx.set_state("ˇhello", Mode::Normal);
    cx.simulate_keystrokes("i j");
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    cx.assert_state("ˇjhello", Mode::Insert);
    cx.update_editor(|editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        let highlights = editor
            .text_highlights(editor::HighlightKey::PendingInput, cx)
            .unwrap()
            .1;

        assert_eq!(
            highlights
                .iter()
                .map(|highlight| highlight.to_offset(&snapshot.buffer_snapshot()))
                .collect::<Vec<_>>(),
            vec![MultiBufferOffset(0)..MultiBufferOffset(1)]
        )
    });
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    cx.assert_state("jˇhello", Mode::Insert);
    cx.simulate_keystrokes("k j k");
    cx.assert_state("jˇkhello", Mode::Normal);
}

#[perf]
#[gpui::test]
async fn test_jk_max_count(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("1\nˇ2\n3").await;
    cx.simulate_shared_keystrokes("9 9 9 9 9 9 9 9 9 9 9 9 9 9 9 9 9 9 9 9 j")
        .await;
    cx.shared_state().await.assert_eq("1\n2\nˇ3");

    let number: String = usize::MAX.to_string().split("").join(" ");
    cx.simulate_shared_keystrokes(&format!("{number} k")).await;
    cx.shared_state().await.assert_eq("ˇ1\n2\n3");
}

#[perf]
#[gpui::test]
async fn test_comma_w(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.update(|_, cx| {
        cx.bind_keys([KeyBinding::new(
            ", w",
            motion::Down {
                display_lines: false,
            },
            Some("vim_mode == normal"),
        )])
    });
    cx.neovim.exec("map ,w j").await;

    cx.set_shared_state("ˇhello hello\nhello hello").await;
    cx.simulate_shared_keystrokes("f o ; , w").await;
    cx.shared_state()
        .await
        .assert_eq("hello hello\nhello hellˇo");

    cx.set_shared_state("ˇhello hello\nhello hello").await;
    cx.simulate_shared_keystrokes("f o ; , i").await;
    cx.shared_state()
        .await
        .assert_eq("hellˇo hello\nhello hello");
}
