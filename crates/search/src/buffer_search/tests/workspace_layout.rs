use super::*;

#[cfg(target_os = "macos")]
#[gpui::test]
async fn test_hides_and_uses_secondary_when_in_singleton_buffer(cx: &mut TestAppContext) {
    let (editor, search_bar, cx) = init_test(cx);

    let initial_location = search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.set_active_pane_item(Some(&editor), window, cx)
    });

    assert_eq!(initial_location, ToolbarItemLocation::Secondary);

    let mut events = cx.events::<ToolbarItemEvent, BufferSearchBar>(&search_bar);

    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.dismiss(&Dismiss, window, cx);
    });

    assert_eq!(
        events.try_recv().unwrap(),
        (ToolbarItemEvent::ChangeLocation(ToolbarItemLocation::Hidden))
    );

    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.show(window, cx);
    });

    assert_eq!(
        events.try_recv().unwrap(),
        (ToolbarItemEvent::ChangeLocation(ToolbarItemLocation::Secondary))
    );
}

#[perf]
#[gpui::test]
async fn test_uses_primary_left_when_in_multi_buffer(cx: &mut TestAppContext) {
    let (editor, search_bar, cx) = init_multibuffer_test(cx);

    let initial_location = search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.set_active_pane_item(Some(&editor), window, cx)
    });

    assert_eq!(initial_location, ToolbarItemLocation::PrimaryLeft);

    let mut events = cx.events::<ToolbarItemEvent, BufferSearchBar>(&search_bar);

    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.dismiss(&Dismiss, window, cx);
    });

    assert_eq!(
        events.try_recv().unwrap(),
        (ToolbarItemEvent::ChangeLocation(ToolbarItemLocation::PrimaryLeft))
    );

    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.show(window, cx);
    });

    assert_eq!(
        events.try_recv().unwrap(),
        (ToolbarItemEvent::ChangeLocation(ToolbarItemLocation::PrimaryLeft))
    );
}

#[perf]
#[gpui::test]
async fn test_hides_and_uses_secondary_when_part_of_project_search(cx: &mut TestAppContext) {
    let (editor, search_bar, cx) = init_multibuffer_test(cx);

    editor.update(cx, |editor, _| {
        editor.set_in_project_search(true);
    });

    let initial_location = search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.set_active_pane_item(Some(&editor), window, cx)
    });

    assert_eq!(initial_location, ToolbarItemLocation::Hidden);

    let mut events = cx.events::<ToolbarItemEvent, BufferSearchBar>(&search_bar);

    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.dismiss(&Dismiss, window, cx);
    });

    assert_eq!(
        events.try_recv().unwrap(),
        (ToolbarItemEvent::ChangeLocation(ToolbarItemLocation::Hidden))
    );

    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.show(window, cx);
    });

    assert_eq!(
        events.try_recv().unwrap(),
        (ToolbarItemEvent::ChangeLocation(ToolbarItemLocation::Secondary))
    );
}

#[perf]
#[gpui::test]
async fn test_sets_collapsed_when_editor_fold_events_emitted(cx: &mut TestAppContext) {
    let (editor, search_bar, cx) = init_multibuffer_test(cx);

    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.set_active_pane_item(Some(&editor), window, cx);
    });

    editor.update_in(cx, |editor, window, cx| {
        editor.fold_all(&FoldAll, window, cx);
    });
    cx.run_until_parked();

    let is_collapsed = editor.read_with(cx, |editor, cx| editor.has_any_buffer_folded(cx));
    assert!(is_collapsed);

    editor.update_in(cx, |editor, window, cx| {
        editor.unfold_all(&UnfoldAll, window, cx);
    });
    cx.run_until_parked();

    let is_collapsed = editor.read_with(cx, |editor, cx| editor.has_any_buffer_folded(cx));
    assert!(!is_collapsed);
}

#[perf]
#[gpui::test]
async fn test_collapse_state_syncs_after_manual_buffer_fold(cx: &mut TestAppContext) {
    let (editor, search_bar, cx) = init_multibuffer_test(cx);

    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.set_active_pane_item(Some(&editor), window, cx);
    });

    // Fold all buffers via fold_all
    editor.update_in(cx, |editor, window, cx| {
        editor.fold_all(&FoldAll, window, cx);
    });
    cx.run_until_parked();

    let has_any_folded = editor.read_with(cx, |editor, cx| editor.has_any_buffer_folded(cx));
    assert!(
        has_any_folded,
        "All buffers should be folded after fold_all"
    );

    // Manually unfold one buffer (simulating a chevron click)
    let first_buffer_id = editor.read_with(cx, |editor, cx| {
        editor
            .buffer()
            .read(cx)
            .snapshot(cx)
            .excerpts()
            .nth(0)
            .unwrap()
            .context
            .start
            .buffer_id
    });
    editor.update_in(cx, |editor, _window, cx| {
        editor.unfold_buffer(first_buffer_id, cx);
    });

    let has_any_folded = editor.read_with(cx, |editor, cx| editor.has_any_buffer_folded(cx));
    assert!(
        has_any_folded,
        "Should still report folds when only one buffer is unfolded"
    );

    // Manually unfold the second buffer too
    let second_buffer_id = editor.read_with(cx, |editor, cx| {
        editor
            .buffer()
            .read(cx)
            .snapshot(cx)
            .excerpts()
            .nth(1)
            .unwrap()
            .context
            .start
            .buffer_id
    });
    editor.update_in(cx, |editor, _window, cx| {
        editor.unfold_buffer(second_buffer_id, cx);
    });

    let has_any_folded = editor.read_with(cx, |editor, cx| editor.has_any_buffer_folded(cx));
    assert!(
        !has_any_folded,
        "No folds should remain after unfolding all buffers individually"
    );

    // Manually fold one buffer back
    editor.update_in(cx, |editor, _window, cx| {
        editor.fold_buffer(first_buffer_id, cx);
    });

    let has_any_folded = editor.read_with(cx, |editor, cx| editor.has_any_buffer_folded(cx));
    assert!(
        has_any_folded,
        "Should report folds after manually folding one buffer"
    );
}
