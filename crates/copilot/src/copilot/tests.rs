use super::*;
use fs::FakeFs;
use gpui::TestAppContext;
use language::language_settings::AllLanguageSettings;
use node_runtime::NodeRuntime;
use settings::{Settings, SettingsStore};
use util::{
    path,
    paths::PathStyle,
    rel_path::{RelPath, rel_path},
};

#[path = "tests/ai_settings.rs"]
mod ai_settings;
#[path = "tests/buffer_management.rs"]
mod buffer_management;
#[path = "tests/startup.rs"]
mod startup;
