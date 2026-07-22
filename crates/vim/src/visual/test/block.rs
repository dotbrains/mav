use super::*;

#[gpui::test]
async fn test_visual_block_mode(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {
        "The ˇquick brown
         fox jumps over
         the lazy dog"
    })
    .await;
    cx.simulate_shared_keystrokes("ctrl-v").await;
    cx.shared_state().await.assert_eq(indoc! {
        "The «qˇ»uick brown
        fox jumps over
        the lazy dog"
    });
    cx.simulate_shared_keystrokes("2 down").await;
    cx.shared_state().await.assert_eq(indoc! {
        "The «qˇ»uick brown
        fox «jˇ»umps over
        the «lˇ»azy dog"
    });
    cx.simulate_shared_keystrokes("e").await;
    cx.shared_state().await.assert_eq(indoc! {
        "The «quicˇ»k brown
        fox «jumpˇ»s over
        the «lazyˇ» dog"
    });
    cx.simulate_shared_keystrokes("^").await;
    cx.shared_state().await.assert_eq(indoc! {
        "«ˇThe q»uick brown
        «ˇfox j»umps over
        «ˇthe l»azy dog"
    });
    cx.simulate_shared_keystrokes("$").await;
    cx.shared_state().await.assert_eq(indoc! {
        "The «quick brownˇ»
        fox «jumps overˇ»
        the «lazy dogˇ»"
    });
    cx.simulate_shared_keystrokes("shift-f space").await;
    cx.shared_state().await.assert_eq(indoc! {
        "The «quickˇ» brown
        fox «jumpsˇ» over
        the «lazy ˇ»dog"
    });

    // toggling through visual mode works as expected
    cx.simulate_shared_keystrokes("v").await;
    cx.shared_state().await.assert_eq(indoc! {
        "The «quick brown
        fox jumps over
        the lazy ˇ»dog"
    });
    cx.simulate_shared_keystrokes("ctrl-v").await;
    cx.shared_state().await.assert_eq(indoc! {
        "The «quickˇ» brown
        fox «jumpsˇ» over
        the «lazy ˇ»dog"
    });

    cx.set_shared_state(indoc! {
        "The ˇquick
         brown
         fox
         jumps over the

         lazy dog
        "
    })
    .await;
    cx.simulate_shared_keystrokes("ctrl-v down down").await;
    cx.shared_state().await.assert_eq(indoc! {
        "The«ˇ q»uick
        bro«ˇwn»
        foxˇ
        jumps over the

        lazy dog
        "
    });
    cx.simulate_shared_keystrokes("down").await;
    cx.shared_state().await.assert_eq(indoc! {
        "The «qˇ»uick
        brow«nˇ»
        fox
        jump«sˇ» over the

        lazy dog
        "
    });
    cx.simulate_shared_keystrokes("left").await;
    cx.shared_state().await.assert_eq(indoc! {
        "The«ˇ q»uick
        bro«ˇwn»
        foxˇ
        jum«ˇps» over the

        lazy dog
        "
    });
    cx.simulate_shared_keystrokes("s o escape").await;
    cx.shared_state().await.assert_eq(indoc! {
        "Theˇouick
        broo
        foxo
        jumo over the

        lazy dog
        "
    });

    // https://github.com/mav-industries/mav/issues/6274
    cx.set_shared_state(indoc! {
        "Theˇ quick brown

        fox jumps over
        the lazy dog
        "
    })
    .await;
    cx.simulate_shared_keystrokes("l ctrl-v j j").await;
    cx.shared_state().await.assert_eq(indoc! {
        "The «qˇ»uick brown

        fox «jˇ»umps over
        the lazy dog
        "
    });
}

#[gpui::test]
async fn test_visual_block_issue_2123(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {
        "The ˇquick brown
        fox jumps over
        the lazy dog
        "
    })
    .await;
    cx.simulate_shared_keystrokes("ctrl-v right down").await;
    cx.shared_state().await.assert_eq(indoc! {
        "The «quˇ»ick brown
        fox «juˇ»mps over
        the lazy dog
        "
    });
}
#[gpui::test]
async fn test_visual_block_mode_down_right(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.set_shared_state(indoc! {"
        The ˇquick brown
        fox jumps over
        the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("ctrl-v l l l l l j").await;
    cx.shared_state().await.assert_eq(indoc! {"
        The «quick ˇ»brown
        fox «jumps ˇ»over
        the lazy dog"});
}

#[gpui::test]
async fn test_visual_block_mode_up_left(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.set_shared_state(indoc! {"
        The quick brown
        fox jumpsˇ over
        the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("ctrl-v h h h h h k").await;
    cx.shared_state().await.assert_eq(indoc! {"
        The «ˇquick »brown
        fox «ˇjumps »over
        the lazy dog"});
}

#[gpui::test]
async fn test_visual_block_mode_other_end(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.set_shared_state(indoc! {"
        The quick brown
        fox jˇumps over
        the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("ctrl-v l l l l j").await;
    cx.shared_state().await.assert_eq(indoc! {"
        The quick brown
        fox j«umps ˇ»over
        the l«azy dˇ»og"});
    cx.simulate_shared_keystrokes("o k").await;
    cx.shared_state().await.assert_eq(indoc! {"
        The q«ˇuick »brown
        fox j«ˇumps »over
        the l«ˇazy d»og"});
}

#[gpui::test]
async fn test_visual_block_mode_shift_other_end(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.set_shared_state(indoc! {"
        The quick brown
        fox jˇumps over
        the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("ctrl-v l l l l j").await;
    cx.shared_state().await.assert_eq(indoc! {"
        The quick brown
        fox j«umps ˇ»over
        the l«azy dˇ»og"});
    cx.simulate_shared_keystrokes("shift-o k").await;
    cx.shared_state().await.assert_eq(indoc! {"
        The quick brown
        fox j«ˇumps »over
        the lazy dog"});
}

#[gpui::test]
async fn test_visual_block_insert(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {
        "ˇThe quick brown
        fox jumps over
        the lazy dog
        "
    })
    .await;
    cx.simulate_shared_keystrokes("ctrl-v 9 down").await;
    cx.shared_state().await.assert_eq(indoc! {
        "«Tˇ»he quick brown
        «fˇ»ox jumps over
        «tˇ»he lazy dog
        ˇ"
    });

    cx.simulate_shared_keystrokes("shift-i k escape").await;
    cx.shared_state().await.assert_eq(indoc! {
        "ˇkThe quick brown
        kfox jumps over
        kthe lazy dog
        k"
    });

    cx.set_shared_state(indoc! {
        "ˇThe quick brown
        fox jumps over
        the lazy dog
        "
    })
    .await;
    cx.simulate_shared_keystrokes("ctrl-v 9 down").await;
    cx.shared_state().await.assert_eq(indoc! {
        "«Tˇ»he quick brown
        «fˇ»ox jumps over
        «tˇ»he lazy dog
        ˇ"
    });
    cx.simulate_shared_keystrokes("c k escape").await;
    cx.shared_state().await.assert_eq(indoc! {
        "ˇkhe quick brown
        kox jumps over
        khe lazy dog
        k"
    });
}

#[gpui::test]
async fn test_visual_block_insert_after_ctrl_d_scroll(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    let shared_state_lines = (1..=10)
        .map(|line_number| format!("{line_number:02}"))
        .collect::<Vec<_>>()
        .join("\n");
    let shared_state = format!("ˇ{shared_state_lines}\n");

    cx.set_scroll_height(5).await;
    cx.set_shared_state(&shared_state).await;

    cx.simulate_shared_keystrokes("ctrl-v ctrl-d").await;
    cx.shared_state().await.assert_matches();

    cx.simulate_shared_keystrokes("shift-i x escape").await;
    cx.shared_state().await.assert_eq(indoc! {
        "
        ˇx01
        x02
        x03
        x04
        x05
        06
        07
        08
        09
        10
        "
    });
}

#[gpui::test]
async fn test_visual_block_wrapping_selection(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    // Ensure that the editor is wrapping lines at 12 columns so that each
    // of the lines ends up being wrapped.
    cx.set_shared_wrap(12).await;
    cx.set_shared_state(indoc! {
        "ˇ12345678901234567890
        12345678901234567890
        12345678901234567890
        "
    })
    .await;
    cx.simulate_shared_keystrokes("ctrl-v j").await;
    cx.shared_state().await.assert_eq(indoc! {
        "«1ˇ»2345678901234567890
        «1ˇ»2345678901234567890
        12345678901234567890
        "
    });

    // Test with lines taking up different amounts of display rows to ensure
    // that, even in that case, only the buffer rows are taken into account.
    cx.set_shared_state(indoc! {
        "ˇ123456789012345678901234567890123456789012345678901234567890
        1234567890123456789012345678901234567890
        12345678901234567890
        "
    })
    .await;
    cx.simulate_shared_keystrokes("ctrl-v 2 j").await;
    cx.shared_state().await.assert_eq(indoc! {
        "«1ˇ»23456789012345678901234567890123456789012345678901234567890
        «1ˇ»234567890123456789012345678901234567890
        «1ˇ»2345678901234567890
        "
    });

    // Same scenario as above, but using the up motion to ensure that the
    // result is the same.
    cx.set_shared_state(indoc! {
        "123456789012345678901234567890123456789012345678901234567890
        1234567890123456789012345678901234567890
        ˇ12345678901234567890
        "
    })
    .await;
    cx.simulate_shared_keystrokes("ctrl-v 2 k").await;
    cx.shared_state().await.assert_eq(indoc! {
        "«1ˇ»23456789012345678901234567890123456789012345678901234567890
        «1ˇ»234567890123456789012345678901234567890
        «1ˇ»2345678901234567890
        "
    });
}
