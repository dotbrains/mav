use super::*;

#[perf(iterations = 1)]
#[gpui::test]
async fn test_folded_multibuffer_excerpts(cx: &mut gpui::TestAppContext) {
    VimTestContext::init(cx);
    cx.update(|cx| {
        VimTestContext::init_keybindings(true, cx);
    });
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let multi_buffer = MultiBuffer::build_multi(
            [
                ("111\n222\n333\n444\n", vec![Point::row_range(0..2)]),
                ("aaa\nbbb\nccc\nddd\n", vec![Point::row_range(0..2)]),
                ("AAA\nBBB\nCCC\nDDD\n", vec![Point::row_range(0..2)]),
                ("one\ntwo\nthr\nfou\n", vec![Point::row_range(0..2)]),
            ],
            cx,
        );
        let mut editor = Editor::new(EditorMode::full(), multi_buffer.clone(), None, window, cx);

        let buffer_ids = multi_buffer
            .read(cx)
            .snapshot(cx)
            .excerpts()
            .map(|excerpt| excerpt.context.start.buffer_id)
            .collect::<Vec<_>>();
        // fold all but the second buffer, so that we test navigating between two
        // adjacent folded buffers, as well as folded buffers at the start and
        // end the multibuffer
        editor.fold_buffer(buffer_ids[0], cx);
        editor.fold_buffer(buffer_ids[2], cx);
        editor.fold_buffer(buffer_ids[3], cx);

        editor
    });
    let mut cx = EditorTestContext::for_editor_in(editor.clone(), cx).await;

    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        ˇ[FOLDED]
        [EXCERPT]
        aaa
        bbb
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        [FOLDED]
        "
    });
    cx.simulate_keystroke("j");
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        ˇaaa
        bbb
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        [FOLDED]
        "
    });
    cx.simulate_keystroke("j");
    cx.simulate_keystroke("j");
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        aaa
        bbb
        ˇ[EXCERPT]
        [FOLDED]
        [EXCERPT]
        [FOLDED]
        "
    });
    cx.simulate_keystroke("j");
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        aaa
        bbb
        [EXCERPT]
        ˇ[FOLDED]
        [EXCERPT]
        [FOLDED]
        "
    });
    cx.simulate_keystroke("j");
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        aaa
        bbb
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        ˇ[FOLDED]
        "
    });
    cx.simulate_keystroke("k");
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        aaa
        bbb
        [EXCERPT]
        ˇ[FOLDED]
        [EXCERPT]
        [FOLDED]
        "
    });
    cx.simulate_keystroke("k");
    cx.simulate_keystroke("k");
    cx.simulate_keystroke("k");
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        ˇaaa
        bbb
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        [FOLDED]
        "
    });
    cx.simulate_keystroke("k");
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        ˇ[FOLDED]
        [EXCERPT]
        aaa
        bbb
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        [FOLDED]
        "
    });
    cx.simulate_keystroke("shift-g");
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        aaa
        bbb
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        ˇ[FOLDED]
        "
    });
    cx.simulate_keystrokes("g g");
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        ˇ[FOLDED]
        [EXCERPT]
        aaa
        bbb
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        [FOLDED]
        "
    });
    cx.update_editor(|editor, _, cx| {
        let buffer_ids = editor
            .buffer()
            .read(cx)
            .snapshot(cx)
            .excerpts()
            .map(|excerpt| excerpt.context.start.buffer_id)
            .collect::<Vec<_>>();
        editor.fold_buffer(buffer_ids[1], cx);
    });

    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        ˇ[FOLDED]
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        [FOLDED]
        "
    });
    cx.simulate_keystrokes("2 j");
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        ˇ[FOLDED]
        [EXCERPT]
        [FOLDED]
        "
    });
}

#[perf]
#[gpui::test]
async fn test_delete_paragraph_motion(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.set_shared_state(indoc! {
        "ˇhello world.

        hello world.
        "
    })
    .await;
    cx.simulate_shared_keystrokes("y }").await;
    cx.shared_clipboard().await.assert_eq("hello world.\n");
    cx.simulate_shared_keystrokes("d }").await;
    cx.shared_state().await.assert_eq("ˇ\nhello world.\n");
    cx.shared_clipboard().await.assert_eq("hello world.\n");

    cx.set_shared_state(indoc! {
        "helˇlo world.

            hello world.
            "
    })
    .await;
    cx.simulate_shared_keystrokes("y }").await;
    cx.shared_clipboard().await.assert_eq("lo world.");
    cx.simulate_shared_keystrokes("d }").await;
    cx.shared_state().await.assert_eq("heˇl\n\nhello world.\n");
    cx.shared_clipboard().await.assert_eq("lo world.");
}

#[perf]
#[gpui::test]
async fn test_delete_unmatched_brace(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.set_shared_state(indoc! {
        "fn o(wow: i32) {
          othˇ(wow)
          oth(wow)
        }
        "
    })
    .await;
    cx.simulate_shared_keystrokes("d ] }").await;
    cx.shared_state().await.assert_eq(indoc! {
        "fn o(wow: i32) {
          otˇh
        }
        "
    });
    cx.shared_clipboard().await.assert_eq("(wow)\n  oth(wow)");
    cx.set_shared_state(indoc! {
        "fn o(wow: i32) {
          ˇoth(wow)
          oth(wow)
        }
        "
    })
    .await;
    cx.simulate_shared_keystrokes("d ] }").await;
    cx.shared_state().await.assert_eq(indoc! {
        "fn o(wow: i32) {
         ˇ}
        "
    });
    cx.shared_clipboard()
        .await
        .assert_eq("  oth(wow)\n  oth(wow)\n");
}

#[perf]
#[gpui::test]
async fn test_paragraph_multi_delete(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.set_shared_state(indoc! {
        "
        Emacs is
        ˇa great

        operating system

        all it lacks
        is a

        decent text editor
        "
    })
    .await;

    cx.simulate_shared_keystrokes("2 d a p").await;
    cx.shared_state().await.assert_eq(indoc! {
        "
        ˇall it lacks
        is a

        decent text editor
        "
    });

    cx.simulate_shared_keystrokes("d a p").await;
    cx.shared_clipboard()
        .await
        .assert_eq("all it lacks\nis a\n\n");

    //reset to initial state
    cx.simulate_shared_keystrokes("2 u").await;

    cx.simulate_shared_keystrokes("4 d a p").await;
    cx.shared_state().await.assert_eq(indoc! {"ˇ"});
}

#[perf]
#[gpui::test]
async fn test_yank_paragraph_with_paste(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.set_shared_state(indoc! {
        "
        first paragraph
        ˇstill first

        second paragraph
        still second

        third paragraph
        "
    })
    .await;

    cx.simulate_shared_keystrokes("y a p").await;
    cx.shared_clipboard()
        .await
        .assert_eq("first paragraph\nstill first\n\n");

    cx.simulate_shared_keystrokes("j j p").await;
    cx.shared_state().await.assert_eq(indoc! {
        "
        first paragraph
        still first

        ˇfirst paragraph
        still first

        second paragraph
        still second

        third paragraph
        "
    });
}

#[perf]
#[gpui::test]
async fn test_change_paragraph(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.set_shared_state(indoc! {
        "
        first paragraph
        ˇstill first

        second paragraph
        still second

        third paragraph
        "
    })
    .await;

    cx.simulate_shared_keystrokes("c a p").await;
    cx.shared_clipboard()
        .await
        .assert_eq("first paragraph\nstill first\n\n");

    cx.simulate_shared_keystrokes("escape").await;
    cx.shared_state().await.assert_eq(indoc! {
        "
        ˇ
        second paragraph
        still second

        third paragraph
        "
    });
}
