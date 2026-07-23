use indoc::indoc;

use crate::state::Mode;
use crate::test::{NeovimBackedTestContext, VimTestContext};
#[gpui::test]
async fn test_change_cc(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate(
        "c c",
        indoc! {"
           The quick
             brownˇ fox
           jumps over
           the lazy"},
    )
    .await
    .assert_matches();

    cx.simulate(
        "c c",
        indoc! {"
           ˇThe quick
           brown fox
           jumps over
           the lazy"},
    )
    .await
    .assert_matches();

    cx.simulate(
        "c c",
        indoc! {"
           The quick
             broˇwn fox
           jumps over
           the lazy"},
    )
    .await
    .assert_matches();
}

#[gpui::test]
async fn test_change_gg(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate(
        "c g g",
        indoc! {"
            The quick
            brownˇ fox
            jumps over
            the lazy"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "c g g",
        indoc! {"
            The quick
            brown fox
            jumps over
            the lˇazy"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "c g g",
        indoc! {"
            The qˇuick
            brown fox
            jumps over
            the lazy"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "c g g",
        indoc! {"
            ˇ
            brown fox
            jumps over
            the lazy"},
    )
    .await
    .assert_matches();
}

#[gpui::test]
async fn test_repeated_cj(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    for count in 1..=5 {
        cx.simulate_at_each_offset(
            &format!("c {count} j"),
            indoc! {"
                    ˇThe quˇickˇ browˇn
                    ˇ
                    ˇfox ˇjumpsˇ-ˇoˇver
                    ˇthe lazy dog
                    "},
        )
        .await
        .assert_matches();
    }
}

#[gpui::test]
async fn test_repeated_cl(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    for count in 1..=5 {
        cx.simulate_at_each_offset(
            &format!("c {count} l"),
            indoc! {"
                    ˇThe quˇickˇ browˇn
                    ˇ
                    ˇfox ˇjumpsˇ-ˇoˇver
                    ˇthe lazy dog
                    "},
        )
        .await
        .assert_matches();
    }
}

#[gpui::test]
async fn test_repeated_cb(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    for count in 1..=5 {
        cx.simulate_at_each_offset(
            &format!("c {count} b"),
            indoc! {"
                ˇThe quˇickˇ browˇn
                ˇ
                ˇfox ˇjumpsˇ-ˇoˇver
                ˇthe lazy dog
                "},
        )
        .await
        .assert_matches()
    }
}

#[gpui::test]
async fn test_repeated_ce(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    for count in 1..=5 {
        cx.simulate_at_each_offset(
            &format!("c {count} e"),
            indoc! {"
                    ˇThe quˇickˇ browˇn
                    ˇ
                    ˇfox ˇjumpsˇ-ˇoˇver
                    ˇthe lazy dog
                    "},
        )
        .await
        .assert_matches();
    }
}

#[gpui::test]
async fn test_change_with_selection_spanning_expanded_diff_hunk(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    let diff_base = indoc! {"
            fn main() {
                println!(\"old\");
            }
        "};

    cx.set_state(
        indoc! {"
                fn main() {
                    ˇprintln!(\"new\");
                }
            "},
        Mode::Normal,
    );
    cx.set_head_text(diff_base);
    cx.update_editor(|editor, window, cx| {
        editor.expand_all_diff_hunks(&editor::actions::ExpandAllDiffHunks, window, cx);
    });

    // Enter visual mode and move up so the selection spans from the
    // insertion (current line) into the deletion (diff base line).
    // Then press `c` which in visual mode dispatches `vim::Substitute`,
    // performing the change operation across the insertion/deletion boundary.
    cx.simulate_keystrokes("v k c");
}
