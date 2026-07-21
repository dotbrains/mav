use super::*;

#[gpui::test]
fn test_preview_body_font_size_is_rem_based(cx: &mut TestAppContext) {
    ensure_theme_initialized(cx);
    let (_, cx) = cx.add_window_view(|_, _| TestWindow);
    cx.update(|window, cx| {
        let style = MarkdownStyle::themed(MarkdownFont::Preview, window, cx);
        assert!(
            matches!(style.base_text_style.font_size, AbsoluteLength::Rems(_)),
            "preview body font size must be rem-based, got {:?}",
            style.base_text_style.font_size
        );
        assert!(
            matches!(
                style.container_style.text.font_size,
                Some(AbsoluteLength::Rems(_))
            ),
            "preview container font size must be rem-based, got {:?}",
            style.container_style.text.font_size
        );
    });
}

#[gpui::test]
fn test_heading_font_sizes_are_distinct(cx: &mut TestAppContext) {
    let rendered = render_markdown("# H1\n\n## H2\n\n### H3\n\nBody text", cx);

    assert!(
        rendered.lines.len() >= 4,
        "expected at least 4 rendered lines, got {}",
        rendered.lines.len()
    );

    let h1_line_height = rendered.lines[0].layout.line_height();
    let h2_line_height = rendered.lines[1].layout.line_height();
    let h3_line_height = rendered.lines[2].layout.line_height();
    let body_line_height = rendered.lines[3].layout.line_height();

    assert!(
        h1_line_height > h2_line_height,
        "H1 line height ({h1_line_height:?}) should be greater than H2 ({h2_line_height:?})"
    );
    assert!(
        h2_line_height > h3_line_height,
        "H2 line height ({h2_line_height:?}) should be greater than H3 ({h3_line_height:?})"
    );
    assert!(
        h3_line_height > body_line_height,
        "H3 line height ({h3_line_height:?}) should be greater than body text ({body_line_height:?})"
    );
}

#[gpui::test]
fn test_editor_zoom_does_not_affect_markdown_preview(cx: &mut TestAppContext) {
    ensure_theme_initialized(cx);

    cx.update(|cx| {
        settings::SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.theme.buffer_font_size = Some(16.0.into());
                settings.theme.markdown_preview_font_size = None;
            });
        });
    });
    cx.run_until_parked();

    cx.update(|cx| {
        let before = ThemeSettings::get_global(cx).markdown_preview_font_size(cx);
        assert_eq!(before, px(16.0));

        theme_settings::increase_buffer_font_size(cx);
        theme_settings::increase_buffer_font_size(cx);
        theme_settings::increase_buffer_font_size(cx);

        assert_eq!(ThemeSettings::get_global(cx).buffer_font_size(cx), px(19.0));
        assert_eq!(
            ThemeSettings::get_global(cx).markdown_preview_font_size(cx),
            before
        );
    });
}

#[gpui::test]
fn test_markdown_preview_follows_buffer_font_size_setting_when_unset(cx: &mut TestAppContext) {
    ensure_theme_initialized(cx);

    cx.update(|cx| {
        settings::SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.theme.buffer_font_size = Some(20.0.into());
                settings.theme.markdown_preview_font_size = None;
            });
        });
    });
    cx.run_until_parked();
    cx.update(|cx| {
        assert_eq!(
            ThemeSettings::get_global(cx).markdown_preview_font_size(cx),
            px(20.0)
        );
    });

    cx.update(|cx| {
        settings::SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.theme.buffer_font_size = Some(24.0.into());
            });
        });
    });
    cx.run_until_parked();
    cx.update(|cx| {
        assert_eq!(
            ThemeSettings::get_global(cx).markdown_preview_font_size(cx),
            px(24.0)
        );
    });
}
