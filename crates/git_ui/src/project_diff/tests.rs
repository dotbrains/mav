use super::*;
use crate::project_diff::{self, ProjectDiff};
use collections::HashMap;
use db::indoc;
use editor::test::editor_test_context::{EditorTestContext, assert_state_with_diff};
use git::status::{TrackedStatus, UnmergedStatus, UnmergedStatusCode};
use gpui::TestAppContext;
use project::FakeFs;
use serde_json::json;
use settings::{DiffViewStyle, GitPanelGroupBy, GitPanelSortBy, SettingsStore};
use std::path::Path;
use unindent::Unindent as _;
use util::{
    path,
    rel_path::{RelPath, rel_path},
};
use workspace::MultiWorkspace;

#[ctor::ctor(unsafe)]
fn init_logger() {
    zlog::init_test();
}

pub(super) fn init_test(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let store = SettingsStore::test(cx);
        cx.set_global(store);
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.editor.diff_view_style = Some(DiffViewStyle::Unified);
            });
        });
        theme_settings::init(theme::LoadThemes::JustBase, cx);
        editor::init(cx);
        crate::init(cx);
    });
}

mod branch;
mod hunk_navigation;
mod repository_selection;
mod restore;
mod sorting;
