use super::window_chrome::sidebar_chrome_section;
use super::window_layout::{
    layout_section, pane_modifiers_section, pane_split_direction_section, preview_tabs_section,
    window_section,
};
use super::window_tabs::{status_bar_section, tab_bar_section, tab_settings_section};
use super::*;

pub(super) fn window_and_layout_page() -> SettingsPage {
    SettingsPage {
        title: "Window & Layout",
        items: concat_sections![
            status_bar_section(),
            sidebar_chrome_section(),
            tab_bar_section(),
            tab_settings_section(),
            preview_tabs_section(),
            layout_section(),
            window_section(),
            pane_modifiers_section(),
            pane_split_direction_section(),
        ],
    }
}
