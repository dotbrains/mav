
use editor::test::editor_lsp_test_context::EditorLspTestContext;
use futures::StreamExt;
use indoc::indoc;

use gpui::EntityInputHandler;

use crate::{
    VimGlobals,
    state::Mode,
    test::{NeovimBackedTestContext, VimTestContext},
};
#[gpui::test]
async fn test_repeat_visual(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    // single-line (3 columns)
    cx.set_shared_state(indoc! {
        "ˇthe quick brown
            fox jumps over
            the lazy dog"
    })
    .await;
    cx.simulate_shared_keystrokes("v i w s o escape").await;
    cx.shared_state().await.assert_eq(indoc! {
        "ˇo quick brown
            fox jumps over
            the lazy dog"
    });
    cx.simulate_shared_keystrokes("j w .").await;
    cx.shared_state().await.assert_eq(indoc! {
        "o quick brown
            fox ˇops over
            the lazy dog"
    });
    cx.simulate_shared_keystrokes("f r .").await;
    cx.shared_state().await.assert_eq(indoc! {
        "o quick brown
        fox ops oveˇothe lazy dog"
    });

    // visual
    cx.set_shared_state(indoc! {
        "the ˇquick brown
            fox jumps over
            fox jumps over
            fox jumps over
            the lazy dog"
    })
    .await;
    cx.simulate_shared_keystrokes("v j x").await;
    cx.shared_state().await.assert_eq(indoc! {
        "the ˇumps over
            fox jumps over
            fox jumps over
            the lazy dog"
    });
    cx.simulate_shared_keystrokes(".").await;
    cx.shared_state().await.assert_eq(indoc! {
        "the ˇumps over
            fox jumps over
            the lazy dog"
    });
    cx.simulate_shared_keystrokes("w .").await;
    cx.shared_state().await.assert_eq(indoc! {
        "the umps ˇumps over
        the lazy dog"
    });
    cx.simulate_shared_keystrokes("j .").await;
    cx.shared_state().await.assert_eq(indoc! {
        "the umps umps over
        the ˇog"
    });

    // block mode (3 rows)
    cx.set_shared_state(indoc! {
        "ˇthe quick brown
            fox jumps over
            the lazy dog"
    })
    .await;
    cx.simulate_shared_keystrokes("ctrl-v j j shift-i o escape")
        .await;
    cx.shared_state().await.assert_eq(indoc! {
        "ˇothe quick brown
            ofox jumps over
            othe lazy dog"
    });
    cx.simulate_shared_keystrokes("j 4 l .").await;
    cx.shared_state().await.assert_eq(indoc! {
        "othe quick brown
            ofoxˇo jumps over
            otheo lazy dog"
    });

    // line mode
    cx.set_shared_state(indoc! {
        "ˇthe quick brown
            fox jumps over
            the lazy dog"
    })
    .await;
    cx.simulate_shared_keystrokes("shift-v shift-r o escape")
        .await;
    cx.shared_state().await.assert_eq(indoc! {
        "ˇo
            fox jumps over
            the lazy dog"
    });
    cx.simulate_shared_keystrokes("j .").await;
    cx.shared_state().await.assert_eq(indoc! {
        "o
            ˇo
            the lazy dog"
    });
}

#[gpui::test]
async fn test_repeat_motion_counts(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {
        "ˇthe quick brown
            fox jumps over
            the lazy dog"
    })
    .await;
    cx.simulate_shared_keystrokes("3 d 3 l").await;
    cx.shared_state().await.assert_eq(indoc! {
        "ˇ brown
            fox jumps over
            the lazy dog"
    });
    cx.simulate_shared_keystrokes("j .").await;
    cx.shared_state().await.assert_eq(indoc! {
        " brown
            ˇ over
            the lazy dog"
    });
    cx.simulate_shared_keystrokes("j 2 .").await;
    cx.shared_state().await.assert_eq(indoc! {
        " brown
             over
            ˇe lazy dog"
    });
}
