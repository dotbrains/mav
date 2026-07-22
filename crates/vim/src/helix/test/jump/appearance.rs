use super::*;

#[gpui::test]
async fn test_helix_jump_uses_theme_label_color(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();
    cx.update(|_, cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.theme.experimental_theme_overrides = Some(ThemeStyleContent {
                    colors: ThemeColorsContent {
                        vim_helix_jump_label_foreground: Some("#00ff00".to_string()),
                        ..Default::default()
                    },
                    ..Default::default()
                });
            });
        });
    });
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();

    let configured_label_color =
        cx.update(|_, cx| cx.theme().colors().vim_helix_jump_label_foreground);
    assert_ne!(
        configured_label_color,
        cx.update(|_, cx| cx.theme().status().error)
    );
    cx.set_state("ˇalpha beta gamma", Mode::HelixNormal);

    let label_colors = cx.update_editor(|editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        let display_snapshot = &snapshot.display_snapshot;
        let buffer_snapshot = display_snapshot.buffer_snapshot();
        let selections = editor.selections.all::<Point>(display_snapshot);
        let skip_data = Vim::selection_skip_offsets(buffer_snapshot, &selections, false);
        let cursor_offset = selections
            .first()
            .map(|selection| buffer_snapshot.point_to_offset(selection.head()))
            .unwrap_or(MultiBufferOffset(0));
        let style = editor.style(cx);
        let font = style.text.font();
        let font_size = style.text.font_size.to_pixels(window.rem_size());
        let data = Vim::build_helix_jump_ui_data(
            buffer_snapshot,
            MultiBufferOffset(0),
            buffer_snapshot.len(),
            cursor_offset,
            configured_label_color,
            &skip_data,
            window.text_system(),
            font,
            font_size,
        );

        data.overlays
            .into_iter()
            .map(|overlay| overlay.label.text_color)
            .collect::<Vec<_>>()
    });

    assert!(!label_colors.is_empty());
    assert!(
        label_colors
            .into_iter()
            .all(|color| color == configured_label_color)
    );
}

#[gpui::test]
async fn test_helix_jump_input_is_case_insensitive(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();
    cx.set_state("ˇone two three", Mode::HelixNormal);

    cx.simulate_keystrokes("g w");
    let label = helix_jump_label_for_word(&mut cx, "three");
    let mut chars = label.chars();
    let first = chars
        .next()
        .expect("jump labels are two characters long")
        .to_ascii_uppercase();
    let second = chars
        .next()
        .expect("jump labels are two characters long")
        .to_ascii_uppercase();

    cx.simulate_keystrokes(&format!("{first} {second}"));

    cx.assert_state("one two «threeˇ»", Mode::HelixNormal);
    assert_eq!(cx.active_operator(), None);
}

#[gpui::test]
async fn test_helix_jump_with_unicode_words(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();
    cx.set_state("ˇcafé résumé naïve", Mode::HelixNormal);

    jump_to_word(&mut cx, "naïve");

    cx.assert_state("café résumé «naïveˇ»", Mode::HelixNormal);
    assert_eq!(cx.active_operator(), None);
}
