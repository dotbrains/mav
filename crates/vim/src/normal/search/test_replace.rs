use std::time::Duration;

use crate::{
    state::Mode,
    test::{NeovimBackedTestContext, VimTestContext},
};
use editor::{DisplayPoint, display_map::DisplayRow};

use indoc::indoc;
use search::BufferSearchBar;
use settings::SettingsStore;

#[gpui::test]
async fn test_replace_with_range_at_start(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {
        "ˇa
        a
        a
        a
        a
        a
        a
         "
    })
    .await;
    cx.simulate_shared_keystrokes(": 2 , 5 s / ^ / b").await;
    cx.simulate_shared_keystrokes("enter").await;
    cx.shared_state().await.assert_eq(indoc! {
        "a
        ba
        ba
        ba
        ˇba
        a
        a
         "
    });

    cx.simulate_shared_keystrokes("/ a").await;
    cx.simulate_shared_keystrokes("enter").await;
    cx.shared_state().await.assert_eq(indoc! {
        "a
            ba
            ba
            ba
            bˇa
            a
            a
             "
    });
}

#[gpui::test]
async fn test_search_skipping(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.set_shared_state(indoc! {
        "ˇaa aa aa"
    })
    .await;

    cx.simulate_shared_keystrokes("/ a a").await;
    cx.simulate_shared_keystrokes("enter").await;

    cx.shared_state().await.assert_eq(indoc! {
        "aa ˇaa aa"
    });

    cx.simulate_shared_keystrokes("left / a a").await;
    cx.simulate_shared_keystrokes("enter").await;

    cx.shared_state().await.assert_eq(indoc! {
        "aa ˇaa aa"
    });
}

#[gpui::test]
async fn test_replace_n(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.set_shared_state(indoc! {
        "ˇaa
        bb
        aa"
    })
    .await;

    cx.simulate_shared_keystrokes(": s / b b / d d / n").await;
    cx.simulate_shared_keystrokes("enter").await;

    cx.shared_state().await.assert_eq(indoc! {
        "ˇaa
        bb
        aa"
    });

    let search_bar = cx.update_workspace(|workspace, _, cx| {
        workspace.active_pane().update(cx, |pane, cx| {
            pane.toolbar()
                .read(cx)
                .item_of_type::<BufferSearchBar>()
                .unwrap()
        })
    });
    cx.update_entity(search_bar, |search_bar, _, cx| {
        assert!(!search_bar.is_dismissed());
        assert_eq!(search_bar.query(cx), "bb".to_string());
        assert_eq!(search_bar.replacement(cx), "dd".to_string());
    })
}

#[gpui::test]
async fn test_replace_literal_dollar(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.set_shared_state(indoc! {
        "ˇBase=hello
        echo $Base"
    })
    .await;

    cx.simulate_shared_keystrokes(
        ": % s / \\ $ shift-b a s e / \\ $ shift-b a s e shift-n e w / g",
    )
    .await;
    cx.simulate_shared_keystrokes("enter").await;

    cx.shared_state().await.assert_eq(indoc! {
        "Base=hello
        ˇecho $BaseNew"
    });
}

#[gpui::test]
async fn test_replace_g(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.set_shared_state(indoc! {
        "ˇaa aa aa aa
        aa
        aa"
    })
    .await;

    cx.simulate_shared_keystrokes(": s / a a / b b").await;
    cx.simulate_shared_keystrokes("enter").await;
    cx.shared_state().await.assert_eq(indoc! {
        "ˇbb aa aa aa
        aa
        aa"
    });
    cx.simulate_shared_keystrokes(": s / a a / b b / g").await;
    cx.simulate_shared_keystrokes("enter").await;
    cx.shared_state().await.assert_eq(indoc! {
        "ˇbb bb bb bb
        aa
        aa"
    });
}

#[gpui::test]
async fn test_replace_gdefault(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    // Set the `gdefault` option in both Mav and Neovim.
    cx.simulate_shared_keystrokes(": s e t space g d e f a u l t")
        .await;
    cx.simulate_shared_keystrokes("enter").await;

    cx.set_shared_state(indoc! {
        "ˇaa aa aa aa
            aa
            aa"
    })
    .await;

    // With gdefault on, :s/// replaces all matches (like :s///g normally).
    cx.simulate_shared_keystrokes(": s / a a / b b").await;
    cx.simulate_shared_keystrokes("enter").await;
    cx.shared_state().await.assert_eq(indoc! {
        "ˇbb bb bb bb
            aa
            aa"
    });

    // With gdefault on, :s///g replaces only the first match.
    cx.simulate_shared_keystrokes(": s / b b / c c / g").await;
    cx.simulate_shared_keystrokes("enter").await;
    cx.shared_state().await.assert_eq(indoc! {
        "ˇcc bb bb bb
            aa
            aa"
    });

    // Each successive `/g` flag should invert the one before it.
    cx.simulate_shared_keystrokes(": s / b b / d d / g g").await;
    cx.simulate_shared_keystrokes("enter").await;
    cx.shared_state().await.assert_eq(indoc! {
        "ˇcc dd dd dd
            aa
            aa"
    });

    cx.simulate_shared_keystrokes(": s / c c / e e / g g g")
        .await;
    cx.simulate_shared_keystrokes("enter").await;
    cx.shared_state().await.assert_eq(indoc! {
        "ˇee dd dd dd
            aa
            aa"
    });
}

#[gpui::test]
async fn test_replace_c(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.set_state(
        indoc! {
            "ˇaa
        aa
        aa"
        },
        Mode::Normal,
    );

    cx.simulate_keystrokes("v j : s / a a / d d / c");
    cx.simulate_keystrokes("enter");

    cx.assert_state(
        indoc! {
            "ˇaa
        aa
        aa"
        },
        Mode::Normal,
    );

    cx.simulate_keystrokes("enter");

    cx.assert_state(
        indoc! {
            "dd
        ˇaa
        aa"
        },
        Mode::Normal,
    );

    cx.simulate_keystrokes("enter");
    cx.assert_state(
        indoc! {
            "dd
        ddˇ
        aa"
        },
        Mode::Normal,
    );
    cx.simulate_keystrokes("enter");
    cx.assert_state(
        indoc! {
            "dd
        ddˇ
        aa"
        },
        Mode::Normal,
    );
}

#[gpui::test]
async fn test_replace_with_range(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {
        "ˇa
        a
        a
        a
        a
        a
        a
         "
    })
    .await;
    cx.simulate_shared_keystrokes(": 2 , 5 s / a / b").await;
    cx.simulate_shared_keystrokes("enter").await;
    cx.shared_state().await.assert_eq(indoc! {
        "a
        b
        b
        b
        ˇb
        a
        a
         "
    });
    cx.executor().advance_clock(Duration::from_millis(250));
    cx.run_until_parked();

    cx.simulate_shared_keystrokes("/ a enter").await;
    cx.shared_state().await.assert_eq(indoc! {
        "a
            b
            b
            b
            b
            ˇa
            a
             "
    });
}
