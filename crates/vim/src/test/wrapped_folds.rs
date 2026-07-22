use super::*;

#[cfg(target_os = "macos")]
#[perf]
#[gpui::test]
async fn test_wrapped_lines(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_wrap(12).await;
    // tests line wrap as follows:
    //  1: twelve char
    //     twelve char
    //  2: twelve char
    cx.set_shared_state(indoc! { "
        tˇwelve char twelve char
        twelve char
    "})
        .await;
    cx.simulate_shared_keystrokes("j").await;
    cx.shared_state().await.assert_eq(indoc! {"
        twelve char twelve char
        tˇwelve char
    "});
    cx.simulate_shared_keystrokes("k").await;
    cx.shared_state().await.assert_eq(indoc! {"
        tˇwelve char twelve char
        twelve char
    "});
    cx.simulate_shared_keystrokes("g j").await;
    cx.shared_state().await.assert_eq(indoc! {"
        twelve char tˇwelve char
        twelve char
    "});
    cx.simulate_shared_keystrokes("g j").await;
    cx.shared_state().await.assert_eq(indoc! {"
        twelve char twelve char
        tˇwelve char
    "});

    cx.simulate_shared_keystrokes("g k").await;
    cx.shared_state().await.assert_eq(indoc! {"
        twelve char tˇwelve char
        twelve char
    "});

    cx.simulate_shared_keystrokes("g ^").await;
    cx.shared_state().await.assert_eq(indoc! {"
        twelve char ˇtwelve char
        twelve char
    "});

    cx.simulate_shared_keystrokes("^").await;
    cx.shared_state().await.assert_eq(indoc! {"
        ˇtwelve char twelve char
        twelve char
    "});

    cx.simulate_shared_keystrokes("g $").await;
    cx.shared_state().await.assert_eq(indoc! {"
        twelve charˇ twelve char
        twelve char
    "});
    cx.simulate_shared_keystrokes("$").await;
    cx.shared_state().await.assert_eq(indoc! {"
        twelve char twelve chaˇr
        twelve char
    "});

    cx.set_shared_state(indoc! { "
        tˇwelve char twelve char
        twelve char
    "})
        .await;
    cx.simulate_shared_keystrokes("enter").await;
    cx.shared_state().await.assert_eq(indoc! {"
            twelve char twelve char
            ˇtwelve char
        "});

    cx.set_shared_state(indoc! { "
        twelve char
        tˇwelve char twelve char
        twelve char
    "})
        .await;
    cx.simulate_shared_keystrokes("o o escape").await;
    cx.shared_state().await.assert_eq(indoc! {"
        twelve char
        twelve char twelve char
        ˇo
        twelve char
    "});

    cx.set_shared_state(indoc! { "
        twelve char
        tˇwelve char twelve char
        twelve char
    "})
        .await;
    cx.simulate_shared_keystrokes("shift-a a escape").await;
    cx.shared_state().await.assert_eq(indoc! {"
        twelve char
        twelve char twelve charˇa
        twelve char
    "});
    cx.simulate_shared_keystrokes("shift-i i escape").await;
    cx.shared_state().await.assert_eq(indoc! {"
        twelve char
        ˇitwelve char twelve chara
        twelve char
    "});
    cx.simulate_shared_keystrokes("shift-d").await;
    cx.shared_state().await.assert_eq(indoc! {"
        twelve char
        ˇ
        twelve char
    "});

    cx.set_shared_state(indoc! { "
        twelve char
        twelve char tˇwelve char
        twelve char
    "})
        .await;
    cx.simulate_shared_keystrokes("shift-o o escape").await;
    cx.shared_state().await.assert_eq(indoc! {"
        twelve char
        ˇo
        twelve char twelve char
        twelve char
    "});

    // line wraps as:
    // fourteen ch
    // ar
    // fourteen ch
    // ar
    cx.set_shared_state(indoc! { "
        fourteen chaˇr
        fourteen char
    "})
        .await;

    cx.simulate_shared_keystrokes("d i w").await;
    cx.shared_state().await.assert_eq(indoc! {"
        fourteenˇ•
        fourteen char
    "});
    cx.simulate_shared_keystrokes("j shift-f e f r").await;
    cx.shared_state().await.assert_eq(indoc! {"
        fourteen•
        fourteen chaˇr
    "});
}

#[perf]
#[gpui::test]
async fn test_folds(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.set_neovim_option("foldmethod=manual").await;

    cx.set_shared_state(indoc! { "
        fn boop() {
          ˇbarp()
          bazp()
        }
    "})
        .await;
    cx.simulate_shared_keystrokes("shift-v j z f").await;

    // visual display is now:
    // fn boop () {
    //  [FOLDED]
    // }

    // TODO: this should not be needed but currently zf does not
    // return to normal mode.
    cx.simulate_shared_keystrokes("escape").await;

    // skip over fold downward
    cx.simulate_shared_keystrokes("g g").await;
    cx.shared_state().await.assert_eq(indoc! {"
        ˇfn boop() {
          barp()
          bazp()
        }
    "});

    cx.simulate_shared_keystrokes("j j").await;
    cx.shared_state().await.assert_eq(indoc! {"
        fn boop() {
          barp()
          bazp()
        ˇ}
    "});

    // skip over fold upward
    cx.simulate_shared_keystrokes("2 k").await;
    cx.shared_state().await.assert_eq(indoc! {"
        ˇfn boop() {
          barp()
          bazp()
        }
    "});

    // yank the fold
    cx.simulate_shared_keystrokes("down y y").await;
    cx.shared_clipboard()
        .await
        .assert_eq("  barp()\n  bazp()\n");

    // re-open
    cx.simulate_shared_keystrokes("z o").await;
    cx.shared_state().await.assert_eq(indoc! {"
        fn boop() {
        ˇ  barp()
          bazp()
        }
    "});
}

#[perf]
#[gpui::test]
async fn test_folds_panic(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.set_neovim_option("foldmethod=manual").await;

    cx.set_shared_state(indoc! { "
        fn boop() {
          ˇbarp()
          bazp()
        }
    "})
        .await;
    cx.simulate_shared_keystrokes("shift-v j z f").await;
    cx.simulate_shared_keystrokes("escape").await;
    cx.simulate_shared_keystrokes("g g").await;
    cx.simulate_shared_keystrokes("5 d j").await;
    cx.shared_state().await.assert_eq("ˇ");
    cx.set_shared_state(indoc! {"
        fn boop() {
          ˇbarp()
          bazp()
        }
    "})
        .await;
    cx.simulate_shared_keystrokes("shift-v j j z f").await;
    cx.simulate_shared_keystrokes("escape").await;
    cx.simulate_shared_keystrokes("shift-g shift-v").await;
    cx.shared_state().await.assert_eq(indoc! {"
        fn boop() {
          barp()
          bazp()
        }
        ˇ"});
}

#[perf]
#[gpui::test]
async fn test_clear_counts(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {"
        The quick brown
        fox juˇmps over
        the lazy dog"})
        .await;

    cx.simulate_shared_keystrokes("4 escape 3 d l").await;
    cx.shared_state().await.assert_eq(indoc! {"
        The quick brown
        fox juˇ over
        the lazy dog"});
}

#[perf]
#[gpui::test]
async fn test_zero(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {"
        The quˇick brown
        fox jumps over
        the lazy dog"})
        .await;

    cx.simulate_shared_keystrokes("0").await;
    cx.shared_state().await.assert_eq(indoc! {"
        ˇThe quick brown
        fox jumps over
        the lazy dog"});

    cx.simulate_shared_keystrokes("1 0 l").await;
    cx.shared_state().await.assert_eq(indoc! {"
        The quick ˇbrown
        fox jumps over
        the lazy dog"});
}

#[perf]
#[gpui::test]
async fn test_selection_goal(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {"
        ;;ˇ;
        Lorem Ipsum"})
        .await;

    cx.simulate_shared_keystrokes("a down up ; down up").await;
    cx.shared_state().await.assert_eq(indoc! {"
        ;;;;ˇ
        Lorem Ipsum"});
}

#[cfg(target_os = "macos")]
#[perf]
#[gpui::test]
async fn test_wrapped_motions(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_wrap(12).await;

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

    cx.set_shared_state(indoc! {"
                123456789012aaˇaa
                123456789012😃😃"
    })
    .await;
    cx.simulate_shared_keystrokes("j").await;
    cx.shared_state().await.assert_eq(indoc! {"
        123456789012aaaa
        123456789012😃ˇ😃"
    });

    cx.set_shared_state(indoc! {"
                123456789012aaˇaa
                123456789012😃😃"
    })
    .await;
    cx.simulate_shared_keystrokes("j").await;
    cx.shared_state().await.assert_eq(indoc! {"
        123456789012aaaa
        123456789012😃ˇ😃"
    });

    cx.set_shared_state(indoc! {"
        123456789012aaaaˇaaaaaaaa123456789012
        wow
        123456789012😃😃😃😃😃😃123456789012"
    })
    .await;
    cx.simulate_shared_keystrokes("j j").await;
    cx.shared_state().await.assert_eq(indoc! {"
        123456789012aaaaaaaaaaaa123456789012
        wow
        123456789012😃😃ˇ😃😃😃😃123456789012"
    });
}

#[perf]
#[gpui::test]
async fn test_wrapped_delete_end_document(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_wrap(12).await;

    cx.set_shared_state(indoc! {"
                aaˇaaaaaaaaaaaaaaaaaa
                bbbbbbbbbbbbbbbbbbbb
                cccccccccccccccccccc"
    })
    .await;
    cx.simulate_shared_keystrokes("d shift-g i z z z").await;
    cx.shared_state().await.assert_eq(indoc! {"
                zzzˇ"
    });
}

#[perf]
#[gpui::test]
async fn test_paragraphs_dont_wrap(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {"
        one
        ˇ
        two"})
        .await;

    cx.simulate_shared_keystrokes("} }").await;
    cx.shared_state().await.assert_eq(indoc! {"
        one

        twˇo"});

    cx.simulate_shared_keystrokes("{ { {").await;
    cx.shared_state().await.assert_eq(indoc! {"
        ˇone

        two"});
}

#[perf]
#[gpui::test]
async fn test_select_all_issue_2170(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state(
        indoc! {"
        defmodule Test do
            def test(a, ˇ[_, _] = b), do: IO.puts('hi')
        end
    "},
        Mode::Normal,
    );
    cx.simulate_keystrokes("g a");
    cx.assert_state(
        indoc! {"
        defmodule Test do
            def test(a, «[ˇ»_, _] = b), do: IO.puts('hi')
        end
    "},
        Mode::Visual,
    );
}
