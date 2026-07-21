use super::editor_basic::{
    auto_save_section, multibuffer_section, scrolling_section, which_key_section,
};
use super::editor_feedback::{
    drag_and_drop_selection_section, gutter_section, hover_popover_section, signature_help_section,
};
use super::editor_scroll::{minimap_section, scrollbar_section};
use super::editor_toolbar::toolbar_section;
use super::editor_vim::vim_settings_section;
use super::*;

pub(super) fn editor_page() -> SettingsPage {
    let items = concat_sections!(
        auto_save_section(),
        which_key_section(),
        multibuffer_section(),
        scrolling_section(),
        signature_help_section(),
        hover_popover_section(),
        drag_and_drop_selection_section(),
        gutter_section(),
        scrollbar_section(),
        minimap_section(),
        toolbar_section(),
        vim_settings_section(),
        language_settings_data(),
    );

    SettingsPage {
        title: "Editor",
        items: items,
    }
}
