use git::{
    repository::repo_path,
    status::{StatusCode, UnmergedStatus, UnmergedStatusCode},
};
use gpui::{TestAppContext, UpdateGlobal, VisualTestContext, px};
use indoc::indoc;
use project::FakeFs;
use serde_json::json;
use settings::SettingsStore;
use theme::LoadThemes;
use util::path;
use util::rel_path::rel_path;

use workspace::MultiWorkspace;

use super::*;

mod commit_state;
mod diff_remote;
mod entries;
mod output_focus;
mod prompt_generation;
mod staging;
mod tree_view;
mod view_file;

fn init_test(cx: &mut gpui::TestAppContext) {
    zlog::init_test();

    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
        theme_settings::init(LoadThemes::JustBase, cx);
        language_model::init(cx);
        editor::init(cx);
        crate::init(cx);
    });
}

fn register_git_commit_language(project: &Entity<Project>, cx: &mut VisualTestContext) {
    project.read_with(cx, |project, _| {
        project.languages().add(Arc::new(language::Language::new(
            language::LanguageConfig {
                name: "Git Commit".into(),
                ..Default::default()
            },
            None,
        )));
    });
}

fn entry_index_for_repo_path(panel: &GitPanel, repo_path: &RepoPath) -> Option<usize> {
    panel.entries.iter().position(|entry| {
        entry
            .status_entry()
            .is_some_and(|entry| &entry.repo_path == repo_path)
    })
}

async fn await_git_panel_entries(panel: &Entity<GitPanel>, cx: &mut VisualTestContext) {
    let handle = cx.update_window_entity(panel, |panel, _, _| {
        std::mem::replace(&mut panel.update_visible_entries_task, Task::ready(()))
    });
    cx.executor().advance_clock(2 * UPDATE_DEBOUNCE);
    handle.await;
}

fn assert_editor_opened_with_path(
    workspace: &Entity<Workspace>,
    expected_path: &Path,
    cx: &mut VisualTestContext,
) {
    workspace.update_in(cx, |workspace, _window, cx| {
        let editor = workspace
            .item_of_type::<editor::Editor>(cx)
            .expect("Editor should exist after View File");
        let file_path = editor
            .read(cx)
            .active_buffer(cx)
            .expect("Buffer should have an active buffer")
            .read(cx)
            .file()
            .cloned()
            .expect("Buffer should have a file");
        assert_eq!(file_path.path().as_ref().as_std_path(), expected_path);
    });
}

async fn setup_git_panel_with_changes(
    cx: &mut TestAppContext,
    tree: serde_json::Value,
    status_entries: &[(&str, git::status::StatusCode)],
) -> (
    Entity<Project>,
    Entity<Workspace>,
    Entity<GitPanel>,
    VisualTestContext,
) {
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(path!("/project"), tree).await;

    if !status_entries.is_empty() {
        fs.set_status_for_repo(
            path!("/project/.git").as_ref(),
            &status_entries
                .iter()
                .map(|(path, status)| (*path, status.worktree()))
                .collect::<Vec<_>>(),
        );
    }
    let project = Project::test(fs, [Path::new(path!("/project"))], cx).await;
    let window_handle =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window_handle
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let mut cx = VisualTestContext::from_window(window_handle.into(), cx);

    cx.read(|cx| {
        project
            .read(cx)
            .worktrees(cx)
            .next()
            .unwrap()
            .read(cx)
            .as_local()
            .unwrap()
            .scan_complete()
    })
    .await;

    cx.executor().run_until_parked();

    let panel = workspace.update_in(&mut cx, GitPanel::new);
    await_git_panel_entries(&panel, &mut cx).await;

    (project, workspace, panel, cx)
}

fn assert_entry_paths(entries: &[GitListEntry], expected_paths: &[Option<&str>]) {
    assert_eq!(entries.len(), expected_paths.len());
    for (entry, expected_path) in entries.iter().zip(expected_paths) {
        assert_eq!(
            entry.status_entry().map(|status| status
                .repo_path
                .as_ref()
                .as_std_path()
                .to_string_lossy()
                .to_string()),
            expected_path.map(|s| s.to_string())
        );
    }
}
