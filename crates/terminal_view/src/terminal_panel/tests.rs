use super::*;
use gpui::{TestAppContext, UpdateGlobal as _};
use project::FakeFs;
use settings::SettingsStore;
use std::num::NonZero;
use workspace::MultiWorkspace;

mod new_terminal;
mod prepare;
mod spawn;

async fn init_workspace_with_panel(
    cx: &mut TestAppContext,
) -> (gpui::WindowHandle<MultiWorkspace>, Entity<TerminalPanel>) {
    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let window_handle = cx.add_window(|window, cx| MultiWorkspace::test_new(project, window, cx));

    let terminal_panel = window_handle
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.workspace().update(cx, |workspace, cx| {
                let panel = cx.new(|cx| TerminalPanel::new(workspace, window, cx));
                workspace.add_panel(panel.clone(), window, cx);
                panel
            })
        })
        .expect("Failed to initialize workspace with terminal panel");

    (window_handle, terminal_panel)
}

fn set_max_tabs(cx: &mut TestAppContext, value: Option<usize>) {
    cx.update_global(|store: &mut SettingsStore, cx| {
        store.update_user_settings(cx, |settings| {
            settings.workspace.max_tabs = value.map(|v| NonZero::new(v).unwrap())
        });
    });
}

pub fn init_test(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let store = SettingsStore::test(cx);
        cx.set_global(store);
        theme_settings::init(theme::LoadThemes::JustBase, cx);
        editor::init(cx);
        crate::init(cx);
    });
}
