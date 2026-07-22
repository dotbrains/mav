mod tests {
    use super::*;
    use acp_thread::StubAgentConnection;
    use action_log::ActionLog;
    use agent::DbThread;
    use agent_client_protocol::schema::v1 as acp;
    use gpui::{TestAppContext, VisualTestContext};
    use project::FakeFs;
    use project::Project;
    use remote::WslConnectionOptions;
    use std::path::Path;
    use std::rc::Rc;
    use workspace::MultiWorkspace;

    fn make_db_thread(title: &str, updated_at: DateTime<Utc>) -> DbThread {
        DbThread {
            title: title.to_string().into(),
            messages: Vec::new(),
            updated_at,
            detailed_summary: None,
            initial_project_snapshot: None,
            cumulative_token_usage: Default::default(),
            request_token_usage: Default::default(),
            model: None,
            profile: None,
            subagent_context: None,
            speed: None,
            thinking_enabled: false,
            thinking_effort: None,
            draft_prompt: None,
            ui_scroll_position: None,
            sandboxed_terminal_temp_dir: None,
            sandbox_grants: Default::default(),
        }
    }

    fn make_metadata(
        session_id: &str,
        title: &str,
        updated_at: DateTime<Utc>,
        folder_paths: PathList,
    ) -> ThreadMetadata {
        ThreadMetadata {
            thread_id: ThreadId::new(),
            archived: false,
            session_id: Some(acp::SessionId::new(session_id)),
            agent_id: agent::MAV_AGENT_ID.clone(),
            title: if title.is_empty() {
                None
            } else {
                Some(title.to_string().into())
            },
            title_override: None,
            updated_at,
            created_at: Some(updated_at),
            interacted_at: None,
            worktree_paths: WorktreePaths::from_folder_paths(&folder_paths),
            remote_connection: None,
        }
    }

    fn init_test(cx: &mut TestAppContext) {
        let fs = FakeFs::new(cx.executor());
        cx.update(|cx| {
            let settings_store = settings::SettingsStore::test(cx);
            cx.set_global(settings_store);
            theme_settings::init(theme::LoadThemes::JustBase, cx);
            editor::init(cx);
            release_channel::init("0.0.0".parse().unwrap(), cx);
            prompt_store::init(cx);
            <dyn Fs>::set_global(fs, cx);
            ThreadMetadataStore::init_global(cx);
            ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
        });
        cx.run_until_parked();
    }

    fn setup_panel_with_project(
        project: Entity<Project>,
        cx: &mut TestAppContext,
    ) -> (Entity<crate::AgentPanel>, VisualTestContext) {
        let multi_workspace =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace_entity = multi_workspace
            .read_with(cx, |mw, _cx| mw.workspace().clone())
            .unwrap();
        let mut vcx = VisualTestContext::from_window(multi_workspace.into(), cx);
        let panel = workspace_entity.update_in(&mut vcx, |workspace, window, cx| {
            cx.new(|cx| crate::AgentPanel::new(workspace, window, cx))
        });
        (panel, vcx)
    }

    fn clear_thread_metadata_remote_connection_backfill(cx: &mut TestAppContext) {
        let kvp = cx.update(|cx| KeyValueStore::global(cx));
        gpui::block_on(kvp.delete_kvp("thread-metadata-remote-connection-backfill".to_string()))
            .unwrap();
    }

    fn run_store_migrations(cx: &mut TestAppContext) {
        clear_thread_metadata_remote_connection_backfill(cx);
        cx.update(|cx| {
            let migration_task = migrate_thread_metadata(cx);
            migrate_thread_remote_connections(cx, migration_task);
        });
        cx.run_until_parked();
    }

    mod archive_entries;
    mod basic;
    mod migration_tests;
    mod thread_id_migration_tests;

    mod archived_worktrees;
    mod conversation;
    mod retention;
    mod worktree_paths;
}
