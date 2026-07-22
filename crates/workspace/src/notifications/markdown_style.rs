use gpui::{App, TextStyleRefinement, UnderlineStyle};
use markdown::MarkdownStyle;
use settings::Settings;
use theme_settings::ThemeSettings;
use ui::prelude::*;

pub(super) fn markdown_style(window: &Window, cx: &App) -> MarkdownStyle {
    let settings = ThemeSettings::get_global(cx);
    let ui_font_family = settings.ui_font.family.clone();
    let ui_font_fallbacks = settings.ui_font.fallbacks.clone();
    let buffer_font_family = settings.buffer_font.family.clone();
    let buffer_font_fallbacks = settings.buffer_font.fallbacks.clone();

    let mut base_text_style = window.text_style();
    base_text_style.refine(&TextStyleRefinement {
        font_family: Some(ui_font_family),
        font_fallbacks: ui_font_fallbacks,
        color: Some(cx.theme().colors().text),
        ..Default::default()
    });

    MarkdownStyle {
        base_text_style,
        selection_background_color: cx.theme().colors().element_selection_background,
        inline_code: TextStyleRefinement {
            background_color: Some(cx.theme().colors().editor_background.opacity(0.5)),
            font_family: Some(buffer_font_family),
            font_fallbacks: buffer_font_fallbacks,
            ..Default::default()
        },
        link: TextStyleRefinement {
            underline: Some(UnderlineStyle {
                thickness: px(1.),
                color: Some(cx.theme().colors().text_accent),
                wavy: false,
            }),
            ..Default::default()
        },
        ..Default::default()
    }
}
