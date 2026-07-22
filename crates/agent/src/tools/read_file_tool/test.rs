use super::*;
use fs::Fs as _;
use gpui::{AppContext, TestAppContext, UpdateGlobal as _};
use project::{FakeFs, Project};
use serde_json::json;
use settings::SettingsStore;
use std::path::PathBuf;
use std::sync::Arc;
use util::path;

mod basic;
mod global_skill;
mod security;
mod symlink;
mod worktree_settings;

fn error_text(content: LanguageModelToolResultContent) -> String {
    match content {
        LanguageModelToolResultContent::Text(text) => text,
        _ => panic!("Expected text content"),
    }
}

fn init_test(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
    });
}

fn single_pixel_png() -> Vec<u8> {
    vec![
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 2,
        0, 0, 0, 144, 119, 83, 222, 0, 0, 0, 12, 73, 68, 65, 84, 8, 215, 99, 248, 15, 0, 1, 1, 1,
        0, 24, 221, 141, 176, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ]
}
