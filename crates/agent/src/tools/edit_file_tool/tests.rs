use super::*;
use crate::{ContextServerRegistry, Templates, ToolInputSender};
use fs::Fs as _;
use gpui::{AppContext as _, TestAppContext, UpdateGlobal};
use language_model::fake_provider::FakeLanguageModel;
use project::ProjectPath;
use prompt_store::ProjectContext;
use serde_json::json;
use settings::Settings;
use settings::SettingsStore;
use util::path;
use util::rel_path::{RelPath, rel_path};

#[gpui::test]

async fn setup_test_with_fs(
    cx: &mut TestAppContext,
    fs: Arc<project::FakeFs>,
    worktree_paths: &[&std::path::Path],
) -> (
    Arc<EditFileTool>,
    Entity<Project>,
    Entity<ActionLog>,
    Arc<project::FakeFs>,
    Entity<Thread>,
) {
    let project = Project::test(fs.clone(), worktree_paths.iter().copied(), cx).await;
    let language_registry = project.read_with(cx, |project, _cx| project.languages().clone());
    let context_server_registry =
        cx.new(|cx| ContextServerRegistry::new(project.read(cx).context_server_store(), cx));
    let model = Arc::new(FakeLanguageModel::default());
    let thread = cx.new(|cx| {
        crate::Thread::new(
            project.clone(),
            cx.new(|_cx| ProjectContext::default()),
            context_server_registry,
            Templates::new(),
            Some(model),
            cx,
        )
    });
    let action_log = thread.read_with(cx, |thread, _| thread.action_log().clone());
    let edit_tool = Arc::new(EditFileTool::new(
        project.clone(),
        thread.downgrade(),
        action_log.clone(),
        language_registry,
    ));
    (edit_tool, project, action_log, fs, thread)
}

async fn setup_test(
    cx: &mut TestAppContext,
    initial_tree: serde_json::Value,
) -> (
    Arc<EditFileTool>,
    Entity<Project>,
    Entity<ActionLog>,
    Arc<project::FakeFs>,
    Entity<Thread>,
) {
    init_test(cx);
    let fs = project::FakeFs::new(cx.executor());
    fs.insert_tree("/root", initial_tree).await;
    setup_test_with_fs(cx, fs, &[path!("/root").as_ref()]).await
}

fn init_test(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
        SettingsStore::update_global(cx, |store: &mut SettingsStore, cx| {
            store.update_user_settings(cx, |settings| {
                settings
                    .project
                    .all_languages
                    .defaults
                    .ensure_final_newline_on_save = Some(false);
            });
        });
    });
}

#[path = "tests/authorization_core.rs"]
mod authorization_core;
#[path = "tests/authorization_symlink.rs"]
mod authorization_symlink;
#[path = "tests/basic.rs"]
mod basic;
#[path = "tests/confirmation_title.rs"]
mod confirmation_title;
#[path = "tests/edge_cases.rs"]
mod edge_cases;
#[path = "tests/external_dirty.rs"]
mod external_dirty;
#[path = "tests/incremental_path.rs"]
mod incremental_path;
#[path = "tests/streaming_partials.rs"]
mod streaming_partials;
