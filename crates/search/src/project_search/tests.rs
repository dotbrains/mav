use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{self, AtomicUsize},
    },
    time::Duration,
};

use super::*;
use editor::{DisplayPoint, display_map::DisplayRow};
use gpui::{Action, TestAppContext, VisualTestContext, WindowHandle};
use language::{FakeLspAdapter, rust_lang};
use pretty_assertions::assert_eq;
use project::{FakeFs, Fs};
use serde_json::json;
use settings::{InlayHintSettingsContent, SettingsStore, ThemeColorsContent, ThemeStyleContent};
use util::{path, paths::PathStyle, rel_path::rel_path};
use util_macros::perf;
use workspace::{DeploySearch, MultiWorkspace};

mod buffer_reuse;
mod deleted_files;
mod deploy_focus;
mod deploy_options;
mod deploy_panes;
mod dismiss_modal;
mod filters;
mod focus;
mod history;
mod history_multiple_views;
mod in_directory;
mod inlays;
mod replace_all;
mod results_navigation;
mod scroll_results;
mod smartcase;

pub(super) fn init_test(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let settings = SettingsStore::test(cx);
        cx.set_global(settings);

        theme_settings::init(theme::LoadThemes::JustBase, cx);

        editor::init(cx);
        crate::init(cx);
    });
}

pub(super) fn perform_search(
    search_view: WindowHandle<ProjectSearchView>,
    text: impl Into<Arc<str>>,
    cx: &mut TestAppContext,
) {
    search_view
        .update(cx, |search_view, window, cx| {
            search_view.query_editor.update(cx, |query_editor, cx| {
                query_editor.set_text(text, window, cx)
            });
            search_view.search(cx);
        })
        .unwrap();
    // Ensure editor highlights appear after the search is done
    cx.executor()
        .advance_clock(editor::SELECTION_HIGHLIGHT_DEBOUNCE_TIMEOUT + Duration::from_millis(100));
    cx.background_executor.run_until_parked();
}
