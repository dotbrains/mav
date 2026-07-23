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
async fn test_record_interrupted(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state("ˇhello\n", Mode::Normal);
    cx.simulate_keystrokes("4 i j cmd-shift-p escape");
    cx.simulate_keystrokes("escape");
    cx.assert_state("ˇjhello\n", Mode::Normal);
}

#[gpui::test]
async fn test_repeat_over_blur(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("ˇhello hello hello\n").await;
    cx.simulate_shared_keystrokes("c f o x escape").await;
    cx.shared_state().await.assert_eq("ˇx hello hello\n");
    cx.simulate_shared_keystrokes(": escape").await;
    cx.simulate_shared_keystrokes(".").await;
    cx.shared_state().await.assert_eq("ˇx hello\n");
}

#[gpui::test]
async fn test_repeat_after_blur_resets_dot_replaying(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    // Bind `ctrl-f` to the `buffer_search::Deploy` action so that this can
    // be triggered while in Insert mode, ensuring that an action which
    // moves the focus away from the editor, gets recorded.
    cx.update(|_, cx| {
        cx.bind_keys([gpui::KeyBinding::new(
            "ctrl-f",
            search::buffer_search::Deploy::find(),
            None,
        )])
    });

    cx.set_state("ˇhello", Mode::Normal);

    // We're going to enter insert mode, which will start recording, type a
    // character and then immediately use `ctrl-f` to trigger the buffer
    // search. Triggering the buffer search will move focus away from the
    // editor, effectively stopping the recording immediately after
    // `buffer_search::Deploy` is recorded. The first `escape` is used to
    // dismiss the search bar, while the second is used to move from Insert
    // to Normal mode.
    cx.simulate_keystrokes("i x ctrl-f escape escape");
    cx.run_until_parked();

    // Using the `.` key will dispatch the `vim::Repeat` action, repeating
    // the set of recorded actions. This will eventually focus on the search
    // bar, preventing the `EndRepeat` action from being correctly handled.
    cx.simulate_keystrokes(".");
    cx.run_until_parked();

    // After replay finishes, even though the `EndRepeat` action wasn't
    // handled, seeing as the editor lost focus during replay, the
    // `dot_replaying` value should be set back to `false`.
    assert!(
        !cx.update(|_, cx| cx.global::<VimGlobals>().dot_replaying),
        "dot_replaying should be false after repeat completes"
    );
}

#[gpui::test]
async fn test_undo_repeated_insert(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("hellˇo").await;
    cx.simulate_shared_keystrokes("3 a . escape").await;
    cx.shared_state().await.assert_eq("hello..ˇ.");
    cx.simulate_shared_keystrokes("u").await;
    cx.shared_state().await.assert_eq("hellˇo");
}

#[gpui::test]
async fn test_record_replay(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("ˇhello world").await;
    cx.simulate_shared_keystrokes("q w c w j escape q").await;
    cx.shared_state().await.assert_eq("ˇj world");
    cx.simulate_shared_keystrokes("2 l @ w").await;
    cx.shared_state().await.assert_eq("j ˇj");
}

#[gpui::test]
async fn test_record_replay_count(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("ˇhello world!!").await;
    cx.simulate_shared_keystrokes("q a v 3 l s 0 escape l q")
        .await;
    cx.shared_state().await.assert_eq("0ˇo world!!");
    cx.simulate_shared_keystrokes("2 @ a").await;
    cx.shared_state().await.assert_eq("000ˇ!");
}

#[gpui::test]
async fn test_record_replay_dot(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("ˇhello world").await;
    cx.simulate_shared_keystrokes("q a r a l r b l q").await;
    cx.shared_state().await.assert_eq("abˇllo world");
    cx.simulate_shared_keystrokes(".").await;
    cx.shared_state().await.assert_eq("abˇblo world");
    cx.simulate_shared_keystrokes("shift-q").await;
    cx.shared_state().await.assert_eq("ababˇo world");
    cx.simulate_shared_keystrokes(".").await;
    cx.shared_state().await.assert_eq("ababˇb world");
}

#[gpui::test]
async fn test_record_replay_of_dot(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("ˇhello world").await;
    cx.simulate_shared_keystrokes("r o q w . q").await;
    cx.shared_state().await.assert_eq("ˇoello world");
    cx.simulate_shared_keystrokes("d l").await;
    cx.shared_state().await.assert_eq("ˇello world");
    cx.simulate_shared_keystrokes("@ w").await;
    cx.shared_state().await.assert_eq("ˇllo world");
}

#[gpui::test]
async fn test_record_replay_interleaved(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("ˇhello world").await;
    cx.simulate_shared_keystrokes("q z r a l q").await;
    cx.shared_state().await.assert_eq("aˇello world");
    cx.simulate_shared_keystrokes("q b @ z @ z q").await;
    cx.shared_state().await.assert_eq("aaaˇlo world");
    cx.simulate_shared_keystrokes("@ @").await;
    cx.shared_state().await.assert_eq("aaaaˇo world");
    cx.simulate_shared_keystrokes("@ b").await;
    cx.shared_state().await.assert_eq("aaaaaaˇworld");
    cx.simulate_shared_keystrokes("@ @").await;
    cx.shared_state().await.assert_eq("aaaaaaaˇorld");
    cx.simulate_shared_keystrokes("q z r b l q").await;
    cx.shared_state().await.assert_eq("aaaaaaabˇrld");
    cx.simulate_shared_keystrokes("@ b").await;
    cx.shared_state().await.assert_eq("aaaaaaabbbˇd");
}

#[gpui::test]
async fn test_repeat_clear(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    // Check that, when repeat is preceded by something other than a number,
    // the current operator is cleared, in order to prevent infinite loops.
    cx.set_state("ˇhello world", Mode::Normal);
    cx.simulate_keystrokes("d .");
    assert_eq!(cx.active_operator(), None);
}

#[gpui::test]
async fn test_repeat_clear_repeat(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {
        "ˇthe quick brown
            fox jumps over
            the lazy dog"
    })
    .await;
    cx.simulate_shared_keystrokes("d d").await;
    cx.shared_state().await.assert_eq(indoc! {
        "ˇfox jumps over
        the lazy dog"
    });
    cx.simulate_shared_keystrokes("d . .").await;
    cx.shared_state().await.assert_eq(indoc! {
        "ˇthe lazy dog"
    });
}

#[gpui::test]
async fn test_repeat_clear_count(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {
        "ˇthe quick brown
            fox jumps over
            the lazy dog"
    })
    .await;
    cx.simulate_shared_keystrokes("d d").await;
    cx.shared_state().await.assert_eq(indoc! {
        "ˇfox jumps over
        the lazy dog"
    });
    cx.simulate_shared_keystrokes("2 d .").await;
    cx.shared_state().await.assert_eq(indoc! {
        "ˇfox jumps over
        the lazy dog"
    });
    cx.simulate_shared_keystrokes(".").await;
    cx.shared_state().await.assert_eq(indoc! {
        "ˇthe lazy dog"
    });

    cx.set_shared_state(indoc! {
        "ˇthe quick brown
            fox jumps over
            the lazy dog
            the quick brown
            fox jumps over
            the lazy dog"
    })
    .await;
    cx.simulate_shared_keystrokes("2 d d").await;
    cx.shared_state().await.assert_eq(indoc! {
        "ˇthe lazy dog
            the quick brown
            fox jumps over
            the lazy dog"
    });
    cx.simulate_shared_keystrokes("5 d .").await;
    cx.shared_state().await.assert_eq(indoc! {
        "ˇthe lazy dog
            the quick brown
            fox jumps over
            the lazy dog"
    });
    cx.simulate_shared_keystrokes(".").await;
    cx.shared_state().await.assert_eq(indoc! {
        "ˇfox jumps over
        the lazy dog"
    });
}
