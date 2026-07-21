#[path = "agent/command.rs"]
mod command;
#[path = "agent/command_catalog.rs"]
mod command_catalog;
#[path = "agent/connection.rs"]
mod connection;
#[path = "agent/connection_traits.rs"]
mod connection_traits;
mod db;
#[path = "agent/language_model_cache.rs"]
mod language_model_cache;
mod legacy_thread;
#[path = "agent/model_selector.rs"]
mod model_selector;
mod native_agent_server;
pub mod outline;
mod pattern_extraction;
#[path = "agent/project_context.rs"]
mod project_context;
#[path = "agent/prompt_invocation.rs"]
mod prompt_invocation;
mod sandboxing;
#[path = "agent/session_controls.rs"]
mod session_controls;
#[path = "agent/session_lifecycle.rs"]
mod session_lifecycle;
#[path = "agent/skill_catalog.rs"]
mod skill_catalog;
#[path = "agent/skills_watch.rs"]
mod skills_watch;
mod templates;
#[cfg(test)]
mod tests;
mod thread;
#[path = "agent/thread_environment.rs"]
mod thread_environment;
mod thread_store;
mod tool_permissions;
mod tools;
#[path = "agent/worktree_context.rs"]
mod worktree_context;

use command::{Command, strip_slash_command_prefix};
pub use connection::NativeAgentConnection;
pub use connection_traits::MAV_AGENT_ID;
use context_server::ContextServerId;
pub use db::*;
use itertools::Itertools;
use language_model_cache::LanguageModels;
use model_selector::NativeAgentModelSelector;
pub use native_agent_server::NativeAgentServer;
pub use pattern_extraction::*;
pub use sandboxing::{
    ThreadSandbox, sandbox_worktree_writable_paths, settings_sandbox_policy,
    settings_thread_sandbox,
};
use session_controls::{
    NativeAgentSessionList, NativeAgentSessionRetry, NativeAgentSessionSetTitle,
    NativeAgentSessionTruncate,
};
pub use shell_command_parser::extract_commands;
pub use skill_catalog::{
    NativeAvailableSkill, SkillLoadingIssue, SkillLoadingIssueKind, SkillLoadingIssuesUpdated,
    skill_body_resolver_for_project, skills_resolver_for_project,
};
use skill_catalog::{
    SkillLoadingIssueData, apply_skill_overrides, combine_skills,
    expand_project_skills_directories, project_skill_files_from_worktree, select_catalog_skills,
};
use skills_watch::SkillsState;
pub use templates::*;
pub use thread::*;
use thread_environment::NativeThreadEnvironment;
pub use thread_store::*;
pub use tool_permissions::*;
pub use tools::*;

use acp_thread::{
    AcpThread, AgentModelId, AgentModelSelector, AgentSessionInfo, AgentSessionList,
    AgentSessionListRequest, AgentSessionListResponse, ClientUserMessageId, TokenUsageRatio,
};
use agent_client_protocol::schema::v1 as acp;
use agent_skills::{
    AGENTS_DIR_NAME, MAX_SKILL_DESCRIPTIONS_SIZE, MAX_SKILL_FILE_SIZE, ProjectSkillGroup,
    SKILL_FILE_NAME, Skill, SkillIndex, SkillLoadError, SkillLoadWarning, SkillScopeId,
    SkillSource, SkillSummary, builtin_skills, global_skills_dir, load_skills_from_directory,
    parse_skill_frontmatter, project_skills_relative_path, read_skill_body_from_content,
};
use anyhow::{Context as _, Result, anyhow};
use chrono::{DateTime, Utc};
use collections::{HashMap, HashSet, IndexMap};

use fs::Fs;
use futures::channel::{mpsc, oneshot};
use futures::future::Shared;
use futures::{FutureExt as _, StreamExt as _, future};
use gpui::{
    App, AppContext, AsyncApp, Context, Entity, EntityId, SharedString, Subscription, Task,
    TaskExt, WeakEntity,
};
use language_model::{
    IconOrSvg, LanguageModel, LanguageModelId, LanguageModelProvider, LanguageModelProviderId,
    LanguageModelRegistry,
};
use project::{
    AgentId, Project, ProjectItem, ProjectPath, Worktree, WorktreeId,
    trusted_worktrees::TrustedWorktrees,
};
use prompt_store::{ProjectContext, RULES_FILE_NAMES, RulesFileContext, WorktreeContext};
use serde::{Deserialize, Serialize};
use settings::{LanguageModelSelection, Settings as _, update_settings_file};
use std::any::Any;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, LazyLock};
use util::ResultExt;
use util::path_list::PathList;
use util::rel_path::RelPath;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectSnapshot {
    pub worktree_snapshots: Vec<project::telemetry_snapshot::TelemetryWorktreeSnapshot>,
    pub timestamp: DateTime<Utc>,
}

pub struct RulesLoadingError {
    pub message: SharedString,
}

pub const COMPACT_COMMAND_NAME: &str = "compact";

/// Returns the set of MCP prompt names that must be server-qualified
/// (`/<server>.<name>`) to stay unambiguous in the slash-command popup: names
/// shared by more than one MCP prompt, or names colliding with a reserved
/// built-in command (e.g. `/compact`). A built-in always wins an unqualified
/// invocation, so colliding MCP prompts are only reachable when prefixed.
fn ambiguous_mcp_prompt_names<'a>(
    reserved: impl IntoIterator<Item = &'a str>,
    prompt_names: impl IntoIterator<Item = &'a str>,
) -> HashSet<&'a str> {
    let mut counts: HashMap<&str, usize> = HashMap::default();
    for name in reserved.into_iter().chain(prompt_names) {
        *counts.entry(name).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .filter_map(|(name, count)| (count > 1).then_some(name))
        .collect()
}

struct ProjectState {
    project: Entity<Project>,
    project_context: Entity<ProjectContext>,
    skills: Arc<Vec<Skill>>,
    skill_loading_issues: Vec<SkillLoadingIssue>,
    project_context_needs_refresh: watch::Sender<()>,
    _maintain_project_context: Task<Result<()>>,
    context_server_registry: Entity<ContextServerRegistry>,
    _subscriptions: Vec<Subscription>,
}

/// Holds both the internal Thread and the AcpThread for a session
struct Session {
    /// The internal thread that processes messages
    thread: Entity<Thread>,
    /// The ACP thread that handles protocol communication
    acp_thread: Entity<acp_thread::AcpThread>,
    project_id: EntityId,
    pending_save: Task<Result<()>>,
    _subscriptions: Vec<Subscription>,
    ref_count: usize,
}

struct PendingSession {
    task: Shared<Task<Result<Entity<AcpThread>, Arc<anyhow::Error>>>>,
    ref_count: usize,
}

/// Implemented by the UI layer to provide the ability for agent tools to create
/// sibling threads that appear in the agent panel.
///
/// `agent_ui::AgentPanel` installs an implementation of this trait on the
/// `NativeAgent` when it sets up a connection. Tools in a native-agent thread
/// then discover and use the host via `NativeThreadEnvironment`. The UI side
/// is responsible for keeping the installed host current; a host whose
/// backing UI has been torn down will fail its first request with a clear
/// error rather than being detected up front.
pub trait SiblingThreadHost {
    fn create_sibling_thread(
        &self,
        request: SiblingThreadRequest,
        cx: &mut AsyncApp,
    ) -> Task<Result<SiblingThreadInfo>>;

    fn list_available_agents(&self, cx: &mut App) -> Result<AvailableAgents>;
}

pub struct NativeAgent {
    /// Session ID -> Session mapping
    sessions: HashMap<acp::SessionId, Session>,
    pending_sessions: HashMap<acp::SessionId, PendingSession>,
    thread_store: Entity<ThreadStore>,
    /// Project-specific state keyed by project EntityId
    projects: HashMap<EntityId, ProjectState>,
    /// Shared templates for all threads
    templates: Arc<Templates>,
    /// Cached model information
    models: LanguageModels,
    /// Handler installed by the UI for `create_thread` / `list_agents_and_models` tools.
    sibling_thread_host: Option<Rc<dyn SiblingThreadHost>>,
    fs: Arc<dyn Fs>,
    _subscriptions: Vec<Subscription>,
    /// Tracks the lifecycle of global skills directory observation. We
    /// don't eagerly watch (or even check for) `~/.agents/skills/` at
    /// startup; users who never engage with the agent panel pay zero
    /// filesystem cost. The watch is kicked off lazily by
    /// [`Self::ensure_skills_scan_started`], which is called from the
    /// three agent-panel interaction points: input box focus, slash
    /// autocomplete, and conversation submit.
    skills_state: SkillsState,
}

impl gpui::EventEmitter<SkillLoadingIssuesUpdated> for NativeAgent {}

static RULES_FILE_REL_PATHS: LazyLock<Vec<Arc<RelPath>>> = LazyLock::new(|| {
    RULES_FILE_NAMES
        .iter()
        .filter_map(|name| RelPath::unix(name).ok().map(|path| path.into_arc()))
        .collect()
});

static AGENTS_PREFIX: LazyLock<Option<Arc<RelPath>>> = LazyLock::new(|| {
    RelPath::unix(AGENTS_DIR_NAME)
        .ok()
        .map(|path| path.into_arc())
});

static SKILLS_PREFIX: LazyLock<Option<Arc<RelPath>>> = LazyLock::new(|| {
    RelPath::unix(project_skills_relative_path())
        .ok()
        .map(|path| path.into_arc())
});

impl NativeAgent {
    pub fn new(
        thread_store: Entity<ThreadStore>,
        templates: Arc<Templates>,
        fs: Arc<dyn Fs>,
        cx: &mut App,
    ) -> Entity<NativeAgent> {
        log::debug!("Creating new NativeAgent");

        cx.new(|cx| {
            let subscriptions = vec![
                cx.subscribe(
                    &LanguageModelRegistry::global(cx),
                    Self::handle_models_updated_event,
                ),
                // Flush thread content on quit so an in-flight async save
                // can't leave a thread orphaned ("no thread found with ID").
                cx.on_app_quit(Self::flush_threads_on_quit),
            ];

            if !cx.has_global::<SkillIndex>() {
                cx.set_global(SkillIndex::default());
            }

            Self {
                sessions: HashMap::default(),
                pending_sessions: HashMap::default(),
                thread_store,
                projects: HashMap::default(),
                templates,
                models: LanguageModels::new(cx),
                sibling_thread_host: None,
                fs,
                _subscriptions: subscriptions,
                skills_state: SkillsState::default(),
            }
        })
    }

    pub fn set_sibling_thread_host(&mut self, host: Rc<dyn SiblingThreadHost>) {
        self.sibling_thread_host = Some(host);
    }

    pub fn sibling_thread_host(&self) -> Option<Rc<dyn SiblingThreadHost>> {
        self.sibling_thread_host.clone()
    }

    fn new_session(
        &mut self,
        project: Entity<Project>,
        cx: &mut Context<Self>,
    ) -> Entity<AcpThread> {
        let project_id = self.get_or_create_project_state(&project, cx);
        let project_state = &self.projects[&project_id];

        let registry = LanguageModelRegistry::read_global(cx);
        let available_count = registry.available_models(cx).count();
        log::debug!("Total available models: {}", available_count);

        let default_model = registry.default_model().and_then(|default_model| {
            self.models
                .model_from_id(&LanguageModels::model_id(&default_model.model))
        });
        let thread = cx.new(|cx| {
            Thread::new(
                project,
                project_state.project_context.clone(),
                project_state.context_server_registry.clone(),
                self.templates.clone(),
                default_model,
                cx,
            )
        });

        self.register_session(thread, project_id, 1, cx)
    }

    fn register_session(
        &mut self,
        thread_handle: Entity<Thread>,
        project_id: EntityId,
        ref_count: usize,
        cx: &mut Context<Self>,
    ) -> Entity<AcpThread> {
        let connection = Rc::new(NativeAgentConnection(cx.entity()));

        let thread = thread_handle.read(cx);
        let session_id = thread.id().clone();
        let parent_session_id = thread.parent_thread_id();
        let title = thread.title();
        let draft_prompt = thread.draft_prompt().map(Vec::from);
        let scroll_position = thread.ui_scroll_position();
        let token_usage = thread.latest_token_usage();
        let project = thread.project.clone();
        let action_log = thread.action_log.clone();
        let prompt_capabilities_rx = thread.prompt_capabilities_rx.clone();
        let acp_thread = cx.new(|cx| {
            let mut acp_thread = acp_thread::AcpThread::new(
                parent_session_id,
                title,
                None,
                connection,
                project.clone(),
                action_log.clone(),
                session_id.clone(),
                prompt_capabilities_rx,
                cx,
            );
            acp_thread.set_draft_prompt(draft_prompt, cx);
            acp_thread.set_ui_scroll_position(scroll_position);
            acp_thread.update_token_usage(token_usage, cx);
            acp_thread
        });

        let registry = LanguageModelRegistry::read_global(cx);
        let summarization_model = registry.thread_summary_model(cx).map(|c| c.model);

        let weak = cx.weak_entity();
        let weak_thread = thread_handle.downgrade();
        thread_handle.update(cx, |thread, cx| {
            thread.set_summarization_model(summarization_model, cx);
            thread.add_default_tools(
                Rc::new(NativeThreadEnvironment {
                    acp_thread: acp_thread.downgrade(),
                    thread: weak_thread,
                    agent: weak.clone(),
                }) as _,
                cx,
            );
            // The resolver closure reads `state.skills` at invocation
            // time, so skills added or removed by the SKILL.md watcher
            // after the thread is constructed are still visible to the
            // model — without this, the catalog and tool would drift out
            // of sync until the session was reopened.
            thread.add_tool(SkillTool::with_body_resolver(
                skills_resolver_for_project(weak.clone(), project_id),
                skill_body_resolver_for_project(project.clone(), self.fs.clone()),
            ));
        });

        let subscriptions = vec![
            cx.subscribe(&thread_handle, Self::handle_thread_title_updated),
            cx.subscribe(&thread_handle, Self::handle_thread_token_usage_updated),
            cx.observe(&thread_handle, move |this, thread, cx| {
                this.save_thread(thread, cx)
            }),
        ];

        self.sessions.insert(
            session_id,
            Session {
                thread: thread_handle,
                acp_thread: acp_thread.clone(),
                project_id,
                _subscriptions: subscriptions,
                pending_save: Task::ready(Ok(())),
                ref_count,
            },
        );

        self.update_available_commands_for_project(project_id, cx);

        acp_thread
    }

    pub fn models(&self) -> &LanguageModels {
        &self.models
    }
}

#[cfg(test)]
mod internal_tests {
    use std::path::Path;

    use super::*;
    use acp_thread::{AgentConnection, AgentModelGroupName, AgentModelInfo, MentionUri};
    use agent_settings::COMPACTION_PROMPT;
    use fs::FakeFs;
    use gpui::TestAppContext;
    use indoc::formatdoc;
    use language_model::fake_provider::{FakeLanguageModel, FakeLanguageModelProvider};
    use language_model::{
        CompletionIntent, LanguageModelCompletionEvent, LanguageModelProviderId,
        LanguageModelProviderName,
    };
    use serde_json::json;
    use settings::SettingsStore;
    use util::{path, rel_path::rel_path};

    fn make_global_skill(name: &str, description: &str) -> Skill {
        Skill {
            name: name.to_string(),
            description: description.to_string(),
            source: SkillSource::Global,
            directory_path: PathBuf::from(format!("/home/user/.agents/skills/{name}")),
            skill_file_path: PathBuf::from(format!("/home/user/.agents/skills/{name}/SKILL.md")),
            load_warnings: Vec::new(),
            disable_model_invocation: false,
            embedded_body: None,
        }
    }

    /// Filter to only user-defined (non-built-in) skills for test assertions.
    fn user_skills(skills: &[Skill]) -> Vec<&Skill> {
        skills
            .iter()
            .filter(|s| !matches!(s.source, SkillSource::BuiltIn))
            .collect()
    }

    async fn setup_native_agent_session(
        cx: &mut TestAppContext,
    ) -> (
        Rc<NativeAgentConnection>,
        Entity<NativeAgent>,
        Entity<Project>,
        Entity<AcpThread>,
    ) {
        let fs = FakeFs::new(cx.executor());
        fs.insert_tree("/", json!({ "a": {} })).await;
        let project = Project::test(fs.clone(), [Path::new("/a")], cx).await;
        let thread_store = cx.new(|cx| ThreadStore::new(cx));
        let agent = cx.update(|cx| NativeAgent::new(thread_store, Templates::new(), fs, cx));
        let connection = Rc::new(NativeAgentConnection(agent.clone()));
        let acp_thread = cx
            .update(|cx| {
                connection.clone().new_session(
                    project.clone(),
                    PathList::new(&[Path::new("/a")]),
                    cx,
                )
            })
            .await
            .unwrap();

        (connection, agent, project, acp_thread)
    }

    fn native_thread_for_session(
        agent: &Entity<NativeAgent>,
        session_id: &acp::SessionId,
        cx: &App,
    ) -> Entity<Thread> {
        agent.read_with(cx, |agent, _cx| {
            agent.sessions.get(session_id).unwrap().thread.clone()
        })
    }

    fn request_texts_after_system(
        messages: &[language_model::LanguageModelRequestMessage],
    ) -> Vec<String> {
        messages
            .iter()
            .skip(1)
            .map(language_model::LanguageModelRequestMessage::string_contents)
            .collect()
    }

    #[gpui::test]
    async fn test_compact_command_is_available(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs.clone(), [], cx).await;
        let thread_store = cx.new(|cx| ThreadStore::new(cx));
        let agent =
            cx.update(|cx| NativeAgent::new(thread_store, Templates::new(), fs.clone(), cx));

        let connection = NativeAgentConnection(agent.clone());
        let acp_thread = cx
            .update(|cx| {
                Rc::new(connection.clone()).new_session(
                    project.clone(),
                    PathList::new(&[Path::new("/")]),
                    cx,
                )
            })
            .await
            .unwrap();
        cx.run_until_parked();

        cx.update(|cx| {
            let commands = acp_thread.read(cx).available_commands();

            let compact = commands.iter().find(|command| command.name == "compact");
            let compact = compact.expect("compact command should be available");
            assert_eq!(
                acp_thread::command_category_from_meta(&compact.meta),
                Some(acp_thread::CommandCategory::Native),
            );
        });
    }

    #[gpui::test]
    async fn test_compact_prompt_routes_to_manual_compaction(cx: &mut TestAppContext) {
        init_test(cx);
        let (connection, agent, project, acp_thread) = setup_native_agent_session(cx).await;
        let session_id = cx.update(|cx| acp_thread.read(cx).session_id().clone());
        let thread = cx.update(|cx| native_thread_for_session(&agent, &session_id, cx));
        let model = Arc::new(FakeLanguageModel::default());
        let old_message_id = ClientUserMessageId::new();

        cx.update(|cx| {
            let path_style = project.read(cx).path_style(cx);
            thread.update(cx, |thread, cx| {
                thread.set_model(model.clone(), cx);
                thread.push_acp_user_block(
                    old_message_id,
                    [acp::ContentBlock::from("old user")],
                    path_style,
                    cx,
                );
                thread.push_acp_agent_block("old assistant".into(), cx);
            });
        });

        let compact_message_id = ClientUserMessageId::new();
        let prompt_task = cx.update(|cx| {
            acp_thread::AgentSessionClientUserMessageIds::prompt(
                connection.as_ref(),
                compact_message_id,
                acp::PromptRequest::new(session_id.clone(), vec!["/compact".into()]),
                cx,
            )
        });
        cx.run_until_parked();

        let request = model.pending_completions().pop().unwrap();
        assert_eq!(
            request.intent,
            Some(CompletionIntent::ThreadContextSummarization)
        );
        assert_eq!(
            request_texts_after_system(&request.messages),
            vec![
                "old user".to_string(),
                "old assistant".to_string(),
                COMPACTION_PROMPT.to_string(),
            ]
        );

        model.send_completion_stream_text_chunk(&request, "summary");
        model.end_completion_stream(&request);
        cx.run_until_parked();
        prompt_task.await.unwrap();
    }

    #[gpui::test]
    async fn test_threads_flushed_to_database_on_app_quit(cx: &mut TestAppContext) {
        init_test(cx);

        let (connection, agent, project, acp_thread) = setup_native_agent_session(cx).await;
        let session_id = cx.update(|cx| acp_thread.read(cx).session_id().clone());
        let thread = cx.update(|cx| native_thread_for_session(&agent, &session_id, cx));

        // A second session whose thread stays empty must be skipped by the
        // quit flush rather than persisted as an empty row.
        let empty_acp_thread = cx
            .update(|cx| {
                connection.clone().new_session(
                    project.clone(),
                    PathList::new(&[Path::new("/a")]),
                    cx,
                )
            })
            .await
            .unwrap();
        let empty_session_id = cx.update(|cx| empty_acp_thread.read(cx).session_id().clone());

        // Give the first thread content so it's no longer an empty draft, plus
        // an in-progress draft prompt that the flush must capture.
        cx.update(|cx| {
            let path_style = project.read(cx).path_style(cx);
            thread.update(cx, |thread, cx| {
                thread.push_acp_user_block(
                    ClientUserMessageId::new(),
                    [acp::ContentBlock::from("hello from the user")],
                    path_style,
                    cx,
                );
            });
            acp_thread.update(cx, |acp_thread, cx| {
                acp_thread
                    .set_draft_prompt(Some(vec![acp::ContentBlock::from("draft in progress")]), cx);
            });
        });
        cx.run_until_parked();

        // Reproduce the orphaned state from the bug: the sidebar metadata and
        // serialized panel still reference the session, but the per-session
        // async content save never landed, so the content row is absent.
        let database = cx.update(|cx| ThreadsDatabase::connect(cx)).await.unwrap();
        database.delete_thread(session_id.clone()).await.unwrap();
        assert!(
            database
                .load_thread(session_id.clone())
                .await
                .unwrap()
                .is_none(),
            "precondition: content row should be missing before the quit flush"
        );

        // Quit through the real shutdown path so the `on_app_quit`
        // registration is exercised, not just the flush itself.
        cx.update(|cx| cx.shutdown());

        let restored = database
            .load_thread(session_id.clone())
            .await
            .unwrap()
            .expect("thread content should be persisted to the database on quit");
        assert_eq!(
            restored.messages.len(),
            1,
            "the user message should survive the quit flush"
        );
        assert_eq!(
            restored.draft_prompt,
            Some(vec![acp::ContentBlock::from("draft in progress")]),
            "the current draft prompt should be captured by the quit flush"
        );
        assert!(
            database
                .load_thread(empty_session_id)
                .await
                .unwrap()
                .is_none(),
            "empty threads should not be persisted by the quit flush"
        );
    }

    #[test]
    fn test_ambiguous_mcp_prompt_names() {
        // Reserving the built-in `/compact` forces a same-named MCP prompt to be
        // server-qualified so it stays reachable; unique names stay bare.
        let ambiguous = ambiguous_mcp_prompt_names([COMPACT_COMMAND_NAME], ["compact", "deploy"]);
        assert!(ambiguous.contains("compact"));
        assert!(!ambiguous.contains("deploy"));

        // Without the reservation, a unique MCP prompt is left bare.
        let ambiguous = ambiguous_mcp_prompt_names([], ["compact", "deploy"]);
        assert!(ambiguous.is_empty());

        // Two MCP prompts sharing a name are both qualified regardless of
        // reservation.
        let ambiguous = ambiguous_mcp_prompt_names([], ["dup", "dup", "unique"]);
        assert!(ambiguous.contains("dup"));
        assert!(!ambiguous.contains("unique"));
    }

    #[test]
    fn test_qualified_compact_commands_are_not_native_compact() {
        let unqualified_blocks = [acp::ContentBlock::from("/compact")];
        let unqualified = Command::parse(&unqualified_blocks).unwrap();
        assert!(unqualified.is_unqualified("compact"));

        let mcp_blocks = [acp::ContentBlock::from("/server.compact")];
        let mcp_qualified = Command::parse(&mcp_blocks).unwrap();
        assert_eq!(mcp_qualified.prompt_name, "compact");
        assert_eq!(mcp_qualified.explicit_server_id, Some("server"));
        assert!(!mcp_qualified.is_unqualified("compact"));

        let skill_blocks = [acp::ContentBlock::from("/:compact")];
        let skill_qualified = Command::parse(&skill_blocks).unwrap();
        assert_eq!(skill_qualified.prompt_name, "compact");
        assert_eq!(skill_qualified.skill_scope, Some(""));
        assert!(!skill_qualified.is_unqualified("compact"));
    }

    mod skill_catalog_tests {
        use super::*;

        include!("agent_tests/skill_catalog.rs");
    }

    #[gpui::test]
    async fn test_maintaining_project_context(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        fs.insert_tree(
            "/",
            json!({
                "a": {}
            }),
        )
        .await;
        let project = Project::test(fs.clone(), [], cx).await;
        let thread_store = cx.new(|cx| ThreadStore::new(cx));
        let agent =
            cx.update(|cx| NativeAgent::new(thread_store, Templates::new(), fs.clone(), cx));

        // Creating a session registers the project and triggers context building.
        let connection = NativeAgentConnection(agent.clone());
        let _acp_thread = cx
            .update(|cx| {
                Rc::new(connection).new_session(
                    project.clone(),
                    PathList::new(&[Path::new("/")]),
                    cx,
                )
            })
            .await
            .unwrap();
        cx.run_until_parked();

        let thread = agent.read_with(cx, |agent, _cx| {
            agent.sessions.values().next().unwrap().thread.clone()
        });

        agent.read_with(cx, |agent, cx| {
            let project_id = project.entity_id();
            let state = agent.projects.get(&project_id).unwrap();
            assert_eq!(state.project_context.read(cx).worktrees, vec![]);
            assert_eq!(thread.read(cx).project_context().read(cx).worktrees, vec![]);
        });

        let worktree = project
            .update(cx, |project, cx| project.create_worktree("/a", true, cx))
            .await
            .unwrap();
        cx.run_until_parked();
        agent.read_with(cx, |agent, cx| {
            let project_id = project.entity_id();
            let state = agent.projects.get(&project_id).unwrap();
            let expected_worktrees = vec![WorktreeContext {
                root_name: "a".into(),
                abs_path: Path::new("/a").into(),
                rules_file: None,
            }];
            assert_eq!(state.project_context.read(cx).worktrees, expected_worktrees);
            assert_eq!(
                thread.read(cx).project_context().read(cx).worktrees,
                expected_worktrees
            );
        });

        // Creating `/a/.rules` updates the project context.
        fs.insert_file("/a/.rules", Vec::new()).await;
        cx.run_until_parked();
        agent.read_with(cx, |agent, cx| {
            let project_id = project.entity_id();
            let state = agent.projects.get(&project_id).unwrap();
            let rules_entry = worktree
                .read(cx)
                .entry_for_path(rel_path(".rules"))
                .unwrap();
            let expected_worktrees = vec![WorktreeContext {
                root_name: "a".into(),
                abs_path: Path::new("/a").into(),
                rules_file: Some(RulesFileContext {
                    path_in_worktree: rel_path(".rules").into(),
                    text: "".into(),
                    project_entry_id: rules_entry.id.to_usize(),
                }),
            }];
            assert_eq!(state.project_context.read(cx).worktrees, expected_worktrees);
            assert_eq!(
                thread.read(cx).project_context().read(cx).worktrees,
                expected_worktrees
            );
        });
    }

    #[gpui::test]
    async fn test_global_skills_load_and_reload(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        let skills_dir = global_skills_dir();
        let initial_skill_dir = skills_dir.join("my-skill");
        let initial_skill_path = initial_skill_dir.join("SKILL.md");
        fs.create_dir(&initial_skill_dir).await.unwrap();
        fs.insert_file(
            &initial_skill_path,
            b"---\nname: my-skill\ndescription: First version\n---\n\nbody-v1".to_vec(),
        )
        .await;

        let project = Project::test(fs.clone(), [], cx).await;
        let thread_store = cx.new(|cx| ThreadStore::new(cx));
        let agent =
            cx.update(|cx| NativeAgent::new(thread_store, Templates::new(), fs.clone(), cx));

        // Simulate the user-interaction trigger that the agent panel
        // fires (input focus, slash autocomplete, or submit). In tests
        // we call it directly because there's no panel.
        cx.update(|cx| {
            agent.update(cx, |agent, cx| agent.ensure_skills_scan_started(cx));
        });

        let connection = NativeAgentConnection(agent.clone());
        let _acp_thread = cx
            .update(|cx| {
                Rc::new(connection).new_session(
                    project.clone(),
                    PathList::new(&[Path::new("/")]),
                    cx,
                )
            })
            .await
            .unwrap();
        cx.run_until_parked();

        // The pre-existing skill should be loaded into the project state.
        agent.read_with(cx, |agent, _cx| {
            let state = agent.projects.get(&project.entity_id()).unwrap();
            let user = user_skills(&state.skills);
            assert_eq!(user.len(), 1);
            assert_eq!(user[0].name, "my-skill");
            assert_eq!(user[0].description, "First version");
        });

        // Modify the SKILL.md and verify the project context refreshes.
        fs.write(
            &initial_skill_path,
            b"---\nname: my-skill\ndescription: Second version\n---\n\nbody-v2",
        )
        .await
        .unwrap();
        cx.run_until_parked();

        agent.read_with(cx, |agent, _cx| {
            let state = agent.projects.get(&project.entity_id()).unwrap();
            let user = user_skills(&state.skills);
            assert_eq!(user.len(), 1);
            assert_eq!(user[0].description, "Second version");
        });
    }

    #[gpui::test]
    async fn test_global_skill_with_long_description_loads_with_warning(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        let skills_dir = global_skills_dir();
        let skill_dir = skills_dir.join("long-description");
        let skill_path = skill_dir.join("SKILL.md");
        let long_description = "a".repeat(agent_skills::MAX_SKILL_DESCRIPTION_LEN + 1);
        fs.create_dir(&skill_dir).await.unwrap();
        fs.insert_file(
            &skill_path,
            format!("---\nname: long-description\ndescription: {long_description}\n---\n\nbody")
                .into_bytes(),
        )
        .await;

        let project = Project::test(fs.clone(), [], cx).await;
        let project_id = project.entity_id();
        let thread_store = cx.new(|cx| ThreadStore::new(cx));
        let agent =
            cx.update(|cx| NativeAgent::new(thread_store, Templates::new(), fs.clone(), cx));

        cx.update(|cx| {
            agent.update(cx, |agent, cx| agent.ensure_skills_scan_started(cx));
        });

        let connection = NativeAgentConnection(agent.clone());
        let acp_thread = cx
            .update(|cx| {
                Rc::new(connection.clone()).new_session(
                    project.clone(),
                    PathList::new(&[Path::new("/")]),
                    cx,
                )
            })
            .await
            .unwrap();
        cx.run_until_parked();

        let loaded_skill = agent.read_with(cx, |agent, cx| {
            let state = agent.projects.get(&project_id).unwrap();
            let user = user_skills(&state.skills);
            assert_eq!(user.len(), 1);
            assert_eq!(user[0].name, "long-description");
            assert_eq!(user[0].description, long_description);

            let catalog_names: Vec<&str> = state
                .project_context
                .read(cx)
                .skills()
                .iter()
                .map(|skill| skill.name.as_str())
                .collect();
            assert!(
                catalog_names.contains(&"long-description"),
                "long-description skill should remain in the model catalog: {catalog_names:?}"
            );

            assert!(
                state.skill_loading_issues.iter().any(|issue| {
                    issue.kind == SkillLoadingIssueKind::DescriptionTooLong
                        && issue.path == skill_path
                        && issue.message.to_string().contains("1024-byte limit")
                }),
                "expected a description-length warning issue, got {:?}",
                state.skill_loading_issues
            );

            (*user[0]).clone()
        });

        let session_id = acp_thread.read_with(cx, |thread, _cx| thread.session_id().clone());
        cx.update(|cx| {
            let available_skills = connection.available_skills(&session_id, cx);
            let available_skill = available_skills
                .iter()
                .find(|skill| skill.name == "long-description")
                .expect("long-description should appear in available skills");
            assert_eq!(available_skill.description, long_description);
            assert!(
                available_skill
                    .warning
                    .as_ref()
                    .is_some_and(|warning| warning.contains("1024-byte limit")),
                "available skill should expose warning text, got {:?}",
                available_skill.warning
            );
        });

        let body = agent_skills::read_skill_body(fs.as_ref(), &loaded_skill.skill_file_path)
            .await
            .expect("body should load despite description-length warning");
        assert_eq!(body, "body");
    }

    #[gpui::test]
    async fn test_symlinked_global_skills_load_and_reload(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        let skills_dir = global_skills_dir();
        let external_skill_dir = PathBuf::from(path!("/external/my-skill"));
        let skill_link_dir = skills_dir.join("my-skill");
        let skill_link_path = skill_link_dir.join("SKILL.md");

        fs.insert_tree(
            &external_skill_dir,
            json!({
                "SKILL.md": "---\nname: my-skill\ndescription: First symlinked version\n---\n\nbody-v1"
            }),
        )
        .await;
        fs.create_dir(&skills_dir).await.unwrap();
        fs.create_symlink(&skill_link_dir, external_skill_dir)
            .await
            .unwrap();

        let project = Project::test(fs.clone(), [], cx).await;
        let project_id = project.entity_id();
        let thread_store = cx.new(|cx| ThreadStore::new(cx));
        let agent =
            cx.update(|cx| NativeAgent::new(thread_store, Templates::new(), fs.clone(), cx));

        cx.update(|cx| {
            agent.update(cx, |agent, cx| agent.ensure_skills_scan_started(cx));
        });

        let connection = NativeAgentConnection(agent.clone());
        let _acp_thread = cx
            .update(|cx| {
                Rc::new(connection).new_session(
                    project.clone(),
                    PathList::new(&[Path::new("/")]),
                    cx,
                )
            })
            .await
            .unwrap();
        cx.run_until_parked();

        let loaded_skill = agent.read_with(cx, |agent, cx| {
            let state = agent.projects.get(&project_id).unwrap();
            let user = user_skills(&state.skills);
            assert_eq!(user.len(), 1);
            assert_eq!(user[0].name, "my-skill");
            assert_eq!(user[0].description, "First symlinked version");
            assert_eq!(user[0].source, SkillSource::Global);
            assert_eq!(user[0].skill_file_path, skill_link_path);

            let catalog_skills = state.project_context.read(cx).skills();
            let catalog_skill = catalog_skills
                .iter()
                .find(|skill| skill.name == "my-skill")
                .expect("symlinked skill should be included in the model-facing catalog");
            assert_eq!(catalog_skill.description, "First symlinked version");
            assert_eq!(
                catalog_skill.location,
                skill_link_path.to_string_lossy().as_ref()
            );

            (*user[0]).clone()
        });
        let body = agent_skills::read_skill_body(fs.as_ref(), &loaded_skill.skill_file_path)
            .await
            .unwrap();
        assert_eq!(body, "body-v1");

        fs.write(
            &skill_link_path,
            b"---\nname: my-skill\ndescription: Second symlinked version\n---\n\nbody-v2",
        )
        .await
        .unwrap();
        cx.run_until_parked();

        let reloaded_skill = agent.read_with(cx, |agent, cx| {
            let state = agent.projects.get(&project_id).unwrap();
            let user = user_skills(&state.skills);
            assert_eq!(user.len(), 1);
            assert_eq!(user[0].name, "my-skill");
            assert_eq!(user[0].description, "Second symlinked version");
            assert_eq!(user[0].source, SkillSource::Global);
            assert_eq!(user[0].skill_file_path, skill_link_path);

            let catalog_skills = state.project_context.read(cx).skills();
            let catalog_skill = catalog_skills
                .iter()
                .find(|skill| skill.name == "my-skill")
                .expect("reloaded symlinked skill should be included in the model-facing catalog");
            assert_eq!(catalog_skill.description, "Second symlinked version");
            assert_eq!(
                catalog_skill.location,
                skill_link_path.to_string_lossy().as_ref()
            );

            (*user[0]).clone()
        });
        let body = agent_skills::read_skill_body(fs.as_ref(), &reloaded_skill.skill_file_path)
            .await
            .unwrap();
        assert_eq!(body, "body-v2");
    }

    #[gpui::test]
    async fn test_global_skills_dir_created_after_startup(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        let skills_dir = global_skills_dir();

        // Intentionally do NOT pre-create `skills_dir`. The first scan
        // trigger should find no directory and leave the watch state
        // idle; a later trigger after the directory is created should
        // attach to the deepest existing ancestor and react when the
        // directory is created later.

        let project = Project::test(fs.clone(), [], cx).await;
        let thread_store = cx.new(|cx| ThreadStore::new(cx));
        let agent =
            cx.update(|cx| NativeAgent::new(thread_store, Templates::new(), fs.clone(), cx));

        // First scan trigger: nothing on disk yet, state stays idle.
        cx.update(|cx| {
            agent.update(cx, |agent, cx| agent.ensure_skills_scan_started(cx));
        });

        let connection = NativeAgentConnection(agent.clone());
        let _acp_thread = cx
            .update(|cx| {
                Rc::new(connection).new_session(
                    project.clone(),
                    PathList::new(&[Path::new("/")]),
                    cx,
                )
            })
            .await
            .unwrap();
        cx.run_until_parked();

        // No skills directory exists yet, so no skills should be loaded.
        agent.read_with(cx, |agent, _cx| {
            let state = agent.projects.get(&project.entity_id()).unwrap();
            assert!(
                user_skills(&state.skills).is_empty(),
                "expected no user skills before the global skills dir exists, got {:?}",
                state.skills
            );
        });

        // Create the global skills directory and a skill within it.
        let new_skill_dir = skills_dir.join("late-skill");
        fs.create_dir(&new_skill_dir).await.unwrap();
        fs.insert_file(
            &new_skill_dir.join("SKILL.md"),
            b"---\nname: late-skill\ndescription: Created after startup\n---\n\nbody".to_vec(),
        )
        .await;

        // Fire the trigger again, simulating the user interacting with
        // the agent panel after creating the skills directory. The
        // second scan should find the directory and start the watch,
        // which refreshes project context.
        cx.update(|cx| {
            agent.update(cx, |agent, cx| agent.ensure_skills_scan_started(cx));
        });
        cx.run_until_parked();

        agent.read_with(cx, |agent, _cx| {
            let state = agent.projects.get(&project.entity_id()).unwrap();
            let user = user_skills(&state.skills);
            assert_eq!(user.len(), 1);
            assert_eq!(user[0].name, "late-skill");
            assert_eq!(user[0].description, "Created after startup");
        });
    }

    /// Regression test for the case where a skill is added (e.g. by the
    /// SKILL.md file watcher) AFTER a session is registered. The system
    /// prompt and slash-command list both read live state, so they pick
    /// up the new skill automatically. The `SkillTool` registered on the
    /// thread used to hold a stale snapshot of `state.skills` taken at
    /// thread-construction time, which meant the model would see the new
    /// skill in `<available_skills>` but get "not found" when it tried to
    /// invoke it. The fix wires the tool to a dynamic resolver closure
    /// that re-reads `state.skills` for the project on every invocation.
    #[gpui::test]
    async fn test_skills_added_after_session_visible_to_skill_tool(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        let skills_dir = global_skills_dir();

        // No skills directory exists at startup; the watcher should
        // create one and pick up SKILL.md when it's added later.
        let project = Project::test(fs.clone(), [], cx).await;
        let thread_store = cx.new(|cx| ThreadStore::new(cx));
        let agent =
            cx.update(|cx| NativeAgent::new(thread_store, Templates::new(), fs.clone(), cx));

        // First scan trigger: nothing on disk yet.
        cx.update(|cx| {
            agent.update(cx, |agent, cx| agent.ensure_skills_scan_started(cx));
        });

        let connection = NativeAgentConnection(agent.clone());
        let _acp_thread = cx
            .update(|cx| {
                Rc::new(connection).new_session(
                    project.clone(),
                    PathList::new(&[Path::new("/")]),
                    cx,
                )
            })
            .await
            .unwrap();
        cx.run_until_parked();

        let project_id = project.entity_id();
        agent.read_with(cx, |agent, _cx| {
            let state = agent.projects.get(&project_id).unwrap();
            assert!(
                user_skills(&state.skills).is_empty(),
                "expected no user skills before the global skills dir exists, got {:?}",
                state.skills
            );
        });

        // Build the same resolver closure that `register_session` uses.
        // This is the production resolver factored into a helper so the
        // test can verify resolution behavior directly without setting
        // up the full tool-call plumbing (`ToolInput`,
        // `ToolCallEventStream`, authorization channel, ...).
        let resolve =
            cx.update(|_cx| super::skills_resolver_for_project(agent.downgrade(), project_id));

        // Sanity check: before any skills exist, the resolver returns an
        // empty list — NOT the snapshot that `Thread::new` would have
        // captured.
        cx.update(|cx| {
            let all = resolve(cx);
            let user: Vec<_> = all
                .iter()
                .filter(|s| !matches!(s.source, SkillSource::BuiltIn))
                .collect();
            assert!(user.is_empty());
        });

        // Now create a SKILL.md AFTER the session was registered. With
        // the old code this would be invisible to the `SkillTool`
        // because the tool held an `Arc<Vec<Skill>>` snapshot taken at
        // thread construction time.
        let new_skill_dir = skills_dir.join("my-skill");
        fs.create_dir(&new_skill_dir).await.unwrap();
        fs.insert_file(
            &new_skill_dir.join("SKILL.md"),
            b"---\nname: my-skill\ndescription: Created after session\n---\n\nbody".to_vec(),
        )
        .await;

        // Second scan trigger: now the directory exists, so the scan
        // starts the watch and refreshes project context.
        cx.update(|cx| {
            agent.update(cx, |agent, cx| agent.ensure_skills_scan_started(cx));
        });
        cx.run_until_parked();

        // `state.skills` reflects the new skill (the watcher ran).
        agent.read_with(cx, |agent, _cx| {
            let state = agent.projects.get(&project_id).unwrap();
            let user = user_skills(&state.skills);
            assert_eq!(user.len(), 1);
            assert_eq!(user[0].name, "my-skill");
        });

        // The resolver the `SkillTool` uses must see it too. This is the
        // crux of the regression test: the tool's view of skills is
        // resolved at invocation time, not at thread-construction time.
        cx.update(|cx| {
            let all = resolve(cx);
            let snapshot: Vec<_> = all
                .iter()
                .filter(|s| !matches!(s.source, SkillSource::BuiltIn))
                .collect();
            assert_eq!(
                snapshot.len(),
                1,
                "dynamic resolver should see the new skill"
            );
            assert_eq!(snapshot[0].name, "my-skill");
            assert_eq!(snapshot[0].description, "Created after session");
        });

        // And rendering the envelope through the same path the tool uses
        // produces a `<skill_content name="my-skill">` block, confirming
        // the model would see the new skill if it invoked the tool.
        let skill_for_render = cx.update(|cx| {
            let snapshot = resolve(cx);
            snapshot
                .iter()
                .find(|s| s.name == "my-skill" && !s.disable_model_invocation)
                .cloned()
                .expect("my-skill should be model-invocable")
        });
        let body = agent_skills::read_skill_body(fs.as_ref(), &skill_for_render.skill_file_path)
            .await
            .expect("skill body should load");
        let rendered = render_skill_envelope(&skill_for_render, &body);
        assert!(
            rendered.contains("<skill_content name=\"my-skill\">"),
            "rendered envelope missing skill_content tag: {rendered}"
        );
    }

    /// Subagents must inherit access to the same skills as their parent.
    /// Production wires this up in `NativeThreadEnvironment::create_subagent_thread`,
    /// which calls `agent.register_session(subagent, project_id, ...)` —
    /// `register_session` is what installs the `SkillTool` on the thread
    /// using a resolver closure keyed on `project_id`. Because the
    /// subagent shares its parent's `project_id`, both threads end up
    /// resolving skills against the same `state.skills`.
    ///
    /// This test exercises that production path directly: it creates a
    /// parent session via the agent connection, builds a subagent thread
    /// the same way `create_subagent_thread` does, and runs it through
    /// `register_session`. It then asserts that the `SkillTool` is
    /// registered on the subagent thread and that resolving against the
    /// same `project_id` produces the same skill set the parent sees.
    #[gpui::test]
    async fn test_subagent_skills_lookup_matches_parent(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        let skills_dir = global_skills_dir();
        let skill_dir = skills_dir.join("shared-skill");
        fs.create_dir(&skill_dir).await.unwrap();
        fs.insert_file(
            &skill_dir.join("SKILL.md"),
            b"---\nname: shared-skill\ndescription: A shared skill\n---\n\nbody".to_vec(),
        )
        .await;

        let project = Project::test(fs.clone(), [], cx).await;
        let thread_store = cx.new(|cx| ThreadStore::new(cx));
        let agent =
            cx.update(|cx| NativeAgent::new(thread_store, Templates::new(), fs.clone(), cx));

        // Open a parent session through the connection, the same way
        // production does. This triggers project-context refresh which
        // populates `state.skills` for the project.
        let connection = NativeAgentConnection(agent.clone());
        let _parent_acp = cx
            .update(|cx| {
                Rc::new(connection).new_session(
                    project.clone(),
                    PathList::new(&[Path::new("/")]),
                    cx,
                )
            })
            .await
            .unwrap();
        cx.run_until_parked();

        let project_id = project.entity_id();

        // Sanity check: resolving against the parent's project sees the skill.
        let parent_resolve =
            cx.update(|_cx| super::skills_resolver_for_project(agent.downgrade(), project_id));
        cx.update(|cx| {
            let all = parent_resolve(cx);
            let parent_skills: Vec<_> = all
                .iter()
                .filter(|s| !matches!(s.source, SkillSource::BuiltIn))
                .collect();
            assert_eq!(parent_skills.len(), 1);
            assert_eq!(parent_skills[0].name, "shared-skill");
        });

        // Grab the parent thread out of the agent's session map. This
        // mirrors what `create_subagent_thread` does internally — it
        // looks up the parent session by `parent_session_id` and reads
        // its `project_id` to forward to `register_session`.
        let (parent_thread, parent_project_id) = agent.read_with(cx, |agent, _cx| {
            let session = agent
                .sessions
                .values()
                .next()
                .expect("parent session should exist");
            (session.thread.clone(), session.project_id)
        });
        assert_eq!(parent_project_id, project_id);

        // Build the subagent thread the same way
        // `NativeThreadEnvironment::create_subagent_thread` does.
        let subagent_thread = cx.update(|cx| cx.new(|cx| Thread::new_subagent(&parent_thread, cx)));

        // Run the subagent through the production registration path.
        // This is what installs the `SkillTool` on the thread.
        let _subagent_acp = agent.update(cx, |agent, cx| {
            agent.register_session(subagent_thread.clone(), parent_project_id, 1, cx)
        });

        // Verify the subagent thread has the `SkillTool` installed —
        // without `register_session`, it would not.
        subagent_thread.read_with(cx, |thread, _cx| {
            assert!(thread.is_subagent());
            assert!(
                thread.has_registered_tool(SkillTool::NAME),
                "subagent should have SkillTool registered after register_session"
            );
        });

        // The subagent's `SkillTool` is wired to a resolver closure keyed
        // on the same `project_id` the parent used, so it sees the same
        // skill set. We check this by constructing an equivalent resolver
        // against the same project_id and asserting it matches.
        let subagent_resolve = cx
            .update(|_cx| super::skills_resolver_for_project(agent.downgrade(), parent_project_id));
        cx.update(|cx| {
            let all = subagent_resolve(cx);
            let subagent_skills: Vec<_> = all
                .iter()
                .filter(|s| !matches!(s.source, SkillSource::BuiltIn))
                .collect();
            assert_eq!(subagent_skills.len(), 1);
            assert_eq!(subagent_skills[0].name, "shared-skill");
        });
    }

    #[gpui::test]
    async fn test_skills_appear_as_available_skills(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        let skills_dir = global_skills_dir();

        // Two skills: one model-invocable (default), one slash-only via
        // `disable-model-invocation: true`. Both should still appear in
        // the slash menu as first-class skills.
        let visible_dir = skills_dir.join("visible-skill");
        fs.create_dir(&visible_dir).await.unwrap();
        fs.insert_file(
            &visible_dir.join("SKILL.md"),
            b"---\nname: visible-skill\ndescription: Visible skill\n---\n\nbody".to_vec(),
        )
        .await;

        let hidden_dir = skills_dir.join("deploy");
        fs.create_dir(&hidden_dir).await.unwrap();
        fs.insert_file(
            &hidden_dir.join("SKILL.md"),
            b"---\nname: deploy\ndescription: Deploy to prod\ndisable-model-invocation: true\n---\n\nbody"
                .to_vec(),
        )
        .await;

        let project = Project::test(fs.clone(), [], cx).await;
        let thread_store = cx.new(|cx| ThreadStore::new(cx));
        let agent =
            cx.update(|cx| NativeAgent::new(thread_store, Templates::new(), fs.clone(), cx));

        let connection = NativeAgentConnection(agent.clone());
        let acp_thread = cx
            .update(|cx| {
                Rc::new(connection.clone()).new_session(
                    project.clone(),
                    PathList::new(&[Path::new("/")]),
                    cx,
                )
            })
            .await
            .unwrap();
        cx.run_until_parked();

        let project_id = project.entity_id();
        let session_id = acp_thread.read_with(cx, |thread, _cx| thread.session_id().clone());

        agent.read_with(cx, |agent, cx| {
            let commands = NativeAgent::build_available_commands_for_project(
                agent.projects.get(&project_id),
                cx,
            );
            let names: Vec<&str> = commands.iter().map(|c| c.name.as_str()).collect();
            assert!(
                !names.contains(&"visible-skill"),
                "skills should not be exposed as ACP slash commands: {names:?}"
            );
            assert!(
                !names.contains(&"deploy"),
                "slash-only skills should not be exposed as ACP slash commands: {names:?}"
            );
        });

        cx.update(|cx| {
            let skills = connection.available_skills(&session_id, cx);
            let names: Vec<&str> = skills.iter().map(|skill| skill.name.as_str()).collect();
            assert!(
                names.contains(&"visible-skill"),
                "visible skill missing from available skills: {names:?}"
            );
            assert!(
                names.contains(&"deploy"),
                "slash-only skill missing from available skills: {names:?}"
            );
        });

        // The model's catalog (ProjectContext.skills) should NOT include
        // `deploy` since it has disable_model_invocation set.
        agent.read_with(cx, |agent, cx| {
            let state = agent.projects.get(&project_id).unwrap();
            let catalog: Vec<&str> = state
                .project_context
                .read(cx)
                .skills()
                .iter()
                .map(|s| s.name.as_str())
                .collect();
            assert!(
                catalog.contains(&"visible-skill"),
                "visible skill missing from catalog: {catalog:?}"
            );
            assert!(
                !catalog.contains(&"deploy"),
                "deploy should be excluded from catalog: {catalog:?}"
            );
        });
    }

    #[gpui::test]
    async fn test_project_skills_require_worktree_trust(cx: &mut TestAppContext) {
        use collections::{HashMap, HashSet};
        use project::trusted_worktrees::{self, PathTrust, TrustedWorktrees};

        init_test(cx);
        cx.update(|cx| {
            // The trust global isn't created by `init_test`. We need it
            // for `Project::test_with_worktree_trust` to actually wire up
            // trust tracking and for our subscription in
            // `register_project_with_initial_context` to fire.
            trusted_worktrees::init(HashMap::default(), cx);
        });

        let fs = FakeFs::new(cx.executor());
        fs.insert_tree(
            "/project",
            json!({
                ".agents": {
                    "skills": {
                        "my-skill": {
                            "SKILL.md": "---\nname: my-skill\ndescription: A project skill\n---\n\nbody"
                        }
                    }
                }
            }),
        )
        .await;

        // `test_with_worktree_trust` initializes the trust system and
        // starts every worktree as restricted, mirroring production
        // behavior on a freshly opened folder.
        let project =
            Project::test_with_worktree_trust(fs.clone(), [Path::new("/project")], cx).await;
        let thread_store = cx.new(|cx| ThreadStore::new(cx));
        let agent =
            cx.update(|cx| NativeAgent::new(thread_store, Templates::new(), fs.clone(), cx));

        let connection = NativeAgentConnection(agent.clone());
        let acp_thread = cx
            .update(|cx| {
                Rc::new(connection.clone()).new_session(
                    project.clone(),
                    PathList::new(&[Path::new("/project")]),
                    cx,
                )
            })
            .await
            .unwrap();
        cx.run_until_parked();

        let project_id = project.entity_id();
        let session_id = acp_thread.read_with(cx, |thread, _cx| thread.session_id().clone());
        let worktree_id = project.read_with(cx, |project, cx| {
            project.worktrees(cx).next().unwrap().read(cx).id()
        });

        // Untrusted: project skills are excluded from the loaded list and
        // never make it into the catalog or slash commands.
        agent.read_with(cx, |agent, cx| {
            let state = agent.projects.get(&project_id).unwrap();
            assert!(
                user_skills(&state.skills).is_empty(),
                "untrusted worktree skills should not load: {:?}",
                state
                    .skills
                    .iter()
                    .map(|s| s.name.as_str())
                    .collect::<Vec<_>>()
            );
            let commands = NativeAgent::build_available_commands_for_project(Some(state), cx);
            let names: Vec<&str> = commands.iter().map(|c| c.name.as_str()).collect();
            assert!(
                !names.contains(&"my-skill"),
                "untrusted skill leaked into slash commands: {names:?}"
            );
        });

        // Granting trust should trigger a context refresh; the skill then
        // appears in both the catalog and the slash-command list.
        cx.update(|cx| {
            let trusted_worktrees = TrustedWorktrees::try_get_global(cx)
                .expect("trusted worktrees global initialized by test_with_worktree_trust");
            trusted_worktrees.update(cx, |trusted_worktrees, cx| {
                trusted_worktrees.trust(
                    &project.read(cx).worktree_store(),
                    HashSet::from_iter([PathTrust::Worktree(worktree_id)]),
                    cx,
                );
            });
        });
        cx.run_until_parked();

        agent.read_with(cx, |agent, _cx| {
            let state = agent.projects.get(&project_id).unwrap();
            let user = user_skills(&state.skills);
            let names: Vec<&str> = user.iter().map(|s| s.name.as_str()).collect();
            assert_eq!(names, vec!["my-skill"]);
        });

        cx.update(|cx| {
            let skills = connection.available_skills(&session_id, cx);
            let skill_names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
            assert!(
                skill_names.contains(&"my-skill"),
                "trusted skill should appear in available skills: {skill_names:?}"
            );
        });
    }

    /// Open a session against a freshly created project and trust its only
    /// worktree, so project-local skills load. Returns the agent, the
    /// project, and the worktree id of the project root.
    async fn open_trusted_project_skills(
        cx: &mut TestAppContext,
        fs: Arc<FakeFs>,
        root: &str,
    ) -> (Entity<NativeAgent>, Entity<Project>, WorktreeId) {
        use collections::{HashMap, HashSet};
        use project::trusted_worktrees::{self, PathTrust, TrustedWorktrees};

        cx.update(|cx| {
            trusted_worktrees::init(HashMap::default(), cx);
        });

        let project = Project::test_with_worktree_trust(fs.clone(), [Path::new(root)], cx).await;
        let thread_store = cx.new(|cx| ThreadStore::new(cx));
        let agent =
            cx.update(|cx| NativeAgent::new(thread_store, Templates::new(), fs.clone(), cx));

        let connection = NativeAgentConnection(agent.clone());
        let _acp_thread = cx
            .update(|cx| {
                Rc::new(connection).new_session(
                    project.clone(),
                    PathList::new(&[Path::new(root)]),
                    cx,
                )
            })
            .await
            .unwrap();
        cx.run_until_parked();

        let worktree_id = project.read_with(cx, |project, cx| {
            project.worktrees(cx).next().unwrap().read(cx).id()
        });
        cx.update(|cx| {
            let trusted_worktrees = TrustedWorktrees::try_get_global(cx)
                .expect("trusted worktrees global initialized by test_with_worktree_trust");
            trusted_worktrees.update(cx, |trusted_worktrees, cx| {
                trusted_worktrees.trust(
                    &project.read(cx).worktree_store(),
                    HashSet::from_iter([PathTrust::Worktree(worktree_id)]),
                    cx,
                );
            });
        });
        cx.run_until_parked();

        (agent, project, worktree_id)
    }

    /// The body resolver for a project-local skill must read the file
    /// through a project buffer rather than the local filesystem. This is
    /// what makes project skills resolvable in remote workspaces, where
    /// the `fs` the agent holds is the client's filesystem and not where
    /// the project files actually live. We prove the buffer path is used
    /// by editing the buffer in memory (without saving) and asserting the
    /// resolver returns the edited body, not the on-disk body.
    #[gpui::test]
    async fn test_project_skill_body_resolves_through_buffer(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        fs.insert_tree(
            "/project",
            json!({
                ".agents": {
                    "skills": {
                        "my-skill": {
                            "SKILL.md": "---\nname: my-skill\ndescription: A project skill\n---\n\ndisk body"
                        }
                    }
                }
            }),
        )
        .await;

        let (agent, project, worktree_id) =
            open_trusted_project_skills(cx, fs.clone(), "/project").await;
        let project_id = project.entity_id();

        let skill = agent.read_with(cx, |agent, _cx| {
            let state = agent.projects.get(&project_id).unwrap();
            user_skills(&state.skills)
                .into_iter()
                .find(|s| s.name == "my-skill")
                .cloned()
                .expect("project skill should be loaded")
        });
        assert!(matches!(skill.source, SkillSource::ProjectLocal { .. }));

        let resolver =
            cx.update(|_cx| super::skill_body_resolver_for_project(project.clone(), fs.clone()));

        let body = cx
            .update(|cx| resolver(skill.clone(), &mut cx.to_async()))
            .await
            .unwrap();
        assert_eq!(body, "disk body");

        // Edit the buffer in memory without writing to disk.
        let relative_path: Arc<RelPath> = rel_path(".agents/skills/my-skill/SKILL.md").into();
        let buffer = project
            .update(cx, |project, cx| {
                project.open_buffer((worktree_id, relative_path), cx)
            })
            .await
            .unwrap();
        buffer.update(cx, |buffer, cx| {
            buffer.set_text(
                "---\nname: my-skill\ndescription: A project skill\n---\n\nedited body",
                cx,
            );
        });

        let body = cx
            .update(|cx| resolver(skill.clone(), &mut cx.to_async()))
            .await
            .unwrap();
        assert_eq!(
            body, "edited body",
            "resolver must read the in-memory buffer, not the on-disk file"
        );
    }

    /// A project SKILL.md whose on-disk size exceeds the cap must be
    /// rejected with a size-limit error and excluded from the loaded
    /// skills, exercising the size guard in `load_project_skills`.
    #[gpui::test]
    async fn test_oversized_project_skill_reports_error(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        let oversized = format!(
            "---\nname: huge-skill\ndescription: Too big\n---\n\n{}",
            "a".repeat(MAX_SKILL_FILE_SIZE + 1)
        );
        fs.insert_tree(
            "/project",
            json!({
                ".agents": { "skills": { "huge-skill": { "SKILL.md": oversized } } }
            }),
        )
        .await;

        let (agent, project, _worktree_id) =
            open_trusted_project_skills(cx, fs.clone(), "/project").await;
        let project_id = project.entity_id();

        agent.read_with(cx, |agent, _cx| {
            let state = agent.projects.get(&project_id).unwrap();
            assert!(
                user_skills(&state.skills).is_empty(),
                "oversized skill must not load: {:?}",
                user_skills(&state.skills)
                    .iter()
                    .map(|s| s.name.as_str())
                    .collect::<Vec<_>>()
            );
            assert!(
                state
                    .skill_loading_issues
                    .iter()
                    .any(|issue| issue.kind == SkillLoadingIssueKind::LoadFailed
                        && issue.message.to_string().contains("maximum size")),
                "expected a size-limit error, got {:?}",
                state.skill_loading_issues
            );
        });
    }

    /// A malformed project SKILL.md must surface a per-skill load error
    /// without preventing sibling skills in the same worktree from
    /// loading.
    #[gpui::test]
    async fn test_malformed_project_skill_reports_error(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        fs.insert_tree(
            "/project",
            json!({
                ".agents": {
                    "skills": {
                        "good": {
                            "SKILL.md": "---\nname: good\ndescription: Fine\n---\n\nbody"
                        },
                        "bad": {
                            "SKILL.md": "this file has no frontmatter"
                        }
                    }
                }
            }),
        )
        .await;

        let (agent, project, _worktree_id) =
            open_trusted_project_skills(cx, fs.clone(), "/project").await;
        let project_id = project.entity_id();

        agent.read_with(cx, |agent, _cx| {
            let state = agent.projects.get(&project_id).unwrap();
            let names: Vec<&str> = user_skills(&state.skills)
                .iter()
                .map(|s| s.name.as_str())
                .collect();
            assert_eq!(names, vec!["good"], "only the valid skill should load");
            assert!(
                state
                    .skill_loading_issues
                    .iter()
                    .any(|issue| issue.kind == SkillLoadingIssueKind::LoadFailed
                        && issue.path.ends_with("bad/SKILL.md")),
                "expected an error for the malformed skill, got {:?}",
                state.skill_loading_issues
            );
        });
    }

    /// The skill catalog (metadata) is also loaded through project
    /// buffers, and the broadened `.agents` refresh trigger must rebuild
    /// it when files under `.agents` change. We edit the SKILL.md buffer
    /// in memory, then touch an unrelated file directly under `.agents`
    /// (not under `.agents/skills`) and assert the catalog reflects the
    /// in-memory edit. Under the previous `.agents/skills`-only trigger
    /// this refresh would not have fired.
    #[gpui::test]
    async fn test_project_skill_metadata_refreshes_from_buffer(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        fs.insert_tree(
            "/project",
            json!({
                ".agents": {
                    "skills": {
                        "my-skill": {
                            "SKILL.md": "---\nname: my-skill\ndescription: Original\n---\n\nbody"
                        }
                    }
                }
            }),
        )
        .await;

        let (agent, project, worktree_id) =
            open_trusted_project_skills(cx, fs.clone(), "/project").await;
        let project_id = project.entity_id();

        agent.read_with(cx, |agent, _cx| {
            let state = agent.projects.get(&project_id).unwrap();
            let skill = user_skills(&state.skills)
                .into_iter()
                .find(|s| s.name == "my-skill")
                .expect("skill should be loaded");
            assert_eq!(skill.description, "Original");
        });

        let relative_path: Arc<RelPath> = rel_path(".agents/skills/my-skill/SKILL.md").into();
        let buffer = project
            .update(cx, |project, cx| {
                project.open_buffer((worktree_id, relative_path), cx)
            })
            .await
            .unwrap();
        buffer.update(cx, |buffer, cx| {
            buffer.set_text(
                "---\nname: my-skill\ndescription: Edited in buffer\n---\n\nbody",
                cx,
            );
        });

        // Touch a file directly under `.agents` (not under
        // `.agents/skills`) to trigger the broadened refresh path.
        fs.insert_file("/project/.agents/marker.txt", b"hello".to_vec())
            .await;
        cx.run_until_parked();

        agent.read_with(cx, |agent, _cx| {
            let state = agent.projects.get(&project_id).unwrap();
            let skill = user_skills(&state.skills)
                .into_iter()
                .find(|s| s.name == "my-skill")
                .expect("skill should still be loaded");
            assert_eq!(
                skill.description, "Edited in buffer",
                "catalog must reflect the in-memory buffer after a refresh"
            );
        });
    }

    #[gpui::test]
    async fn test_listing_models(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        fs.insert_tree("/", json!({ "a": {}  })).await;
        let project = Project::test(fs.clone(), [], cx).await;
        let thread_store = cx.new(|cx| ThreadStore::new(cx));
        let connection = NativeAgentConnection(
            cx.update(|cx| NativeAgent::new(thread_store, Templates::new(), fs.clone(), cx)),
        );

        // Create a thread/session
        let acp_thread = cx
            .update(|cx| {
                Rc::new(connection.clone()).new_session(
                    project.clone(),
                    PathList::new(&[Path::new("/a")]),
                    cx,
                )
            })
            .await
            .unwrap();

        let session_id = cx.update(|cx| acp_thread.read(cx).session_id().clone());

        let models = cx
            .update(|cx| {
                connection
                    .model_selector(&session_id)
                    .unwrap()
                    .list_models(cx)
            })
            .await
            .unwrap();

        let acp_thread::AgentModelList::Grouped(models) = models else {
            panic!("Unexpected model group");
        };
        assert_eq!(
            models,
            IndexMap::from_iter([(
                AgentModelGroupName("Fake".into()),
                vec![AgentModelInfo {
                    id: AgentModelId::new("fake/fake"),
                    name: "Fake".into(),
                    description: None,
                    icon: Some(acp_thread::AgentModelIcon::Named(
                        ui::IconName::MavAssistant
                    )),
                    is_latest: false,
                    disabled: None,
                    cost: None,
                }]
            )])
        );
    }

    #[gpui::test]
    async fn test_model_selection_persists_to_settings(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        fs.create_dir(paths::settings_file().parent().unwrap())
            .await
            .unwrap();
        fs.insert_file(
            paths::settings_file(),
            json!({
                "agent": {
                    "default_model": {
                        "provider": "foo",
                        "model": "bar"
                    }
                }
            })
            .to_string()
            .into_bytes(),
        )
        .await;
        let project = Project::test(fs.clone(), [], cx).await;

        let thread_store = cx.new(|cx| ThreadStore::new(cx));

        // Create the agent and connection
        let agent =
            cx.update(|cx| NativeAgent::new(thread_store, Templates::new(), fs.clone(), cx));
        let connection = NativeAgentConnection(agent.clone());

        // Create a thread/session
        let acp_thread = cx
            .update(|cx| {
                Rc::new(connection.clone()).new_session(
                    project.clone(),
                    PathList::new(&[Path::new("/a")]),
                    cx,
                )
            })
            .await
            .unwrap();

        let session_id = cx.update(|cx| acp_thread.read(cx).session_id().clone());

        // Select a model
        let selector = connection.model_selector(&session_id).unwrap();
        let model_id = AgentModelId::new("fake/fake");
        cx.update(|cx| selector.select_model(model_id.clone(), cx))
            .await
            .unwrap();

        // Verify the thread has the selected model
        agent.read_with(cx, |agent, _| {
            let session = agent.sessions.get(&session_id).unwrap();
            session.thread.read_with(cx, |thread, _| {
                assert_eq!(thread.model().unwrap().id().0, "fake");
            });
        });

        cx.run_until_parked();

        // Verify settings file was updated
        let settings_content = fs.load(paths::settings_file()).await.unwrap();
        let settings_json: serde_json::Value = serde_json::from_str(&settings_content).unwrap();

        // Check that the agent settings contain the selected model
        assert_eq!(
            settings_json["agent"]["default_model"]["model"],
            json!("fake")
        );
        assert_eq!(
            settings_json["agent"]["default_model"]["provider"],
            json!("fake")
        );

        // Register a thinking model and select it.
        cx.update(|cx| {
            let thinking_model = Arc::new(FakeLanguageModel::with_id_and_thinking(
                "fake-corp",
                "fake-thinking",
                "Fake Thinking",
                true,
            ));
            let thinking_provider = Arc::new(
                FakeLanguageModelProvider::new(
                    LanguageModelProviderId::from("fake-corp".to_string()),
                    LanguageModelProviderName::from("Fake Corp".to_string()),
                )
                .with_models(vec![thinking_model]),
            );
            LanguageModelRegistry::global(cx).update(cx, |registry, cx| {
                registry.register_provider(thinking_provider, cx);
            });
        });
        agent.update(cx, |agent, cx| agent.models.refresh_list(cx));

        let selector = connection.model_selector(&session_id).unwrap();
        cx.update(|cx| selector.select_model(AgentModelId::new("fake-corp/fake-thinking"), cx))
            .await
            .unwrap();
        cx.run_until_parked();

        // Verify enable_thinking was written to settings as true.
        let settings_content = fs.load(paths::settings_file()).await.unwrap();
        let settings_json: serde_json::Value = serde_json::from_str(&settings_content).unwrap();
        assert_eq!(
            settings_json["agent"]["default_model"]["enable_thinking"],
            json!(true),
            "selecting a thinking model should persist enable_thinking: true to settings"
        );
    }

    #[gpui::test]
    async fn test_select_model_updates_thinking_enabled(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        fs.create_dir(paths::settings_file().parent().unwrap())
            .await
            .unwrap();
        fs.insert_file(paths::settings_file(), b"{}".to_vec()).await;
        let project = Project::test(fs.clone(), [], cx).await;

        let thread_store = cx.new(|cx| ThreadStore::new(cx));
        let agent =
            cx.update(|cx| NativeAgent::new(thread_store, Templates::new(), fs.clone(), cx));
        let connection = NativeAgentConnection(agent.clone());

        let acp_thread = cx
            .update(|cx| {
                Rc::new(connection.clone()).new_session(
                    project.clone(),
                    PathList::new(&[Path::new("/a")]),
                    cx,
                )
            })
            .await
            .unwrap();
        let session_id = cx.update(|cx| acp_thread.read(cx).session_id().clone());

        // Register a second provider with a thinking model.
        cx.update(|cx| {
            let thinking_model = Arc::new(FakeLanguageModel::with_id_and_thinking(
                "fake-corp",
                "fake-thinking",
                "Fake Thinking",
                true,
            ));
            let thinking_provider = Arc::new(
                FakeLanguageModelProvider::new(
                    LanguageModelProviderId::from("fake-corp".to_string()),
                    LanguageModelProviderName::from("Fake Corp".to_string()),
                )
                .with_models(vec![thinking_model]),
            );
            LanguageModelRegistry::global(cx).update(cx, |registry, cx| {
                registry.register_provider(thinking_provider, cx);
            });
        });
        // Refresh the agent's model list so it picks up the new provider.
        agent.update(cx, |agent, cx| agent.models.refresh_list(cx));

        // Thread starts with thinking_enabled = false (the default).
        agent.read_with(cx, |agent, _| {
            let session = agent.sessions.get(&session_id).unwrap();
            session.thread.read_with(cx, |thread, _| {
                assert!(!thread.thinking_enabled(), "thinking defaults to false");
            });
        });

        // Select the thinking model via select_model.
        let selector = connection.model_selector(&session_id).unwrap();
        cx.update(|cx| selector.select_model(AgentModelId::new("fake-corp/fake-thinking"), cx))
            .await
            .unwrap();

        // select_model should have enabled thinking based on the model's supports_thinking().
        agent.read_with(cx, |agent, _| {
            let session = agent.sessions.get(&session_id).unwrap();
            session.thread.read_with(cx, |thread, _| {
                assert!(
                    thread.thinking_enabled(),
                    "select_model should enable thinking when model supports it"
                );
            });
        });

        // Switch back to the non-thinking model.
        let selector = connection.model_selector(&session_id).unwrap();
        cx.update(|cx| selector.select_model(AgentModelId::new("fake/fake"), cx))
            .await
            .unwrap();

        // select_model should have disabled thinking.
        agent.read_with(cx, |agent, _| {
            let session = agent.sessions.get(&session_id).unwrap();
            session.thread.read_with(cx, |thread, _| {
                assert!(
                    !thread.thinking_enabled(),
                    "select_model should disable thinking when model does not support it"
                );
            });
        });
    }

    #[gpui::test]
    async fn test_summarization_model_survives_transient_registry_clearing(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        fs.insert_tree("/", json!({ "a": {} })).await;
        let project = Project::test(fs.clone(), [], cx).await;

        let thread_store = cx.new(|cx| ThreadStore::new(cx));
        let agent =
            cx.update(|cx| NativeAgent::new(thread_store, Templates::new(), fs.clone(), cx));
        let connection = Rc::new(NativeAgentConnection(agent.clone()));

        let acp_thread = cx
            .update(|cx| {
                connection.clone().new_session(
                    project.clone(),
                    PathList::new(&[Path::new("/a")]),
                    cx,
                )
            })
            .await
            .unwrap();
        let session_id = acp_thread.read_with(cx, |thread, _| thread.session_id().clone());

        let thread = agent.read_with(cx, |agent, _| {
            agent.sessions.get(&session_id).unwrap().thread.clone()
        });

        thread.read_with(cx, |thread, _| {
            assert!(
                thread.summarization_model().is_some(),
                "session should have a summarization model from the test registry"
            );
        });

        // Simulate what happens during a provider blip:
        // update_active_language_model_from_settings calls set_default_model(None)
        // when it can't resolve the model, clearing all fallbacks.
        cx.update(|cx| {
            LanguageModelRegistry::global(cx).update(cx, |registry, cx| {
                registry.set_default_model(None, cx);
            });
        });
        cx.run_until_parked();

        thread.read_with(cx, |thread, _| {
            assert!(
                thread.summarization_model().is_some(),
                "summarization model should survive a transient default model clearing"
            );
        });
    }

    #[gpui::test]
    async fn test_loaded_thread_preserves_thinking_enabled(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        fs.insert_tree("/", json!({ "a": {} })).await;
        let project = Project::test(fs.clone(), [path!("/a").as_ref()], cx).await;
        let thread_store = cx.new(|cx| ThreadStore::new(cx));
        let agent = cx
            .update(|cx| NativeAgent::new(thread_store.clone(), Templates::new(), fs.clone(), cx));
        let connection = Rc::new(NativeAgentConnection(agent.clone()));

        // Register a thinking model.
        let thinking_model = Arc::new(FakeLanguageModel::with_id_and_thinking(
            "fake-corp",
            "fake-thinking",
            "Fake Thinking",
            true,
        ));
        let thinking_provider = Arc::new(
            FakeLanguageModelProvider::new(
                LanguageModelProviderId::from("fake-corp".to_string()),
                LanguageModelProviderName::from("Fake Corp".to_string()),
            )
            .with_models(vec![thinking_model.clone()]),
        );
        cx.update(|cx| {
            LanguageModelRegistry::global(cx).update(cx, |registry, cx| {
                registry.register_provider(thinking_provider, cx);
            });
        });
        agent.update(cx, |agent, cx| agent.models.refresh_list(cx));

        // Create a thread and select the thinking model.
        let acp_thread = cx
            .update(|cx| {
                connection.clone().new_session(
                    project.clone(),
                    PathList::new(&[Path::new("/a")]),
                    cx,
                )
            })
            .await
            .unwrap();
        let session_id = acp_thread.read_with(cx, |thread, _| thread.session_id().clone());

        let selector = connection.model_selector(&session_id).unwrap();
        cx.update(|cx| selector.select_model(AgentModelId::new("fake-corp/fake-thinking"), cx))
            .await
            .unwrap();

        // Verify thinking is enabled after selecting the thinking model.
        let thread = agent.read_with(cx, |agent, _| {
            agent.sessions.get(&session_id).unwrap().thread.clone()
        });
        thread.read_with(cx, |thread, _| {
            assert!(
                thread.thinking_enabled(),
                "thinking should be enabled after selecting thinking model"
            );
        });

        // Send a message so the thread gets persisted.
        let send = acp_thread.update(cx, |thread, cx| thread.send(vec!["Hello".into()], cx));
        let send = cx.foreground_executor().spawn(send);
        cx.run_until_parked();

        thinking_model.send_last_completion_stream_text_chunk("Response.");
        thinking_model.end_last_completion_stream();

        send.await.unwrap();
        cx.run_until_parked();

        // Close the session so it can be reloaded from disk.
        cx.update(|cx| connection.clone().close_session(&session_id, cx))
            .await
            .unwrap();
        drop(thread);
        drop(acp_thread);
        agent.read_with(cx, |agent, _| {
            assert!(agent.sessions.is_empty());
        });

        // Reload the thread and verify thinking_enabled is still true.
        let reloaded_acp_thread = agent
            .update(cx, |agent, cx| {
                agent.open_thread(session_id.clone(), project.clone(), cx)
            })
            .await
            .unwrap();
        let reloaded_thread = agent.read_with(cx, |agent, _| {
            agent.sessions.get(&session_id).unwrap().thread.clone()
        });
        reloaded_thread.read_with(cx, |thread, _| {
            assert!(
                thread.thinking_enabled(),
                "thinking_enabled should be preserved when reloading a thread with a thinking model"
            );
        });

        drop(reloaded_acp_thread);
    }

    #[gpui::test]
    async fn test_loaded_thread_preserves_model(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        fs.insert_tree("/", json!({ "a": {} })).await;
        let project = Project::test(fs.clone(), [path!("/a").as_ref()], cx).await;
        let thread_store = cx.new(|cx| ThreadStore::new(cx));
        let agent = cx
            .update(|cx| NativeAgent::new(thread_store.clone(), Templates::new(), fs.clone(), cx));
        let connection = Rc::new(NativeAgentConnection(agent.clone()));

        // Register a model where id() != name(), like real Anthropic models
        // (e.g. id="claude-sonnet-4-5-thinking-latest", name="Claude Sonnet 4.5 Thinking").
        let model = Arc::new(FakeLanguageModel::with_id_and_thinking(
            "fake-corp",
            "custom-model-id",
            "Custom Model Display Name",
            false,
        ));
        let provider = Arc::new(
            FakeLanguageModelProvider::new(
                LanguageModelProviderId::from("fake-corp".to_string()),
                LanguageModelProviderName::from("Fake Corp".to_string()),
            )
            .with_models(vec![model.clone()]),
        );
        cx.update(|cx| {
            LanguageModelRegistry::global(cx).update(cx, |registry, cx| {
                registry.register_provider(provider, cx);
            });
        });
        agent.update(cx, |agent, cx| agent.models.refresh_list(cx));

        // Create a thread and select the model.
        let acp_thread = cx
            .update(|cx| {
                connection.clone().new_session(
                    project.clone(),
                    PathList::new(&[Path::new("/a")]),
                    cx,
                )
            })
            .await
            .unwrap();
        let session_id = acp_thread.read_with(cx, |thread, _| thread.session_id().clone());

        let selector = connection.model_selector(&session_id).unwrap();
        cx.update(|cx| selector.select_model(AgentModelId::new("fake-corp/custom-model-id"), cx))
            .await
            .unwrap();

        let thread = agent.read_with(cx, |agent, _| {
            agent.sessions.get(&session_id).unwrap().thread.clone()
        });
        thread.read_with(cx, |thread, _| {
            assert_eq!(
                thread.model().unwrap().id().0.as_ref(),
                "custom-model-id",
                "model should be set before persisting"
            );
        });

        // Send a message so the thread gets persisted.
        let send = acp_thread.update(cx, |thread, cx| thread.send(vec!["Hello".into()], cx));
        let send = cx.foreground_executor().spawn(send);
        cx.run_until_parked();

        model.send_last_completion_stream_text_chunk("Response.");
        model.end_last_completion_stream();

        send.await.unwrap();
        cx.run_until_parked();

        // Close the session so it can be reloaded from disk.
        cx.update(|cx| connection.clone().close_session(&session_id, cx))
            .await
            .unwrap();
        drop(thread);
        drop(acp_thread);
        agent.read_with(cx, |agent, _| {
            assert!(agent.sessions.is_empty());
        });

        // Reload the thread and verify the model was preserved.
        let reloaded_acp_thread = agent
            .update(cx, |agent, cx| {
                agent.open_thread(session_id.clone(), project.clone(), cx)
            })
            .await
            .unwrap();
        let reloaded_thread = agent.read_with(cx, |agent, _| {
            agent.sessions.get(&session_id).unwrap().thread.clone()
        });
        reloaded_thread.read_with(cx, |thread, _| {
            let reloaded_model = thread
                .model()
                .expect("model should be present after reload");
            assert_eq!(
                reloaded_model.id().0.as_ref(),
                "custom-model-id",
                "reloaded thread should have the same model, not fall back to the default"
            );
        });

        drop(reloaded_acp_thread);
    }

    async fn persist_thread_with_fake_corp_model(
        cx: &mut TestAppContext,
    ) -> (
        Entity<NativeAgent>,
        Rc<NativeAgentConnection>,
        Entity<Project>,
        acp::SessionId,
        Arc<FakeLanguageModelProvider>,
    ) {
        let fs = FakeFs::new(cx.executor());
        fs.insert_tree("/", json!({ "a": {} })).await;
        let project = Project::test(fs.clone(), [path!("/a").as_ref()], cx).await;
        let thread_store = cx.new(|cx| ThreadStore::new(cx));
        let agent = cx
            .update(|cx| NativeAgent::new(thread_store.clone(), Templates::new(), fs.clone(), cx));
        let connection = Rc::new(NativeAgentConnection(agent.clone()));

        let model = Arc::new(FakeLanguageModel::with_id_and_thinking(
            "fake-corp",
            "custom-model-id",
            "Custom Model Display Name",
            false,
        ));
        let provider = Arc::new(
            FakeLanguageModelProvider::new(
                LanguageModelProviderId::from("fake-corp".to_string()),
                LanguageModelProviderName::from("Fake Corp".to_string()),
            )
            .with_models(vec![model.clone()]),
        );
        cx.update(|cx| {
            LanguageModelRegistry::global(cx).update(cx, |registry, cx| {
                registry.register_provider(provider.clone(), cx);
            });
        });
        agent.update(cx, |agent, cx| agent.models.refresh_list(cx));

        let acp_thread = cx
            .update(|cx| {
                connection.clone().new_session(
                    project.clone(),
                    PathList::new(&[Path::new("/a")]),
                    cx,
                )
            })
            .await
            .unwrap();
        let session_id = acp_thread.read_with(cx, |thread, _| thread.session_id().clone());

        let selector = connection.model_selector(&session_id).unwrap();
        cx.update(|cx| selector.select_model(AgentModelId::new("fake-corp/custom-model-id"), cx))
            .await
            .unwrap();

        let send = acp_thread.update(cx, |thread, cx| thread.send(vec!["Hello".into()], cx));
        let send = cx.foreground_executor().spawn(send);
        cx.run_until_parked();
        model.send_last_completion_stream_text_chunk("Response.");
        model.end_last_completion_stream();
        send.await.unwrap();
        cx.run_until_parked();

        cx.update(|cx| connection.clone().close_session(&session_id, cx))
            .await
            .unwrap();
        drop(acp_thread);

        (agent, connection, project, session_id, provider)
    }

    fn unregister_fake_corp(cx: &mut TestAppContext) {
        cx.update(|cx| {
            LanguageModelRegistry::global(cx).update(cx, |registry, cx| {
                registry.unregister_provider(
                    LanguageModelProviderId::from("fake-corp".to_string()),
                    cx,
                );
            });
        });
    }

    #[gpui::test]
    async fn test_loaded_thread_resolves_model_when_provider_loads_late(cx: &mut TestAppContext) {
        init_test(cx);
        let (agent, _connection, project, session_id, provider) =
            persist_thread_with_fake_corp_model(cx).await;

        // Simulate a restart where the provider hasn't fetched its model list
        // yet, so the saved selection can't be resolved at load time.
        unregister_fake_corp(cx);

        let reloaded_acp_thread = agent
            .update(cx, |agent, cx| {
                agent.open_thread(session_id.clone(), project.clone(), cx)
            })
            .await
            .unwrap();
        let thread = agent.read_with(cx, |agent, _| {
            agent.sessions.get(&session_id).unwrap().thread.clone()
        });
        thread.read_with(cx, |thread, _| {
            assert!(
                thread.model().is_none(),
                "should not fall back to an unrelated model"
            );
        });

        // The original selection is persisted even while unresolved, so a save
        // during the window can't overwrite the user's choice with a fallback.
        let db_thread = thread.read_with(cx, |thread, cx| thread.to_db(cx)).await;
        let saved = db_thread.model.expect("selection should be persisted");
        assert_eq!(saved.provider, "fake-corp");
        assert_eq!(saved.model, "custom-model-id");

        cx.update(|cx| {
            LanguageModelRegistry::global(cx).update(cx, |registry, cx| {
                registry.register_provider(provider.clone(), cx);
            });
        });
        cx.run_until_parked();

        thread.read_with(cx, |thread, _| {
            assert_eq!(
                thread
                    .model()
                    .expect("model should resolve once provider loads")
                    .id()
                    .0
                    .as_ref(),
                "custom-model-id"
            );
        });

        drop(reloaded_acp_thread);
    }

    #[gpui::test]
    async fn test_explicit_model_selection_cancels_pending(cx: &mut TestAppContext) {
        init_test(cx);
        let (agent, connection, project, session_id, provider) =
            persist_thread_with_fake_corp_model(cx).await;

        unregister_fake_corp(cx);

        let reloaded_acp_thread = agent
            .update(cx, |agent, cx| {
                agent.open_thread(session_id.clone(), project.clone(), cx)
            })
            .await
            .unwrap();
        let thread = agent.read_with(cx, |agent, _| {
            agent.sessions.get(&session_id).unwrap().thread.clone()
        });
        thread.read_with(cx, |thread, _| {
            assert!(thread.model().is_none());
        });

        // The user explicitly picks a different, available model.
        let other_model = Arc::new(FakeLanguageModel::with_id_and_thinking(
            "other-corp",
            "other-model-id",
            "Other Model",
            false,
        ));
        let other_provider = Arc::new(
            FakeLanguageModelProvider::new(
                LanguageModelProviderId::from("other-corp".to_string()),
                LanguageModelProviderName::from("Other Corp".to_string()),
            )
            .with_models(vec![other_model.clone()]),
        );
        cx.update(|cx| {
            LanguageModelRegistry::global(cx).update(cx, |registry, cx| {
                registry.register_provider(other_provider, cx);
            });
        });
        cx.run_until_parked();

        let selector = connection.model_selector(&session_id).unwrap();
        cx.update(|cx| selector.select_model(AgentModelId::new("other-corp/other-model-id"), cx))
            .await
            .unwrap();

        thread.read_with(cx, |thread, _| {
            assert_eq!(thread.model().unwrap().id().0.as_ref(), "other-model-id");
        });

        // The original provider returning must not clobber the explicit choice.
        cx.update(|cx| {
            LanguageModelRegistry::global(cx).update(cx, |registry, cx| {
                registry.register_provider(provider.clone(), cx);
            });
        });
        cx.run_until_parked();

        thread.read_with(cx, |thread, _| {
            assert_eq!(
                thread.model().unwrap().id().0.as_ref(),
                "other-model-id",
                "a late provider load must not override the explicit selection"
            );
        });

        drop(reloaded_acp_thread);
    }

    #[gpui::test]
    async fn test_save_load_thread(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        fs.insert_tree(
            "/",
            json!({
                "a": {
                    "b.md": "Lorem"
                }
            }),
        )
        .await;
        let project = Project::test(fs.clone(), [path!("/a").as_ref()], cx).await;
        let thread_store = cx.new(|cx| ThreadStore::new(cx));
        let agent = cx
            .update(|cx| NativeAgent::new(thread_store.clone(), Templates::new(), fs.clone(), cx));
        let connection = Rc::new(NativeAgentConnection(agent.clone()));

        let acp_thread = cx
            .update(|cx| {
                connection
                    .clone()
                    .new_session(project.clone(), PathList::new(&[Path::new("")]), cx)
            })
            .await
            .unwrap();
        let session_id = acp_thread.read_with(cx, |thread, _| thread.session_id().clone());
        let thread = agent.read_with(cx, |agent, _| {
            agent.sessions.get(&session_id).unwrap().thread.clone()
        });

        // Ensure empty threads are not saved, even if they get mutated.
        let model = Arc::new(FakeLanguageModel::default());
        let summary_model = Arc::new(FakeLanguageModel::default());
        thread.update(cx, |thread, cx| {
            thread.set_model(model.clone(), cx);
            thread.set_summarization_model(Some(summary_model.clone()), cx);
        });
        cx.run_until_parked();
        assert_eq!(thread_entries(&thread_store, cx), vec![]);

        let send = acp_thread.update(cx, |thread, cx| {
            thread.send(
                vec![
                    "What does ".into(),
                    acp::ContentBlock::ResourceLink(acp::ResourceLink::new(
                        "b.md",
                        MentionUri::File {
                            abs_path: path!("/a/b.md").into(),
                        }
                        .to_uri()
                        .to_string(),
                    )),
                    " mean?".into(),
                ],
                cx,
            )
        });
        let send = cx.foreground_executor().spawn(send);
        cx.run_until_parked();

        model.send_last_completion_stream_text_chunk("Lorem.");
        model.send_last_completion_stream_event(LanguageModelCompletionEvent::UsageUpdate(
            language_model::TokenUsage {
                input_tokens: 150,
                output_tokens: 75,
                ..Default::default()
            },
        ));
        model.end_last_completion_stream();
        cx.run_until_parked();
        summary_model
            .send_last_completion_stream_text_chunk(&format!("Explaining {}", path!("/a/b.md")));
        summary_model.end_last_completion_stream();

        send.await.unwrap();
        let uri = MentionUri::File {
            abs_path: path!("/a/b.md").into(),
        }
        .to_uri();
        acp_thread.read_with(cx, |thread, cx| {
            assert_eq!(
                thread.to_markdown(cx),
                formatdoc! {"
                    ## User

                    What does [@b.md]({uri}) mean?

                    ## Assistant

                    Lorem.

                "}
            )
        });

        cx.run_until_parked();

        // Set a draft prompt with rich content blocks and scroll position
        // AFTER run_until_parked, so the only save that captures these
        // changes is the one performed by close_session itself.
        let draft_blocks = vec![
            acp::ContentBlock::Text(acp::TextContent::new("Check out ")),
            acp::ContentBlock::ResourceLink(acp::ResourceLink::new("b.md", uri.to_string())),
            acp::ContentBlock::Text(acp::TextContent::new(" please")),
        ];
        acp_thread.update(cx, |thread, cx| {
            thread.set_draft_prompt(Some(draft_blocks.clone()), cx);
        });
        thread.update(cx, |thread, _cx| {
            thread.set_ui_scroll_position(Some(gpui::ListOffset {
                item_ix: 5,
                offset_in_item: gpui::px(12.5),
            }));
        });

        // Close the session so it can be reloaded from disk.
        cx.update(|cx| connection.clone().close_session(&session_id, cx))
            .await
            .unwrap();
        drop(thread);
        drop(acp_thread);
        agent.read_with(cx, |agent, _| {
            assert_eq!(agent.sessions.keys().cloned().collect::<Vec<_>>(), []);
        });

        // Ensure the thread can be reloaded from disk.
        assert_eq!(
            thread_entries(&thread_store, cx),
            vec![(
                session_id.clone(),
                format!("Explaining {}", path!("/a/b.md"))
            )]
        );
        let acp_thread = agent
            .update(cx, |agent, cx| {
                agent.open_thread(session_id.clone(), project.clone(), cx)
            })
            .await
            .unwrap();
        acp_thread.read_with(cx, |thread, cx| {
            assert_eq!(
                thread.to_markdown(cx),
                formatdoc! {"
                    ## User

                    What does [@b.md]({uri}) mean?

                    ## Assistant

                    Lorem.

                "}
            )
        });

        // Ensure the draft prompt with rich content blocks survived the round-trip.
        acp_thread.read_with(cx, |thread, _| {
            assert_eq!(thread.draft_prompt(), Some(draft_blocks.as_slice()));
        });

        // Ensure token usage survived the round-trip.
        acp_thread.read_with(cx, |thread, _| {
            let usage = thread
                .token_usage()
                .expect("token usage should be restored after reload");
            assert_eq!(usage.input_tokens, 150);
            assert_eq!(usage.output_tokens, 75);
        });

        // Ensure scroll position survived the round-trip.
        acp_thread.read_with(cx, |thread, _| {
            let scroll = thread
                .ui_scroll_position()
                .expect("scroll position should be restored after reload");
            assert_eq!(scroll.item_ix, 5);
            assert_eq!(scroll.offset_in_item, gpui::px(12.5));
        });
    }

    #[gpui::test]
    async fn test_close_session_saves_thread(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        fs.insert_tree(
            "/",
            json!({
                "a": {
                    "file.txt": "hello"
                }
            }),
        )
        .await;
        let project = Project::test(fs.clone(), [path!("/a").as_ref()], cx).await;
        let thread_store = cx.new(|cx| ThreadStore::new(cx));
        let agent = cx
            .update(|cx| NativeAgent::new(thread_store.clone(), Templates::new(), fs.clone(), cx));
        let connection = Rc::new(NativeAgentConnection(agent.clone()));

        let acp_thread = cx
            .update(|cx| {
                connection
                    .clone()
                    .new_session(project.clone(), PathList::new(&[Path::new("")]), cx)
            })
            .await
            .unwrap();
        let session_id = acp_thread.read_with(cx, |thread, _| thread.session_id().clone());
        let thread = agent.read_with(cx, |agent, _| {
            agent.sessions.get(&session_id).unwrap().thread.clone()
        });

        let model = Arc::new(FakeLanguageModel::default());
        thread.update(cx, |thread, cx| {
            thread.set_model(model.clone(), cx);
        });

        // Send a message so the thread is non-empty (empty threads aren't saved).
        let send = acp_thread.update(cx, |thread, cx| thread.send(vec!["hello".into()], cx));
        let send = cx.foreground_executor().spawn(send);
        cx.run_until_parked();

        model.send_last_completion_stream_text_chunk("world");
        model.end_last_completion_stream();
        send.await.unwrap();
        cx.run_until_parked();

        // Set a draft prompt WITHOUT calling run_until_parked afterwards.
        // This means no observe-triggered save has run for this change.
        // The only way this data gets persisted is if close_session
        // itself performs the save.
        let draft_blocks = vec![acp::ContentBlock::Text(acp::TextContent::new(
            "unsaved draft",
        ))];
        acp_thread.update(cx, |thread, cx| {
            thread.set_draft_prompt(Some(draft_blocks.clone()), cx);
        });

        // Close the session immediately — no run_until_parked in between.
        cx.update(|cx| connection.clone().close_session(&session_id, cx))
            .await
            .unwrap();
        cx.run_until_parked();

        // Reopen and verify the draft prompt was saved.
        let reloaded = agent
            .update(cx, |agent, cx| {
                agent.open_thread(session_id.clone(), project.clone(), cx)
            })
            .await
            .unwrap();
        reloaded.read_with(cx, |thread, _| {
            assert_eq!(
                thread.draft_prompt(),
                Some(draft_blocks.as_slice()),
                "close_session must save the thread; draft prompt was lost"
            );
        });
    }

    #[gpui::test]
    async fn test_thread_summary_releases_loaded_session(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        fs.insert_tree(
            "/",
            json!({
                "a": {
                    "file.txt": "hello"
                }
            }),
        )
        .await;
        let project = Project::test(fs.clone(), [path!("/a").as_ref()], cx).await;
        let thread_store = cx.new(|cx| ThreadStore::new(cx));
        let agent = cx
            .update(|cx| NativeAgent::new(thread_store.clone(), Templates::new(), fs.clone(), cx));
        let connection = Rc::new(NativeAgentConnection(agent.clone()));

        let acp_thread = cx
            .update(|cx| {
                connection
                    .clone()
                    .new_session(project.clone(), PathList::new(&[Path::new("")]), cx)
            })
            .await
            .unwrap();
        let session_id = acp_thread.read_with(cx, |thread, _| thread.session_id().clone());
        let thread = agent.read_with(cx, |agent, _| {
            agent.sessions.get(&session_id).unwrap().thread.clone()
        });

        let model = Arc::new(FakeLanguageModel::default());
        let summary_model = Arc::new(FakeLanguageModel::default());
        thread.update(cx, |thread, cx| {
            thread.set_model(model.clone(), cx);
            thread.set_summarization_model(Some(summary_model.clone()), cx);
        });

        let send = acp_thread.update(cx, |thread, cx| thread.send(vec!["hello".into()], cx));
        let send = cx.foreground_executor().spawn(send);
        cx.run_until_parked();

        model.send_last_completion_stream_text_chunk("world");
        model.end_last_completion_stream();
        send.await.unwrap();
        cx.run_until_parked();

        let summary = agent.update(cx, |agent, cx| {
            agent.thread_summary(session_id.clone(), project.clone(), cx)
        });
        cx.run_until_parked();

        summary_model.send_last_completion_stream_text_chunk("summary");
        summary_model.end_last_completion_stream();

        assert_eq!(summary.await.unwrap(), "summary");
        cx.run_until_parked();

        agent.read_with(cx, |agent, _| {
            let session = agent
                .sessions
                .get(&session_id)
                .expect("thread_summary should not close the active session");
            assert_eq!(
                session.ref_count, 1,
                "thread_summary should release its temporary session reference"
            );
        });

        cx.update(|cx| connection.clone().close_session(&session_id, cx))
            .await
            .unwrap();
        cx.run_until_parked();

        agent.read_with(cx, |agent, _| {
            assert!(
                agent.sessions.is_empty(),
                "closing the active session after thread_summary should unload it"
            );
        });
    }

    #[gpui::test]
    async fn test_loaded_sessions_keep_state_until_last_close(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        fs.insert_tree(
            "/",
            json!({
                "a": {
                    "file.txt": "hello"
                }
            }),
        )
        .await;
        let project = Project::test(fs.clone(), [path!("/a").as_ref()], cx).await;
        let thread_store = cx.new(|cx| ThreadStore::new(cx));
        let agent = cx
            .update(|cx| NativeAgent::new(thread_store.clone(), Templates::new(), fs.clone(), cx));
        let connection = Rc::new(NativeAgentConnection(agent.clone()));

        let acp_thread = cx
            .update(|cx| {
                connection
                    .clone()
                    .new_session(project.clone(), PathList::new(&[Path::new("")]), cx)
            })
            .await
            .unwrap();
        let session_id = acp_thread.read_with(cx, |thread, _| thread.session_id().clone());
        let thread = agent.read_with(cx, |agent, _| {
            agent.sessions.get(&session_id).unwrap().thread.clone()
        });

        let model = cx.update(|cx| {
            LanguageModelRegistry::read_global(cx)
                .default_model()
                .map(|default_model| default_model.model)
                .expect("default test model should be available")
        });
        let fake_model = model.as_fake();
        thread.update(cx, |thread, cx| {
            thread.set_model(model.clone(), cx);
        });

        let send = acp_thread.update(cx, |thread, cx| thread.send(vec!["hello".into()], cx));
        let send = cx.foreground_executor().spawn(send);
        cx.run_until_parked();

        fake_model.send_last_completion_stream_text_chunk("world");
        fake_model.end_last_completion_stream();
        send.await.unwrap();
        cx.run_until_parked();

        cx.update(|cx| connection.clone().close_session(&session_id, cx))
            .await
            .unwrap();
        drop(thread);
        drop(acp_thread);
        agent.read_with(cx, |agent, _| {
            assert!(agent.sessions.is_empty());
        });

        let first_loaded_thread = cx.update(|cx| {
            connection.clone().load_session(
                session_id.clone(),
                project.clone(),
                PathList::new(&[Path::new("")]),
                None,
                cx,
            )
        });
        let second_loaded_thread = cx.update(|cx| {
            connection.clone().load_session(
                session_id.clone(),
                project.clone(),
                PathList::new(&[Path::new("")]),
                None,
                cx,
            )
        });

        let first_loaded_thread = first_loaded_thread.await.unwrap();
        let second_loaded_thread = second_loaded_thread.await.unwrap();

        cx.run_until_parked();

        assert_eq!(
            first_loaded_thread.entity_id(),
            second_loaded_thread.entity_id(),
            "concurrent loads for the same session should share one AcpThread"
        );

        cx.update(|cx| connection.clone().close_session(&session_id, cx))
            .await
            .unwrap();

        agent.read_with(cx, |agent, _| {
            assert!(
                agent.sessions.contains_key(&session_id),
                "closing one loaded session should not drop shared session state"
            );
        });

        let follow_up = second_loaded_thread.update(cx, |thread, cx| {
            thread.send(vec!["still there?".into()], cx)
        });
        let follow_up = cx.foreground_executor().spawn(follow_up);
        cx.run_until_parked();

        fake_model.send_last_completion_stream_text_chunk("yes");
        fake_model.end_last_completion_stream();
        follow_up.await.unwrap();
        cx.run_until_parked();

        second_loaded_thread.read_with(cx, |thread, cx| {
            assert_eq!(
                thread.to_markdown(cx),
                formatdoc! {"
                    ## User

                    hello

                    ## Assistant

                    world

                    ## User

                    still there?

                    ## Assistant

                    yes

                "}
            );
        });

        cx.update(|cx| connection.clone().close_session(&session_id, cx))
            .await
            .unwrap();

        cx.run_until_parked();

        drop(first_loaded_thread);
        drop(second_loaded_thread);
        agent.read_with(cx, |agent, _| {
            assert!(agent.sessions.is_empty());
        });
    }

    #[gpui::test]
    async fn test_rapid_title_changes_do_not_loop(cx: &mut TestAppContext) {
        // Regression test: rapid title changes must not cause a propagation loop
        // between Thread and AcpThread via handle_thread_title_updated.
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        fs.insert_tree("/", json!({ "a": {} })).await;
        let project = Project::test(fs.clone(), [], cx).await;
        let thread_store = cx.new(|cx| ThreadStore::new(cx));
        let agent = cx
            .update(|cx| NativeAgent::new(thread_store.clone(), Templates::new(), fs.clone(), cx));
        let connection = Rc::new(NativeAgentConnection(agent.clone()));

        let acp_thread = cx
            .update(|cx| {
                connection
                    .clone()
                    .new_session(project.clone(), PathList::new(&[Path::new("")]), cx)
            })
            .await
            .unwrap();

        let session_id = acp_thread.read_with(cx, |thread, _| thread.session_id().clone());
        let thread = agent.read_with(cx, |agent, _| {
            agent.sessions.get(&session_id).unwrap().thread.clone()
        });

        let title_updated_count = Rc::new(std::cell::RefCell::new(0usize));
        cx.update(|cx| {
            let count = title_updated_count.clone();
            cx.subscribe(
                &thread,
                move |_entity: Entity<Thread>, _event: &TitleUpdated, _cx: &mut App| {
                    let new_count = {
                        let mut count = count.borrow_mut();
                        *count += 1;
                        *count
                    };
                    assert!(
                        new_count <= 2,
                        "TitleUpdated fired {new_count} times; \
                         title updates are looping"
                    );
                },
            )
            .detach();
        });

        thread.update(cx, |thread, cx| thread.set_title("first".into(), cx));
        thread.update(cx, |thread, cx| thread.set_title("second".into(), cx));

        cx.run_until_parked();

        thread.read_with(cx, |thread, _| {
            assert_eq!(thread.title(), Some("second".into()));
        });
        acp_thread.read_with(cx, |acp_thread, _| {
            assert_eq!(acp_thread.title(), Some("second".into()));
        });

        assert_eq!(*title_updated_count.borrow(), 2);
    }

    fn thread_entries(
        thread_store: &Entity<ThreadStore>,
        cx: &mut TestAppContext,
    ) -> Vec<(acp::SessionId, String)> {
        thread_store.read_with(cx, |store, _| {
            store
                .entries()
                .map(|entry| (entry.id.clone(), entry.title.to_string()))
                .collect::<Vec<_>>()
        })
    }

    fn init_test(cx: &mut TestAppContext) {
        env_logger::try_init().ok();
        cx.update(|cx| {
            let settings_store = SettingsStore::test(cx);
            cx.set_global(settings_store);

            LanguageModelRegistry::test(cx);
        });
    }

    #[test]
    fn test_strip_slash_command_prefix_keeps_inline_args() {
        // The bug being guarded against: skill slash invocation used to
        // discard the entire first text block, which threw away anything
        // the user typed on the same line as the command.
        assert_eq!(
            strip_slash_command_prefix("/fix-review #1, #2, #3"),
            "#1, #2, #3",
        );
    }

    #[test]
    fn test_strip_slash_command_prefix_preserves_newlines() {
        // Continuations across newlines are common when users compose
        // structured prompts; the first newline is the command terminator,
        // but everything after it must reach the model verbatim.
        assert_eq!(
            strip_slash_command_prefix("/fix-review\nline 1\nline 2"),
            "line 1\nline 2",
        );
    }

    #[test]
    fn test_strip_slash_command_prefix_command_only_is_empty() {
        assert_eq!(strip_slash_command_prefix("/fix-review"), "");
        assert_eq!(strip_slash_command_prefix("/fix-review "), "");
    }

    #[test]
    fn test_strip_slash_command_prefix_ignores_leading_whitespace() {
        assert_eq!(strip_slash_command_prefix("   /fix-review hello"), "hello",);
    }

    #[test]
    fn test_strip_slash_command_prefix_passes_through_non_command_text() {
        // Defense in depth: if somehow we're called with a non-slash-prefixed
        // block, the safe behavior is to return it unchanged rather than
        // silently mangling unrelated user text.
        assert_eq!(strip_slash_command_prefix("hello world"), "hello world",);
    }
}

fn mcp_message_content_to_acp_content_block(
    content: context_server::types::MessageContent,
) -> acp::ContentBlock {
    match content {
        context_server::types::MessageContent::Text {
            text,
            annotations: _,
        } => text.into(),
        context_server::types::MessageContent::Image {
            data,
            mime_type,
            annotations: _,
        } => acp::ContentBlock::Image(acp::ImageContent::new(data, mime_type)),
        context_server::types::MessageContent::Audio {
            data,
            mime_type,
            annotations: _,
        } => acp::ContentBlock::Audio(acp::AudioContent::new(data, mime_type)),
        context_server::types::MessageContent::Resource {
            resource,
            annotations: _,
        } => {
            let mut link =
                acp::ResourceLink::new(resource.uri.to_string(), resource.uri.to_string());
            if let Some(mime_type) = resource.mime_type {
                link = link.mime_type(mime_type);
            }
            acp::ContentBlock::ResourceLink(link)
        }
    }
}
