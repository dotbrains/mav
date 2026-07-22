/// todo(windows)
/// The tests in this file assume that server_cx is running on Windows too.
/// We neead to find a way to test Windows-Non-Windows interactions.
use crate::headless_project::HeadlessProject;
use agent::{
    AgentTool, NativeAgent, NativeAgentConnection, ReadFileTool, ReadFileToolInput, SkillTool,
    SkillToolInput, SkillToolOutput, Templates, ThreadStore, ToolCallEventStream, ToolInput,
    skill_body_resolver_for_project, skills_resolver_for_project,
};
use client::{Client, UserStore};
use clock::FakeSystemClock;
use collections::{HashMap, HashSet};
use language_model::{LanguageModelRegistry, LanguageModelToolResultContent};
use languages::rust_lang;

use editor::{
    Editor, SelectionEffects,
    actions::{ConfirmCodeAction, ToggleCodeActions},
    code_context_menus::CodeContextMenu,
};
use extension::ExtensionHostProxy;
use fs::{FakeFs, Fs};
use git::{
    Oid,
    repository::{CommitData, Worktree as GitWorktree},
};
use gpui::{AppContext as _, Entity, SharedString, TestAppContext, UpdateGlobal, VisualContext};
use http_client::{BlockedHttpClient, FakeHttpClient};
use language::{
    Buffer, FakeLspAdapter, LanguageConfig, LanguageMatcher, LanguageRegistry, LineEnding, Point,
    language_settings::{AllLanguageSettings, LanguageSettings},
};
use lsp::{
    CompletionContext, CompletionResponse, CompletionTriggerKind, DEFAULT_LSP_REQUEST_TIMEOUT,
    LanguageServerName,
};
use node_runtime::NodeRuntime;
use project::{
    ProgressToken, Project,
    agent_server_store::AgentServerCommand,
    search::{SearchQuery, SearchResult},
};
use remote::RemoteClient;
use rpc::proto;
use serde_json::json;
use settings::{Settings, SettingsLocation, SettingsStore, initial_server_settings_content};
use smol::stream::StreamExt;
use std::{
    path::{Path, PathBuf},
    rc::Rc,
    str::FromStr,
    sync::Arc,
};
use unindent::Unindent as _;
use util::{path, path_list::PathList, paths::PathMatcher, rel_path::rel_path};

mod agent_tools;
mod basic_remote_editing;
mod code_actions;
mod entry_operations;
mod git_checkpoints;
mod git_diffs_branches;
mod git_repositories;
mod lsp_basic;
mod lsp_cancel;
mod lsp_code_lens;
mod reload_and_paths;
mod search;
mod server_settings_reconnect;
mod settings;
mod worktree_lifecycle;

mod telemetry;

fn init_logger() {
    zlog::init_test();
}

fn build_project(ssh: Entity<RemoteClient>, cx: &mut TestAppContext) -> Entity<Project> {
    cx.update(|cx| {
        if !cx.has_global::<SettingsStore>() {
            let settings_store = SettingsStore::test(cx);
            cx.set_global(settings_store);
        }
    });

    let client = cx.update(|cx| {
        Client::new(
            Arc::new(FakeSystemClock::new()),
            FakeHttpClient::with_404_response(),
            cx,
        )
    });

    let node = NodeRuntime::unavailable();
    let user_store = cx.new(|cx| UserStore::new(client.clone(), cx));
    let languages = Arc::new(LanguageRegistry::test(cx.executor()));
    let fs = FakeFs::new(cx.executor());

    cx.update(|cx| {
        Project::init(&client, cx);
    });

    cx.update(|cx| Project::remote(ssh, client, node, user_store, languages, fs, false, cx))
}
