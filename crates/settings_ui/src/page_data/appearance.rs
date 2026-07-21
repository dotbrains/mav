use super::*;
use super::{appearance_editor::*, appearance_fonts::*, appearance_theme::*};

pub(super) fn appearance_page() -> SettingsPage {
    let items: Box<[SettingsPageItem]> = concat_sections!(
        theme_section(),
        buffer_font_section(),
        ui_font_section(),
        agent_panel_font_section(),
        markdown_preview_font_section(),
        text_rendering_section(),
        cursor_section(),
        highlighting_section(),
        guides_section(),
    );

    SettingsPage {
        title: "Appearance",
        items,
    }
}
