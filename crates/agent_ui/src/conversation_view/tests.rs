use acp_thread::StubAgentConnection;
use action_log::ActionLog;
use agent::{AgentTool, EditFileTool, FetchTool, TerminalTool, ToolPermissionContext};
use agent_servers::FakeAcpAgentServer;
use editor::MultiBufferOffset;
use editor::actions::Paste;
use feature_flags::FeatureFlagAppExt as _;
use fs::FakeFs;
use gpui::{ClipboardItem, EventEmitter, TestAppContext, VisualTestContext, point, size};
use parking_lot::Mutex;
use project::Project;
use serde_json::json;
use settings::SettingsStore;
use std::any::Any;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use workspace::{Item, MultiWorkspace};

use crate::agent_panel;
use crate::completion_provider::AgentContextSource;
use crate::test_support::register_test_sidebar;
use crate::thread_metadata_store::ThreadMetadataStore;

use super::*;

mod auth_error_connections;
mod harness;
mod server_fixtures;
mod session_connections;
pub(crate) use auth_error_connections::*;
pub(crate) use harness::*;
pub(crate) use server_fixtures::*;
pub(crate) use session_connections::*;

pub(crate) fn init_test(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
        // Use an isolated DB so parallel tests can't overwrite each
        // other's global keys (e.g. the last-created entry kind).
        cx.set_global(db::AppDatabase::test_new());
        ThreadMetadataStore::init_global(cx);
        theme_settings::init(theme::LoadThemes::JustBase, cx);
        editor::init(cx);
        agent_panel::init(cx);
        release_channel::init(semver::Version::new(0, 0, 0), cx);
        prompt_store::init(cx)
    });
}

pub(crate) fn active_thread(
    conversation_view: &Entity<ConversationView>,
    cx: &TestAppContext,
) -> Entity<ThreadView> {
    cx.read(|cx| {
        conversation_view
            .read(cx)
            .active_thread()
            .expect("No active thread")
            .clone()
    })
}

pub(crate) fn message_editor(
    conversation_view: &Entity<ConversationView>,
    cx: &TestAppContext,
) -> Entity<MessageEditor> {
    let thread = active_thread(conversation_view, cx);
    cx.read(|cx| thread.read(cx).message_editor.clone())
}
