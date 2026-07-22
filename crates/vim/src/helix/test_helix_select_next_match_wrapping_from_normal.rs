use super::*;

#[gpui::test]
async fn test_helix_select_next_match_wrapping_from_normal(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();

    // Exact repro for #51573: start in HelixNormal, search, then `v` to
    // enter HelixSelect, then `n` past last match.
    //
    // In HelixNormal, search collapses the cursor to the match start.
    // Pressing `v` expands by only one character, creating a partial
    // selection that overlaps the full match range when the search wraps.
    // The overlapping ranges must be merged (not just deduped) to avoid
    // a backward-seeking rope cursor panic.
    cx.set_state(
        indoc! {"
            searˇch term
            stuff
            search term
            other stuff
        "},
        Mode::HelixNormal,
    );
    cx.simulate_keystrokes("/ t e r m");
    cx.simulate_keystrokes("enter");
    cx.simulate_keystrokes("v");
    cx.simulate_keystrokes("n");
    cx.simulate_keystrokes("n");
    // Should not panic when wrapping past last match.
    cx.assert_state(
        indoc! {"
            search «termˇ»
            stuff
            search «termˇ»
            other stuff
        "},
        Mode::HelixSelect,
    );
}
