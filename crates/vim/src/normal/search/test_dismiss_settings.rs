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
async fn test_search_dismiss_restores_cursor(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.set_state("ˇhello world\nfoo bar\nhello again\n", Mode::Normal);

    // Move cursor to line 2
    cx.simulate_keystrokes("j");
    cx.run_until_parked();
    cx.assert_state("hello world\nˇfoo bar\nhello again\n", Mode::Normal);

    // Open search
    cx.simulate_keystrokes("/");
    cx.run_until_parked();

    // Dismiss search with Escape - cursor should return to line 2
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    // Cursor should be restored to line 2 where it was when search was opened
    cx.assert_state("hello world\nˇfoo bar\nhello again\n", Mode::Normal);
}

#[gpui::test]
async fn test_search_dismiss_restores_cursor_no_matches(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.set_state("ˇapple\nbanana\ncherry\n", Mode::Normal);

    // Move cursor to line 2
    cx.simulate_keystrokes("j");
    cx.run_until_parked();
    cx.assert_state("apple\nˇbanana\ncherry\n", Mode::Normal);

    // Open search and type query for something that doesn't exist
    cx.simulate_keystrokes("/ n o n e x i s t e n t");
    cx.run_until_parked();

    // Dismiss search with Escape - cursor should still be at original position
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.assert_state("apple\nˇbanana\ncherry\n", Mode::Normal);
}

#[gpui::test]
async fn test_search_dismiss_after_editor_focus_does_not_restore(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.set_state("ˇhello world\nfoo bar\nhello again\n", Mode::Normal);

    // Move cursor to line 2
    cx.simulate_keystrokes("j");
    cx.run_until_parked();
    cx.assert_state("hello world\nˇfoo bar\nhello again\n", Mode::Normal);

    // Open search and type a query that matches line 3
    cx.simulate_keystrokes("/ a g a i n");
    cx.run_until_parked();

    // Simulate the editor gaining focus while search is still open
    // This represents the user clicking in the editor
    cx.update_editor(|_, window, cx| cx.focus_self(window));
    cx.run_until_parked();

    // Now dismiss the search bar directly
    cx.workspace(|workspace, window, cx| {
        let pane = workspace.active_pane().read(cx);
        if let Some(search_bar) = pane
            .toolbar()
            .read(cx)
            .item_of_type::<search::BufferSearchBar>()
        {
            search_bar.update(cx, |bar, cx| {
                bar.dismiss(&search::buffer_search::Dismiss, window, cx)
            });
        }
    });
    cx.run_until_parked();

    // Cursor should NOT be restored to line 2 (row 1) where search was opened.
    // Since the user "clicked" in the editor (by focusing it), prior_selections
    // was cleared, so dismiss should not restore the cursor.
    // The cursor should be at the match location on line 3 (row 2).
    cx.assert_state("hello world\nfoo bar\nhello ˇagain\n", Mode::Normal);
}

#[gpui::test]
async fn test_vim_search_respects_search_settings(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.update_global(|store: &mut SettingsStore, cx| {
        store.update_user_settings(cx, |settings| {
            settings.vim.get_or_insert_default().use_regex_search = Some(false);
        });
    });

    cx.set_state("ˇcontent", Mode::Normal);
    cx.simulate_keystrokes("/");
    cx.run_until_parked();

    // Verify search options are set from settings
    let search_bar = cx.workspace(|workspace, _, cx| {
        workspace
            .active_pane()
            .read(cx)
            .toolbar()
            .read(cx)
            .item_of_type::<BufferSearchBar>()
            .expect("Buffer search bar should be active")
    });

    cx.update_entity(search_bar, |bar, _window, _cx| {
        assert!(
            !bar.has_search_option(search::SearchOptions::REGEX),
            "Vim search open without regex mode"
        );
    });

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();

    cx.update_global(|store: &mut SettingsStore, cx| {
        store.update_user_settings(cx, |settings| {
            settings.vim.get_or_insert_default().use_regex_search = Some(true);
        });
    });

    cx.simulate_keystrokes("/");
    cx.run_until_parked();

    let search_bar = cx.workspace(|workspace, _, cx| {
        workspace
            .active_pane()
            .read(cx)
            .toolbar()
            .read(cx)
            .item_of_type::<BufferSearchBar>()
            .expect("Buffer search bar should be active")
    });

    cx.update_entity(search_bar, |bar, _window, _cx| {
        assert!(
            bar.has_search_option(search::SearchOptions::REGEX),
            "Vim search opens with regex mode"
        );
    });
}
