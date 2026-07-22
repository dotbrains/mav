use std::{cell::RefCell, num::NonZeroU32};

use crate::{
    ClosePosition, ItemSettingsContent, VsCodeSettingsSource, default_settings,
    settings_content::LanguageSettingsContent, test_settings,
};

use super::*;
use fs::FakeFs;
use unindent::Unindent;
use util::rel_path::rel_path;

#[derive(Debug, PartialEq)]
struct AutoUpdateSetting {
    auto_update: bool,
}

impl Settings for AutoUpdateSetting {
    fn from_settings(content: &SettingsContent) -> Self {
        AutoUpdateSetting {
            auto_update: content.auto_update.unwrap(),
        }
    }
}

#[derive(Debug, PartialEq)]
struct ItemSettings {
    close_position: ClosePosition,
    git_status: bool,
}

impl Settings for ItemSettings {
    fn from_settings(content: &SettingsContent) -> Self {
        let content = content.tabs.clone().unwrap();
        ItemSettings {
            close_position: content.close_position.unwrap(),
            git_status: content.git_status.unwrap(),
        }
    }
}

#[derive(Debug, PartialEq)]
struct DefaultLanguageSettings {
    tab_size: NonZeroU32,
    preferred_line_length: u32,
}

impl Settings for DefaultLanguageSettings {
    fn from_settings(content: &SettingsContent) -> Self {
        let content = &content.project.all_languages.defaults;
        DefaultLanguageSettings {
            tab_size: content.tab_size.unwrap(),
            preferred_line_length: content.preferred_line_length.unwrap(),
        }
    }
}

#[derive(Debug, PartialEq)]
struct ThemeSettings {
    buffer_font_family: FontFamilyName,
    buffer_font_fallbacks: Vec<FontFamilyName>,
}

impl Settings for ThemeSettings {
    fn from_settings(content: &SettingsContent) -> Self {
        let content = content.theme.clone();
        ThemeSettings {
            buffer_font_family: content.buffer_font_family.unwrap(),
            buffer_font_fallbacks: content.buffer_font_fallbacks.unwrap(),
        }
    }
}
