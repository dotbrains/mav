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

    mod command_tests {
        use super::*;

        include!("agent_tests/command.rs");
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

    mod global_skill_tests {
        use super::*;

        include!("agent_tests/global_skills.rs");
    }

    mod skill_visibility_tests {
        use super::*;

        include!("agent_tests/skill_visibility.rs");
    }

    mod project_skill_tests {
        use super::*;

        include!("agent_tests/project_skills.rs");
    }

    mod model_selection_tests {
        use super::*;

        include!("agent_tests/model_selection.rs");
    }

    mod loaded_thread_model_tests {
        use super::*;

        include!("agent_tests/loaded_thread_models.rs");
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
