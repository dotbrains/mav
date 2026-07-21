use super::*;

pub(super) fn panel_editor_container(_window: &mut Window, cx: &mut App) -> Div {
    v_flex()
        .size_full()
        .gap(px(8.))
        .p_2()
        .bg(cx.theme().colors().editor_background)
}

pub(crate) fn git_commit_editor_style(font_size: gpui::Pixels, cx: &App) -> EditorStyle {
    let settings = ThemeSettings::get_global(cx);

    EditorStyle {
        background: cx.theme().colors().editor_background,
        local_player: cx.theme().players().local(),
        text: TextStyle {
            color: cx.theme().colors().text,
            font_family: settings.buffer_font.family.clone(),
            font_fallbacks: settings.buffer_font.fallbacks.clone(),
            font_features: settings.buffer_font.features.clone(),
            font_size: AbsoluteLength::from(font_size),
            font_weight: settings.buffer_font.weight,
            line_height: (font_size * settings.buffer_line_height.value()).into(),
            ..Default::default()
        },
        syntax: cx.theme().syntax().clone(),
        ..Default::default()
    }
}
