use super::*;

#[path = "property_test/assertions.rs"]
mod assertions;
#[path = "property_test/operations.rs"]
mod operations;
#[path = "property_test/state.rs"]
mod state;

use assertions::{update_sidebar, validate_sidebar_properties};
use gpui::proptest::prelude::*;
use operations::perform_operation;
use state::{DISTRIBUTION_SLOTS, TestState};

#[gpui::property_test(config = ProptestConfig {
    cases: 20,
    ..Default::default()
})]
async fn test_sidebar_invariants(
    #[strategy = gpui::proptest::collection::vec(0u32..DISTRIBUTION_SLOTS * 10, 1..10)]
    raw_operations: Vec<u32>,
    cx: &mut TestAppContext,
) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT_PROPTEST_DB: AtomicUsize = AtomicUsize::new(0);

    let test_db_id = NEXT_PROPTEST_DB.fetch_add(1, Ordering::SeqCst);
    cx.update(|cx| {
        cx.set_global(TestTerminalMetadataDbName(format!(
            "PROPTEST_TERMINAL_THREAD_METADATA_{test_db_id}"
        )));
    });

    agent_ui::test_support::init_test(cx);
    cx.update(|cx| {
        cx.set_global(db::AppDatabase::test_new());
        cx.set_global(agent_ui::MaxIdleRetainedThreads(1));
        cx.set_global(agent_ui::thread_metadata_store::TestMetadataDbName(
            format!("PROPTEST_THREAD_METADATA_{test_db_id}"),
        ));

        ThreadStore::init_global(cx);
        ThreadMetadataStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        prompt_store::init(cx);

        // Auto-add an AgentPanel to every workspace so that implicitly
        // created workspaces (e.g. from thread activation) also have one.
        cx.observe_new(
            |workspace: &mut Workspace,
             window: Option<&mut Window>,
             cx: &mut gpui::Context<Workspace>| {
                if let Some(window) = window {
                    let panel = cx.new(|cx| AgentPanel::test_new(workspace, window, cx));
                    workspace.add_panel(panel, window, cx);
                }
            },
        )
        .detach();
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/my-project",
        serde_json::json!({
            ".git": {},
            "src": {},
        }),
    )
    .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));
    let project =
        project::Project::test(fs.clone() as Arc<dyn fs::Fs>, ["/my-project".as_ref()], cx).await;
    project.update(cx, |p, cx| p.git_scans_complete(cx)).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    let mut state = TestState::new(fs);
    let mut executed: Vec<String> = Vec::new();

    for &raw_op in &raw_operations {
        let project_group_count =
            multi_workspace.read_with(cx, |mw, _| mw.project_group_keys().len());
        let operation = state.generate_operation(raw_op, project_group_count);
        executed.push(format!("{:?}", operation));
        perform_operation(operation, &mut state, &multi_workspace, &sidebar, cx).await;
        cx.run_until_parked();

        update_sidebar(&sidebar, cx);
        cx.run_until_parked();

        let result = sidebar.read_with(cx, |sidebar, cx| validate_sidebar_properties(sidebar, cx));
        if let Err(err) = result {
            let log = executed.join("\n  ");
            panic!(
                "Property violation after step {}:\n{err}\n\nOperations:\n  {log}",
                executed.len(),
            );
        }
    }
}
