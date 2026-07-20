use std::{
    cell::Cell,
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use acp_thread::{AcpThread, AcpThreadEvent, MentionUri, ThreadStatus};
use agent::{ContextServerRegistry, SharedThread, ThreadStore};
use agent_client_protocol::schema::v1 as acp;
use agent_servers::AgentServer;
use agent_settings::UserAgentsMd;
use db::kvp::{Dismissable, KeyValueStore};
use itertools::Itertools;
use project::AgentId;
use settings::{LanguageModelProviderSetting, LanguageModelSelection};

use mav_actions::{
    DecreaseBufferFontSize, IncreaseBufferFontSize, ResetBufferFontSize,
    agent::{
        AddSelectionToThread, LogoutAgent, OpenSettings, ReauthenticateAgent, ResetAgentZoom,
        ResetOnboarding, ResolveConflictedFilesWithAgent, ResolveConflictsWithAgent,
        ReviewBranchDiff,
    },
    assistant::{
        FocusAgent, ManageSkills, OpenGlobalAgentsMdRules, OpenProjectAgentsMdRules, Toggle,
        ToggleFocus,
    },
};

use crate::ExpandMessageEditor;
use crate::ManageProfiles;
use crate::agent_connection_store::AgentConnectionStore;
use crate::completion_provider::AgentContextSource;
use crate::terminal_thread_metadata_store::{
    TerminalThreadMetadata, TerminalThreadMetadataStore, terminal_title_without_prefix,
};
use crate::thread_metadata_store::{ThreadId, ThreadMetadataStore, ThreadMetadataStoreEvent};
use crate::{
    AddContextServer, AgentDiffPane, ConversationView, CopyThreadToClipboard, Follow,
    LoadThreadFromClipboard, NewTerminalThread, NewThread, OpenActiveThreadAsMarkdown,
    OpenAgentDiff, ResetFastModeWarnings, ResetTrialEndUpsell, ResetTrialUpsell,
    ShowAllSidebarThreadMetadata, ShowThreadMetadata, ToggleNewThreadMenu, ToggleOptionsMenu,
    agent_configuration::{AgentConfiguration, AssistantConfigurationEvent},
    conversation_view::{
        AcpThreadViewEvent, RootThreadUpdated, ThreadView, reset_fast_mode_warnings,
    },
    ui::{AgentNotification, AgentNotificationEvent, EndTrialUpsell},
};
use crate::{
    Agent, AgentInitialContent, AgentThreadSource, ExternalSourcePrompt, NewExternalAgentThread,
    NewNativeAgentThreadFromSummary,
};
#[path = "agent_panel_actions.rs"]
mod agent_panel_actions;
#[path = "agent_panel_commands.rs"]
mod agent_panel_commands;
#[path = "agent_panel_configuration.rs"]
mod agent_panel_configuration;
#[path = "agent_panel_diagnostics.rs"]
mod agent_panel_diagnostics;
#[path = "agent_panel_draft.rs"]
mod agent_panel_draft;
#[path = "agent_panel_environment.rs"]
mod agent_panel_environment;
#[path = "agent_panel_header.rs"]
mod agent_panel_header;
#[path = "agent_panel_model_override.rs"]
mod agent_panel_model_override;
#[path = "agent_panel_navigation.rs"]
mod agent_panel_navigation;
#[path = "agent_panel_panel.rs"]
mod agent_panel_panel;
#[path = "agent_panel_persistence.rs"]
mod agent_panel_persistence;
#[path = "agent_panel_prompts.rs"]
mod agent_panel_prompts;
#[path = "agent_panel_rules.rs"]
mod agent_panel_rules;
#[path = "agent_panel_sibling_host.rs"]
mod agent_panel_sibling_host;
#[path = "agent_panel_source_init.rs"]
mod agent_panel_source_init;
#[path = "agent_panel_surface.rs"]
mod agent_panel_surface;
#[path = "agent_panel_terminal.rs"]
mod agent_panel_terminal;
#[path = "agent_panel_terminal_lifecycle.rs"]
mod agent_panel_terminal_lifecycle;
#[path = "agent_panel_terminal_metadata.rs"]
mod agent_panel_terminal_metadata;
#[path = "agent_panel_terminal_notifications.rs"]
mod agent_panel_terminal_notifications;
#[path = "agent_panel_terminal_title.rs"]
mod agent_panel_terminal_title;
#[cfg(any(test, feature = "test-support"))]
#[path = "agent_panel_test_support.rs"]
mod agent_panel_test_support;
#[path = "agent_panel_thread_access.rs"]
mod agent_panel_thread_access;
#[path = "agent_panel_thread_creation.rs"]
mod agent_panel_thread_creation;
#[path = "agent_panel_thread_entries.rs"]
mod agent_panel_thread_entries;
#[path = "agent_panel_thread_entry.rs"]
mod agent_panel_thread_entry;
#[path = "agent_panel_thread_io.rs"]
mod agent_panel_thread_io;
#[path = "agent_panel_thread_launch.rs"]
mod agent_panel_thread_launch;
#[path = "agent_panel_thread_status.rs"]
mod agent_panel_thread_status;
#[path = "agent_panel_thread_types.rs"]
mod agent_panel_thread_types;
#[path = "agent_panel_toolbar.rs"]
mod agent_panel_toolbar;
#[path = "agent_panel_view_lifecycle.rs"]
mod agent_panel_view_lifecycle;
#[path = "agent_panel_view_state.rs"]
mod agent_panel_view_state;
use agent_panel_diagnostics::thread_metadata_to_debug_json;
pub(crate) use agent_panel_model_override::apply_native_model_override;
pub use agent_panel_panel::AgentPanelEvent;
use agent_panel_persistence::{
    AgentPanelEntryKind, SerializedActiveThread, SerializedAgentPanel,
    read_global_last_created_entry_kind, read_global_last_used_agent, read_legacy_serialized_panel,
    read_serialized_panel, save_serialized_panel, write_global_last_created_entry_kind,
    write_global_last_used_agent,
};
#[cfg(test)]
use agent_panel_prompts::conflict_resource_block;
use agent_panel_prompts::{
    build_conflict_resolution_prompt, build_conflicted_files_resolution_prompt,
    format_selection_for_terminal,
};
use agent_panel_rules::{open_global_rules, open_project_rules, project_agents_md_path};
use agent_panel_sibling_host::AgentPanelSiblingHost;
pub use agent_panel_terminal::{AgentPanelTerminalInfo, MaxIdleRetainedThreads, TerminalId};
use agent_panel_terminal::{AgentTerminal, TERMINAL_AGENT_TELEMETRY_ID};
pub use agent_panel_thread_types::{CreateThreadOptions, ThreadTitleRegenerationResult};
use agent_panel_view_state::{AgentThread, BaseView, OverlayView, VisibleSurface, WhichFontSize};
use agent_settings::AgentSettings;
use ai_onboarding::AgentPanelOnboarding;
use anyhow::Result;
#[cfg(feature = "audio")]
use audio::{Audio, Sound};
use chrono::{DateTime, Utc};
use client::UserStore;
use cloud_api_types::Plan;
use collections::HashMap;
use editor::{Editor, MultiBuffer};
use extension_host::ExtensionStore;
use feature_flags::{
    AgentSettingsUiFeatureFlag, CreateThreadToolFeatureFlag, FeatureFlagAppExt as _,
};

use fs::Fs;
use futures::FutureExt as _;
use gpui::{
    Action, Anchor, Animation, AnimationExt, AnyElement, App, AsyncWindowContext, ClipboardItem,
    Entity, ExternalPaths, FocusHandle, Focusable, KeyContext, Pixels, PlatformDisplay,
    Subscription, Task, TaskExt, WeakEntity, prelude::*, pulsating_between,
};
use language::LanguageRegistry;
use language_model::LanguageModelRegistry;
use notifications::status_toast::StatusToast;
use project::{Project, ProjectPath, Worktree};
use settings::{NotifyWhenAgentWaiting, Settings, update_settings_file};

use terminal::Event as TerminalEvent;
use terminal_view::TerminalView;
use theme_settings::ThemeSettings;
use ui::{
    ContextMenu, ContextMenuEntry, GradientFade, IconButton, KeyBinding, PopoverMenu,
    PopoverMenuHandle, ProjectEmptyState, Tab, Tooltip, prelude::*, utils::WithRemSize,
};
use util::ResultExt as _;
use workspace::{
    CollaboratorId, DraggedSelection, DraggedTab, MultiWorkspace, PaneKind, PathList,
    ToggleSidebar, ToggleZoom, Workspace, WorkspaceId,
    dock::{Panel, PanelEvent},
    item::ItemEvent,
};

#[cfg(test)]
use collections::HashSet;
#[cfg(test)]
use mav_actions::agent::ConflictContent;

const MIN_PANEL_WIDTH: Pixels = px(300.);
const TERMINAL_INIT_COMMAND_STARTUP_TIMEOUT: Duration = Duration::from_secs(5);

pub fn init(cx: &mut App) {
    cx.observe_new(
        |workspace: &mut Workspace, _window, _cx: &mut Context<Workspace>| {
            workspace
                .register_action(|workspace, _: &NewThread, window, cx| {
                    crate::agent_thread_item::create_agent_thread(
                        workspace,
                        Agent::NativeAgent,
                        None,
                        None,
                        None,
                        true,
                        None,
                        AgentThreadSource::Sidebar,
                        window,
                        cx,
                    );
                })
                .register_action(|workspace, _: &NewTerminalThread, window, cx| {
                    if let Some(panel) = workspace.panel::<AgentPanel>(cx) {
                        panel.update(cx, |panel, cx| {
                            panel.new_terminal(
                                Some(workspace),
                                AgentThreadSource::AgentPanel,
                                window,
                                cx,
                            )
                        });
                        workspace.focus_panel::<AgentPanel>(window, cx);
                    }
                })
                .register_action(
                    |workspace, action: &NewNativeAgentThreadFromSummary, window, cx| {
                        if let Some(panel) = workspace.panel::<AgentPanel>(cx) {
                            panel.update(cx, |panel, cx| {
                                panel.new_native_agent_thread_from_summary(action, window, cx)
                            });
                            workspace.focus_panel::<AgentPanel>(window, cx);
                        }
                    },
                )
                .register_action(|workspace, _: &ExpandMessageEditor, window, cx| {
                    if let Some(panel) = workspace.panel::<AgentPanel>(cx) {
                        workspace.focus_panel::<AgentPanel>(window, cx);
                        panel.update(cx, |panel, cx| panel.expand_message_editor(window, cx));
                    }
                })
                .register_action(|workspace, _: &OpenSettings, window, cx| {
                    if let Some(panel) = workspace.panel::<AgentPanel>(cx) {
                        workspace.focus_panel::<AgentPanel>(window, cx);
                        panel.update(cx, |panel, cx| panel.open_configuration(window, cx));
                    }
                })
                .register_action(|workspace, action: &NewExternalAgentThread, window, cx| {
                    crate::agent_thread_item::create_agent_thread(
                        workspace,
                        Agent::from(action.agent.clone()),
                        None,
                        None,
                        None,
                        true,
                        None,
                        AgentThreadSource::Sidebar,
                        window,
                        cx,
                    );
                })
                .register_action(|workspace, action: &ManageSkills, window, cx| {
                    if let Some(panel) = workspace.panel::<AgentPanel>(cx) {
                        workspace.focus_panel::<AgentPanel>(window, cx);
                        panel.update(cx, |panel, cx| panel.manage_skills(action, window, cx));
                    }
                })
                .register_action(|workspace, _: &OpenGlobalAgentsMdRules, window, cx| {
                    open_global_rules(workspace, window, cx);
                })
                .register_action(|workspace, _: &OpenProjectAgentsMdRules, window, cx| {
                    open_project_rules(workspace, window, cx);
                })
                .register_action(|workspace, _: &Follow, window, cx| {
                    workspace.follow(CollaboratorId::Agent, window, cx);
                })
                .register_action(|workspace, _: &OpenAgentDiff, window, cx| {
                    let thread = workspace
                        .panel::<AgentPanel>(cx)
                        .and_then(|panel| panel.read(cx).active_conversation_view().cloned())
                        .and_then(|conversation| {
                            conversation
                                .read(cx)
                                .root_thread_view()
                                .map(|r| r.read(cx).thread.clone())
                        });

                    if let Some(thread) = thread {
                        AgentDiffPane::deploy_in_workspace(thread, workspace, window, cx);
                    }
                })
                .register_action(|workspace, _: &ToggleOptionsMenu, window, cx| {
                    if let Some(multi_workspace) =
                        workspace.multi_workspace().and_then(|mw| mw.upgrade())
                        && multi_workspace.update(cx, |multi_workspace, cx| {
                            let Some(sidebar) = multi_workspace.sidebar() else {
                                return false;
                            };
                            sidebar.toggle_options_menu(window, cx);
                            true
                        })
                    {
                        cx.stop_propagation();
                        return;
                    }

                    if let Some(panel) = workspace.panel::<AgentPanel>(cx) {
                        workspace.focus_panel::<AgentPanel>(window, cx);
                        panel.update(cx, |panel, cx| {
                            panel.toggle_options_menu(&ToggleOptionsMenu, window, cx);
                        });
                    }
                })
                .register_action(|workspace, _: &ToggleNewThreadMenu, window, cx| {
                    if let Some(panel) = workspace.panel::<AgentPanel>(cx) {
                        workspace.focus_panel::<AgentPanel>(window, cx);
                        panel.update(cx, |panel, cx| {
                            panel.toggle_new_thread_menu(&ToggleNewThreadMenu, window, cx);
                        });
                    }
                })
                .register_action(|_workspace, _: &ResetOnboarding, window, cx| {
                    window.dispatch_action(workspace::RestoreBanner.boxed_clone(), cx);
                    window.refresh();
                })
                .register_action(|workspace, _: &ResetTrialUpsell, _window, cx| {
                    if let Some(panel) = workspace.panel::<AgentPanel>(cx) {
                        panel.update(cx, |panel, _| {
                            panel
                                .new_user_onboarding_upsell_dismissed
                                .store(false, Ordering::Release);
                        });
                    }
                    OnboardingUpsell::set_dismissed(false, cx);
                })
                .register_action(|_workspace, _: &ResetTrialEndUpsell, _window, cx| {
                    TrialEndUpsell::set_dismissed(false, cx);
                })
                .register_action(|_workspace, _: &ResetFastModeWarnings, _window, cx| {
                    reset_fast_mode_warnings(cx);
                })
                .register_action(|workspace, _: &ResetAgentZoom, window, cx| {
                    if let Some(panel) = workspace.panel::<AgentPanel>(cx) {
                        panel.update(cx, |panel, cx| {
                            panel.reset_agent_zoom(window, cx);
                        });
                    }
                })
                .register_action(|workspace, _: &CopyThreadToClipboard, window, cx| {
                    if let Some(panel) = workspace.panel::<AgentPanel>(cx) {
                        panel.update(cx, |panel, cx| {
                            panel.copy_thread_to_clipboard(window, cx);
                        });
                    }
                })
                .register_action(|workspace, _: &LoadThreadFromClipboard, window, cx| {
                    if let Some(panel) = workspace.panel::<AgentPanel>(cx) {
                        workspace.focus_panel::<AgentPanel>(window, cx);
                        panel.update(cx, |panel, cx| {
                            panel.load_thread_from_clipboard(window, cx);
                        });
                    }
                })
                .register_action(|workspace, _: &ShowThreadMetadata, window, cx| {
                    if let Some(panel) = workspace.panel::<AgentPanel>(cx) {
                        panel.update(cx, |panel, cx| {
                            panel.show_thread_metadata(&ShowThreadMetadata, window, cx);
                        });
                    }
                })
                .register_action(|workspace, _: &ShowAllSidebarThreadMetadata, window, cx| {
                    if let Some(panel) = workspace.panel::<AgentPanel>(cx) {
                        panel.update(cx, |panel, cx| {
                            panel.show_all_sidebar_thread_metadata(
                                &ShowAllSidebarThreadMetadata,
                                window,
                                cx,
                            );
                        });
                    }
                })
                .register_action(|workspace, action: &ReviewBranchDiff, window, cx| {
                    let Some(panel) = workspace.panel::<AgentPanel>(cx) else {
                        return;
                    };

                    let mention_uri = MentionUri::GitDiff {
                        base_ref: action.base_ref.to_string(),
                    };
                    let diff_uri = mention_uri.to_uri().to_string();

                    let content_blocks = vec![
                        acp::ContentBlock::Text(acp::TextContent::new(
                            "Please review this branch diff carefully. Point out any issues, \
                             potential bugs, or improvement opportunities you find.\n\n"
                                .to_string(),
                        )),
                        acp::ContentBlock::Resource(acp::EmbeddedResource::new(
                            acp::EmbeddedResourceResource::TextResourceContents(
                                acp::TextResourceContents::new(
                                    action.diff_text.to_string(),
                                    diff_uri,
                                ),
                            ),
                        )),
                    ];

                    workspace.focus_panel::<AgentPanel>(window, cx);

                    panel.update(cx, |panel, cx| {
                        panel.external_thread(
                            None,
                            None,
                            None,
                            None,
                            Some(AgentInitialContent::ContentBlock {
                                blocks: content_blocks,
                                auto_submit: true,
                            }),
                            true,
                            AgentThreadSource::GitPanel,
                            window,
                            cx,
                        );
                    });
                })
                .register_action(
                    |workspace, action: &ResolveConflictsWithAgent, window, cx| {
                        let Some(panel) = workspace.panel::<AgentPanel>(cx) else {
                            return;
                        };

                        let content_blocks = build_conflict_resolution_prompt(&action.conflicts);

                        workspace.focus_panel::<AgentPanel>(window, cx);

                        panel.update(cx, |panel, cx| {
                            panel.external_thread(
                                None,
                                None,
                                None,
                                None,
                                Some(AgentInitialContent::ContentBlock {
                                    blocks: content_blocks,
                                    auto_submit: true,
                                }),
                                true,
                                AgentThreadSource::GitPanel,
                                window,
                                cx,
                            );
                        });
                    },
                )
                .register_action(
                    |workspace, action: &ResolveConflictedFilesWithAgent, window, cx| {
                        let Some(panel) = workspace.panel::<AgentPanel>(cx) else {
                            return;
                        };

                        let content_blocks =
                            build_conflicted_files_resolution_prompt(&action.conflicted_file_paths);

                        workspace.focus_panel::<AgentPanel>(window, cx);

                        panel.update(cx, |panel, cx| {
                            panel.external_thread(
                                None,
                                None,
                                None,
                                None,
                                Some(AgentInitialContent::ContentBlock {
                                    blocks: content_blocks,
                                    auto_submit: true,
                                }),
                                true,
                                AgentThreadSource::GitPanel,
                                window,
                                cx,
                            );
                        });
                    },
                )
                .register_action(
                    |workspace: &mut Workspace, _: &AddSelectionToThread, window, cx| {
                        let active_editor = workspace
                            .active_item(cx)
                            .and_then(|item| item.act_as::<Editor>(cx));
                        let has_editor_selection = active_editor.is_some_and(|editor| {
                            editor.update(cx, |editor, cx| {
                                editor.has_non_empty_selection(&editor.display_snapshot(cx))
                            })
                        });

                        let has_terminal_selection = workspace
                            .active_item(cx)
                            .and_then(|item| item.act_as::<TerminalView>(cx))
                            .is_some_and(|terminal_view| {
                                terminal_view
                                    .read(cx)
                                    .terminal()
                                    .read(cx)
                                    .last_content
                                    .selection_text
                                    .as_ref()
                                    .is_some_and(|text| !text.is_empty())
                            });

                        if !has_editor_selection && !has_terminal_selection {
                            return;
                        }

                        let Some(agent_panel) = workspace.panel::<AgentPanel>(cx) else {
                            return;
                        };

                        let source = AgentContextSource::from_focused(workspace, window, cx);
                        let source = source.or_else(|| {
                            let cached = agent_panel.read(cx).last_context_source.clone()?;
                            cached.exists(workspace, cx).then_some(cached)
                        });
                        let source =
                            source.or_else(|| AgentContextSource::from_active(workspace, cx));

                        let Some(source) = source else {
                            return;
                        };

                        let Some(selection) = source.read_selection(workspace, true, cx) else {
                            return;
                        };

                        if !agent_panel.focus_handle(cx).contains_focused(window, cx) {
                            workspace.toggle_panel_focus::<AgentPanel>(window, cx);
                        }

                        agent_panel.update(cx, |panel, cx| {
                            panel.last_context_source = Some(source);
                            cx.defer_in(window, move |panel, window, cx| {
                                if let Some(conversation_view) = panel.active_conversation_view() {
                                    conversation_view.update(cx, |conversation_view, cx| {
                                        conversation_view.insert_selection(selection, window, cx);
                                    });
                                } else if let Some(terminal_id) = panel.active_terminal_id()
                                    && let Some(agent_terminal) = panel.terminals.get(&terminal_id)
                                {
                                    // Resolve mentions against the cwd: live cwd, else spawn dir.
                                    let working_directory = agent_terminal
                                        .view
                                        .read(cx)
                                        .terminal()
                                        .read(cx)
                                        .working_directory()
                                        .or_else(|| agent_terminal.working_directory.clone());
                                    let text = format_selection_for_terminal(
                                        &selection,
                                        &panel.project,
                                        working_directory.as_deref(),
                                        cx,
                                    );
                                    if !text.is_empty() {
                                        let view = agent_terminal.view.clone();
                                        view.update(cx, |view, cx| {
                                            view.terminal().update(cx, |terminal, _| {
                                                terminal.paste(&text);
                                            });
                                            window.focus(&view.focus_handle(cx), cx);
                                        });
                                    }
                                }
                            });
                        });
                    },
                )
                .register_action(|workspace, _: &menu::Cancel, _window, cx| {
                    if let Some(panel) = workspace.panel::<AgentPanel>(cx) {
                        let dismissed =
                            panel.update(cx, |panel, cx| panel.dismiss_all_notifications(cx));
                        if dismissed {
                            return;
                        }
                    }
                    cx.propagate();
                });
        },
    )
    .detach();
}

pub struct AgentPanel {
    workspace: WeakEntity<Workspace>,
    /// Workspace id is used as a database key
    workspace_id: Option<WorkspaceId>,
    user_store: Entity<UserStore>,
    project: Entity<Project>,
    fs: Arc<dyn Fs>,
    language_registry: Arc<LanguageRegistry>,
    thread_store: Entity<ThreadStore>,
    connection_store: Entity<AgentConnectionStore>,
    context_server_registry: Entity<ContextServerRegistry>,
    configuration: Option<Entity<AgentConfiguration>>,
    configuration_subscription: Option<Subscription>,
    focus_handle: FocusHandle,
    base_view: BaseView,
    last_created_entry_kind: AgentPanelEntryKind,
    overlay_view: Option<OverlayView>,
    draft_thread: Option<Entity<ConversationView>>,
    retained_threads: HashMap<ThreadId, Entity<ConversationView>>,
    terminals: HashMap<TerminalId, AgentTerminal>,
    pending_terminal_spawn: Option<TerminalId>,
    new_thread_menu_handle: PopoverMenuHandle<ContextMenu>,
    agent_panel_menu_handle: PopoverMenuHandle<ContextMenu>,
    _extension_subscription: Option<Subscription>,
    _project_subscription: Subscription,
    zoomed: bool,
    pending_serialization: Option<Task<Result<()>>>,
    new_user_onboarding: Entity<AgentPanelOnboarding>,
    new_user_onboarding_upsell_dismissed: AtomicBool,
    selected_agent: Agent,
    _thread_view_subscription: Option<Subscription>,
    _active_thread_focus_subscription: Option<Subscription>,
    _base_view_observation: Option<Subscription>,
    _draft_editor_observation: Option<Subscription>,
    _active_draft_reclaim_observation: Option<Subscription>,
    _thread_metadata_store_subscription: Subscription,
    last_context_source: Option<AgentContextSource>,

    is_active: bool,
}

impl AgentPanel {
    fn serialize(&mut self, cx: &mut App) {
        let Some(workspace_id) = self.workspace_id else {
            return;
        };

        let selected_agent = self.selected_agent.clone();
        let last_created_entry_kind = self.last_created_entry_kind;
        let last_active_terminal_id = self
            .active_terminal_id()
            .map(|terminal_id| terminal_id.to_key_string());

        let last_active_thread = if last_active_terminal_id.is_some() {
            None
        } else {
            let is_draft_active = self.active_thread_is_draft(cx);
            let active_thread_id = self.active_thread_id(cx);
            let active_thread_agent = self
                .active_conversation_view()
                .map(|cv| cv.read(cx).agent_key().clone())
                .unwrap_or_else(|| self.selected_agent.clone());
            self.active_agent_thread(cx)
                .map(|thread| {
                    let thread = thread.read(cx);

                    let title = thread.title();
                    let work_dirs = thread.work_dirs().cloned();
                    SerializedActiveThread {
                        session_id: (!is_draft_active).then(|| thread.session_id().0.to_string()),
                        thread_id: active_thread_id,
                        agent_type: active_thread_agent.clone(),
                        title: title.map(|t| t.to_string()),
                        work_dirs: work_dirs.map(|dirs| dirs.serialize()),
                    }
                })
                .or_else(|| {
                    // The active view may be in `Loading` or `LoadError` — for
                    // example, while a restored thread is waiting for a custom
                    // agent to finish registering. Without this fallback, a
                    // stray `serialize()` triggered during that window would
                    // write `session_id=None` and wipe the restored session
                    if is_draft_active {
                        return None;
                    }
                    let conversation_view = self.active_conversation_view()?;
                    let session_id = conversation_view.read(cx).root_session_id.clone()?;
                    let metadata = ThreadMetadataStore::try_global(cx)
                        .and_then(|store| store.read(cx).entry_by_session(&session_id).cloned());
                    Some(SerializedActiveThread {
                        session_id: Some(session_id.0.to_string()),
                        thread_id: active_thread_id,
                        agent_type: active_thread_agent.clone(),
                        title: metadata
                            .as_ref()
                            .and_then(|m| m.title.as_ref())
                            .map(|t| t.to_string()),
                        work_dirs: metadata.map(|m| m.folder_paths().serialize()),
                    })
                })
        };

        let new_draft_thread_id = self
            .draft_thread
            .as_ref()
            .map(|draft| draft.read(cx).thread_id);

        let kvp = KeyValueStore::global(cx);
        self.pending_serialization = Some(cx.background_spawn(async move {
            save_serialized_panel(
                workspace_id,
                SerializedAgentPanel {
                    selected_agent: Some(selected_agent),
                    last_created_entry_kind,
                    last_active_thread,
                    last_active_terminal_id,
                    new_draft_thread_id,
                },
                kvp,
            )
            .await?;
            anyhow::Ok(())
        }));
    }

    pub fn load(
        workspace: WeakEntity<Workspace>,
        mut cx: AsyncWindowContext,
    ) -> Task<Result<Entity<Self>>> {
        let kvp = cx.update(|_window, cx| KeyValueStore::global(cx)).ok();
        cx.spawn(async move |cx| {
            let workspace_id = workspace
                .read_with(cx, |workspace, _| workspace.database_id())
                .ok()
                .flatten();

            let (serialized_panel, global_last_used_agent, global_last_created_entry_kind) = cx
                .background_spawn(async move {
                    match kvp {
                        Some(kvp) => {
                            let panel = workspace_id
                                .and_then(|id| read_serialized_panel(id, &kvp))
                                .or_else(|| read_legacy_serialized_panel(&kvp));
                            let global_agent = read_global_last_used_agent(&kvp);
                            let global_entry_kind = read_global_last_created_entry_kind(&kvp);
                            (panel, global_agent, global_entry_kind)
                        }
                        None => (None, None, None),
                    }
                })
                .await;

            let has_open_project = workspace
                .read_with(cx, |workspace, cx| !workspace.root_paths(cx).is_empty())
                .unwrap_or(false);
            let terminal_id_to_restore = if has_open_project {
                serialized_panel
                    .as_ref()
                    .and_then(|panel| panel.last_active_terminal_id.as_deref())
                    .and_then(|terminal_id| {
                        match TerminalId::from_key_string(terminal_id) {
                            Ok(terminal_id) => Some(terminal_id),
                            Err(error) => {
                                log::warn!("failed to parse last active terminal id: {error}");
                                None
                            }
                        }
                    })
            } else {
                None
            };
            let terminal_to_restore = if let Some(terminal_id) = terminal_id_to_restore {
                match cx.update(|_window, cx| {
                    TerminalThreadMetadataStore::try_global(cx).map(|store| {
                        let reload_task = store.read(cx).reload_task();
                        (store, reload_task)
                    })
                }) {
                    Ok(Some((store, reload_task))) => {
                        reload_task.await;
                        match store
                            .read_with(cx, |store, _cx| store.entry(terminal_id).cloned())
                        {
                            Some(metadata) => Some(metadata),
                            None => {
                                log::info!(
                                    "last active terminal is missing, skipping restoration"
                                );
                                None
                            }
                        }
                    }
                    Ok(None) => {
                        log::warn!("failed to restore active terminal: metadata store missing");
                        None
                    }
                    Err(err) => {
                        log::warn!("failed to access terminal metadata store: {err}");
                        None
                    }
                }
            } else {
                None
            };

            let thread_to_restore = if has_open_project && terminal_to_restore.is_none() {
                if let Some(info) = serialized_panel
                    .as_ref()
                    .and_then(|panel| panel.last_active_thread.as_ref())
                {
                    match cx.update(|_window, cx| {
                        ThreadMetadataStore::try_global(cx).map(|store| {
                            let reload_task = store.read(cx).reload_task();
                            (store, reload_task)
                        })
                    }) {
                        Ok(Some((store, reload_task))) => {
                            reload_task.await;
                            let thread_id = store.read_with(cx, |store, _cx| {
                                let primary = info.thread_id.and_then(|tid| store.entry(tid));
                                let fallback = info.session_id.as_ref().and_then(|sid| {
                                    store.entry_by_session(&acp::SessionId::new(sid.clone()))
                                });
                                primary
                                    .or(fallback)
                                    .filter(|entry| !entry.archived)
                                    .map(|entry| entry.thread_id)
                            });
                            match thread_id {
                                Some(thread_id) => Some((info, thread_id)),
                                None => {
                                    log::info!(
                                        "last active thread is archived or missing, skipping restoration"
                                    );
                                    None
                                }
                            }
                        }
                        Ok(None) => {
                            log::warn!("failed to restore active thread: metadata store missing");
                            None
                        }
                        Err(err) => {
                            log::warn!("failed to access thread metadata store: {err}");
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let panel = workspace.update_in(cx, |workspace, window, cx| {
                let panel = cx.new(|cx| Self::new(workspace, window, cx));

                panel.update(cx, |panel, cx| {
                    let is_via_collab = panel.project.read(cx).is_via_collab();
                    // Collab workspaces only support NativeAgent; clamp any
                    // non-native choice so `set_active` can't bypass the
                    // collab guard in `external_thread`.
                    let clamp = |agent: Agent| {
                        if is_via_collab && !agent.is_native() {
                            Agent::NativeAgent
                        } else {
                            agent
                        }
                    };
                    let global_fallback =
                        global_last_used_agent.filter(|agent| !is_via_collab || agent.is_native());

                    if let Some(serialized_panel) = &serialized_panel {
                        panel.last_created_entry_kind = serialized_panel.last_created_entry_kind;
                    } else if let Some(entry_kind) = global_last_created_entry_kind {
                        panel.last_created_entry_kind = entry_kind;
                    }

                    // The thread being restored may have been bound to an
                    // agent different from the panel's last selected one
                    // (e.g. a draft created while a different agent was
                    // active). When restoring a thread, prefer its agent
                    // so the draft survives reload bound to the right
                    // backend; otherwise fall back to the serialized
                    // selection, then the global last-used agent.
                    let initial_agent = match &thread_to_restore {
                        Some((info, _)) => Some(clamp(info.agent_type.clone())),
                        None => serialized_panel
                            .as_ref()
                            .and_then(|p| p.selected_agent.clone())
                            .map(clamp)
                            .or(global_fallback),
                    };
                    if let Some(agent) = initial_agent {
                        panel.selected_agent = agent;
                    }

                    if let Some(metadata) = terminal_to_restore {
                        panel.restore_terminal_for_panel_load(
                            metadata,
                            false,
                            AgentThreadSource::AgentPanel,
                            Some(workspace),
                            window,
                            cx,
                        );
                    } else if let Some((info, thread_id)) = thread_to_restore {
                        let agent = panel.selected_agent.clone();
                        panel.load_agent_thread(
                            agent,
                            thread_id,
                            info.work_dirs.as_ref().map(PathList::deserialize),
                            info.title.clone().map(Into::into),
                            false,
                            AgentThreadSource::AgentPanel,
                            window,
                            cx,
                        );
                    }
                    if let Some(new_draft_thread_id) = serialized_panel
                        .as_ref()
                        .and_then(|p| p.new_draft_thread_id)
                    {
                        panel.restore_new_draft(new_draft_thread_id, window, cx);
                    }
                    cx.notify();
                });

                panel
            })?;

            Ok(panel)
        })
    }

    pub(crate) fn new(workspace: &Workspace, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let fs = workspace.app_state().fs.clone();
        let user_store = workspace.app_state().user_store.clone();
        let project = workspace.project();
        let language_registry = project.read(cx).languages().clone();
        let client = workspace.client().clone();
        let workspace_id = workspace.database_id();
        let workspace = workspace.weak_handle();

        let context_server_registry =
            cx.new(|cx| ContextServerRegistry::new(project.read(cx).context_server_store(), cx));

        let thread_store = ThreadStore::global(cx);

        let base_view = BaseView::Uninitialized;

        let weak_panel = cx.entity().downgrade();
        let onboarding = cx.new(|cx| {
            AgentPanelOnboarding::new(
                user_store.clone(),
                client,
                move |_window, cx| {
                    weak_panel
                        .update(cx, |panel, cx| {
                            panel.dismiss_ai_onboarding(cx);
                        })
                        .ok();
                },
                cx,
            )
        });

        // Subscribe to extension events to sync agent servers when extensions change
        let extension_subscription = ExtensionStore::try_global(cx).map(|store| {
            cx.subscribe(&store, |this, _source, event, cx| match event {
                extension_host::Event::ExtensionUninstalled(id) => {
                    this.migrate_agent_server_from_extensions(id.clone(), cx);
                }
                _ => {}
            })
        });

        let connection_store = cx.new(|cx| AgentConnectionStore::new(project.clone(), cx));
        let _project_subscription =
            cx.subscribe(&project, |this, _project, event, cx| match event {
                project::Event::WorktreeAdded(_)
                | project::Event::WorktreeRemoved(_)
                | project::Event::WorktreeOrderChanged
                | project::Event::WorktreePathsChanged { .. } => {
                    this.ensure_native_agent_connection(cx);
                    this.update_thread_work_dirs(cx);
                    this.persist_all_terminal_metadata(cx);
                    cx.notify();
                }
                _ => {}
            });

        let _thread_metadata_store_subscription = cx.subscribe(
            &ThreadMetadataStore::global(cx),
            |this, _store, event, cx| {
                let ThreadMetadataStoreEvent::ThreadArchived(thread_id) = event;
                if this.retained_threads.remove(thread_id).is_some() {
                    cx.notify();
                }
            },
        );

        cx.on_release(|this, cx| {
            this.dismiss_all_terminal_notifications(cx);
        })
        .detach();

        let panel = Self {
            workspace_id,
            base_view,
            last_created_entry_kind: AgentPanelEntryKind::Thread,
            overlay_view: None,
            workspace,
            user_store,
            project: project.clone(),
            fs: fs.clone(),
            language_registry,
            connection_store,
            configuration: None,
            configuration_subscription: None,
            focus_handle: cx.focus_handle(),
            context_server_registry,
            draft_thread: None,
            retained_threads: HashMap::default(),
            terminals: HashMap::default(),
            pending_terminal_spawn: None,
            new_thread_menu_handle: PopoverMenuHandle::default(),
            agent_panel_menu_handle: PopoverMenuHandle::default(),

            _extension_subscription: extension_subscription,
            _project_subscription,
            zoomed: false,
            pending_serialization: None,
            new_user_onboarding: onboarding,
            thread_store,
            selected_agent: Agent::default(),
            _thread_view_subscription: None,
            _active_thread_focus_subscription: None,
            new_user_onboarding_upsell_dismissed: AtomicBool::new(OnboardingUpsell::dismissed(cx)),
            _base_view_observation: None,
            _draft_editor_observation: None,
            _active_draft_reclaim_observation: None,
            _thread_metadata_store_subscription,
            last_context_source: None,
            is_active: false,
        };

        panel.ensure_native_agent_connection(cx);
        panel
    }
}

impl Focusable for AgentPanel {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        match self.visible_surface() {
            VisibleSurface::Uninitialized => self.focus_handle.clone(),
            VisibleSurface::AgentThread(conversation_view) => conversation_view.focus_handle(cx),
            VisibleSurface::Terminal(terminal_view) => terminal_view.focus_handle(cx),
            VisibleSurface::Configuration(configuration) => {
                if let Some(configuration) = configuration {
                    configuration.focus_handle(cx)
                } else {
                    self.focus_handle.clone()
                }
            }
        }
    }
}

impl Render for AgentPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // WARNING: Changes to this element hierarchy can have
        // non-obvious implications to the layout of children.
        //
        // If you need to change it, please confirm:
        // - The message editor expands (cmd-option-esc) correctly
        // - When expanded, the buttons at the bottom of the panel are displayed correctly
        // - Font size works as expected and can be changed with cmd-+/cmd-
        // - Scrolling in all views works as expected
        // - Files can be dropped into the panel
        let content = v_flex()
            .key_context(self.key_context())
            .relative()
            .size_full()
            .justify_between()
            .bg(cx.theme().colors().panel_background)
            .on_action(cx.listener(|this, action: &NewThread, window, cx| {
                this.new_thread(action, window, cx);
            }))
            .on_action(cx.listener(|this, _: &NewTerminalThread, window, cx| {
                cx.stop_propagation();
                this.new_terminal(None, AgentThreadSource::AgentPanel, window, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenSettings, window, cx| {
                this.open_configuration(window, cx);
            }))
            .on_action(cx.listener(Self::open_active_thread_as_markdown))
            .on_action(cx.listener(Self::manage_skills))
            .on_action(cx.listener(Self::go_back))
            .on_action(cx.listener(Self::toggle_options_menu))
            .on_action(cx.listener(Self::increase_font_size))
            .on_action(cx.listener(Self::decrease_font_size))
            .on_action(cx.listener(Self::reset_font_size))
            .on_action(cx.listener(Self::toggle_zoom))
            .on_action(cx.listener(|this, _: &ReauthenticateAgent, window, cx| {
                if let Some(conversation_view) = this.active_conversation_view() {
                    conversation_view.update(cx, |conversation_view, cx| {
                        conversation_view.reauthenticate(window, cx)
                    })
                }
            }))
            .on_action(cx.listener(|this, _: &LogoutAgent, window, cx| {
                if let Some(conversation_view) = this.active_conversation_view() {
                    conversation_view.update(cx, |conversation_view, cx| {
                        conversation_view.logout(window, cx)
                    })
                }
            }))
            .child(self.render_toolbar(window, cx))
            .children(self.render_new_user_onboarding(window, cx))
            .map(|parent| match self.visible_surface() {
                VisibleSurface::Uninitialized if !self.has_open_project(cx) => {
                    parent.child(self.render_no_project_state(cx))
                }
                VisibleSurface::Uninitialized => parent,
                VisibleSurface::AgentThread(conversation_view) => parent
                    .child(conversation_view.clone())
                    .child(self.render_drag_target(cx)),
                VisibleSurface::Terminal(terminal_view) => parent
                    .child(terminal_view.clone())
                    .child(self.render_drag_target(cx)),
                VisibleSurface::Configuration(configuration) => {
                    parent.children(configuration.cloned())
                }
            })
            .children(self.render_trial_end_upsell(window, cx));

        match self.visible_font_size() {
            WhichFontSize::AgentFont => {
                WithRemSize::new(ThemeSettings::get_global(cx).agent_ui_font_size(cx))
                    .size_full()
                    .child(content)
                    .into_any()
            }
            _ => content.into_any(),
        }
    }
}

struct OnboardingUpsell;

impl Dismissable for OnboardingUpsell {
    const KEY: &'static str = "dismissed-trial-upsell";
}

struct TrialEndUpsell;

impl Dismissable for TrialEndUpsell {
    const KEY: &'static str = "dismissed-trial-end-upsell";
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NewWorktreeBranchTarget;
    use crate::conversation_view::tests::{StubAgentServer, init_test};
    use crate::test_support::{
        active_session_id, active_thread_id, open_thread_with_connection,
        open_thread_with_custom_connection, register_test_sidebar, send_message,
    };
    use acp_thread::{AgentConnection, StubAgentConnection, ThreadStatus};
    use action_log::ActionLog;
    use anyhow::{Result, anyhow};
    use feature_flags::FeatureFlagAppExt;
    use fs::FakeFs;
    use gpui::{App, TestAppContext, UpdateGlobal, VisualTestContext};
    use parking_lot::Mutex;
    use project::{Project, WorktreePaths};
    use settings::{SettingsStore, WorkingDirectory};
    use std::any::Any;

    use serde_json::json;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::Instant;

    #[path = "agent_panel_conflict_prompt_tests.rs"]
    mod conflict_prompt_tests;
    #[path = "agent_panel_serialization_tests.rs"]
    mod serialization_tests;
    #[path = "agent_panel_session_tracking_connection.rs"]
    mod session_tracking_connection;
    use session_tracking_connection::SessionTrackingConnection;
    #[path = "agent_panel_agent_selection_tests.rs"]
    mod agent_selection_tests;
    #[path = "agent_panel_disassociation_tests.rs"]
    mod disassociation_tests;
    #[path = "agent_panel_draft_lifecycle_tests.rs"]
    mod draft_lifecycle_tests;
    #[path = "agent_panel_draft_promotion_tests.rs"]
    mod draft_promotion_tests;
    #[path = "agent_panel_draft_prompt_tests.rs"]
    mod draft_prompt_tests;
    #[path = "agent_panel_draft_reload_tests.rs"]
    mod draft_reload_tests;
    #[path = "agent_panel_draft_switching_tests.rs"]
    mod draft_switching_tests;
    #[path = "agent_panel_entry_action_tests.rs"]
    mod entry_action_tests;
    #[path = "agent_panel_entry_command_tests.rs"]
    mod entry_command_tests;
    #[path = "agent_panel_external_drop_tests.rs"]
    mod external_drop_tests;
    #[path = "agent_panel_misc_regression_tests.rs"]
    mod misc_regression_tests;
    #[path = "agent_panel_source_initialization_tests.rs"]
    mod source_initialization_tests;
    #[path = "agent_panel_terminal_init_tests.rs"]
    mod terminal_init_tests;
    #[path = "agent_panel_terminal_notification_lifecycle_tests.rs"]
    mod terminal_notification_lifecycle_tests;
    #[path = "agent_panel_terminal_notification_overlay_tests.rs"]
    mod terminal_notification_overlay_tests;
    #[path = "agent_panel_terminal_notification_sidebar_tests.rs"]
    mod terminal_notification_sidebar_tests;
    #[path = "agent_panel_terminal_restore_tests.rs"]
    mod terminal_restore_tests;
    #[path = "agent_panel_terminal_title_display_tests.rs"]
    mod terminal_title_display_tests;
    #[path = "agent_panel_terminal_title_editor_tests.rs"]
    mod terminal_title_editor_tests;
    #[path = "agent_panel_thread_cleanup_tests.rs"]
    mod thread_cleanup_tests;
    #[path = "agent_panel_thread_navigation_tests.rs"]
    mod thread_navigation_tests;
    #[path = "agent_panel_thread_restore_tests.rs"]
    mod thread_restore_tests;
    #[path = "agent_panel_thread_workdir_tests.rs"]
    mod thread_workdir_tests;
    #[path = "agent_panel_worktree_rollback_tests.rs"]
    mod worktree_rollback_tests;

    /// Extracts the text from a Text content block, panicking if it's not Text.
    fn expect_text_block(block: &acp::ContentBlock) -> &str {
        match block {
            acp::ContentBlock::Text(t) => t.text.as_str(),
            other => panic!("expected Text block, got {:?}", other),
        }
    }

    /// Extracts the (text_content, uri) from a Resource content block, panicking
    /// if it's not a TextResourceContents resource.
    fn expect_resource_block(block: &acp::ContentBlock) -> (&str, &str) {
        match block {
            acp::ContentBlock::Resource(r) => match &r.resource {
                acp::EmbeddedResourceResource::TextResourceContents(t) => {
                    (t.text.as_str(), t.uri.as_str())
                }
                other => panic!("expected TextResourceContents, got {:?}", other),
            },
            other => panic!("expected Resource block, got {:?}", other),
        }
    }

    fn open_generating_thread_with_loadable_connection(
        panel: &Entity<AgentPanel>,
        connection: &StubAgentConnection,
        cx: &mut VisualTestContext,
    ) -> (acp::SessionId, ThreadId) {
        open_thread_with_custom_connection(panel, connection.clone(), cx);
        let session_id = active_session_id(panel, cx);
        let thread_id = active_thread_id(panel, cx);
        send_message(panel, cx);
        cx.update(|_, cx| {
            connection.send_update(
                session_id.clone(),
                acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk::new("done".into())),
                cx,
            );
        });
        cx.run_until_parked();
        (session_id, thread_id)
    }

    fn open_idle_thread_with_non_loadable_connection(
        panel: &Entity<AgentPanel>,
        connection: &StubAgentConnection,
        cx: &mut VisualTestContext,
    ) -> (acp::SessionId, ThreadId) {
        open_thread_with_custom_connection(panel, connection.clone(), cx);
        let session_id = active_session_id(panel, cx);
        let thread_id = active_thread_id(panel, cx);

        connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
            acp::ContentChunk::new("done".into()),
        )]);
        send_message(panel, cx);

        (session_id, thread_id)
    }

    async fn setup_panel(cx: &mut TestAppContext) -> (Entity<AgentPanel>, VisualTestContext) {
        init_test(cx);
        cx.update(|cx| {
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
        });

        let fs = FakeFs::new(cx.executor());
        cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));
        fs.insert_tree("/project", json!({ "file.txt": "" })).await;
        let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;

        let multi_workspace =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));

        let workspace = multi_workspace
            .read_with(cx, |mw, _cx| mw.workspace().clone())
            .unwrap();

        let mut cx = VisualTestContext::from_window(multi_workspace.into(), cx);

        let panel = workspace.update_in(&mut cx, |workspace, window, cx| {
            cx.new(|cx| AgentPanel::new(workspace, window, cx))
        });

        (panel, cx)
    }

    async fn setup_visible_panel(
        cx: &mut TestAppContext,
    ) -> (Entity<AgentPanel>, VisualTestContext) {
        setup_visible_panel_with_sidebar(cx, true).await
    }

    async fn setup_visible_panel_with_sidebar(
        cx: &mut TestAppContext,
        threads_list_active: bool,
    ) -> (Entity<AgentPanel>, VisualTestContext) {
        init_test(cx);
        cx.update(|cx| {
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
            AgentSettings::override_global(
                AgentSettings {
                    notify_when_agent_waiting: NotifyWhenAgentWaiting::PrimaryScreen,
                    ..AgentSettings::get_global(cx).clone()
                },
                cx,
            );
        });

        let fs = FakeFs::new(cx.executor());
        cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));
        fs.insert_tree("/project", json!({ "file.txt": "" })).await;
        let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;

        let multi_workspace =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));

        let workspace = multi_workspace
            .read_with(cx, |multi_workspace, _cx| {
                multi_workspace.workspace().clone()
            })
            .unwrap();

        let mut cx = VisualTestContext::from_window(multi_workspace.into(), cx);
        register_test_sidebar(threads_list_active, &mut cx);

        let panel = workspace.update_in(&mut cx, |workspace, window, cx| {
            let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
            workspace.add_panel(panel.clone(), window, cx);
            workspace.focus_panel::<AgentPanel>(window, cx);
            panel
        });

        (panel, cx)
    }

    async fn setup_workspace_panel(
        cx: &mut TestAppContext,
    ) -> (Entity<Workspace>, Entity<AgentPanel>, VisualTestContext) {
        init_test(cx);
        cx.update(|cx| {
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
        });

        let fs = FakeFs::new(cx.executor());
        fs.insert_tree("/project", json!({ "file.txt": "" })).await;
        cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));
        let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;

        let multi_workspace =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));

        let workspace = multi_workspace
            .read_with(cx, |mw, _cx| mw.workspace().clone())
            .unwrap();

        let mut cx = VisualTestContext::from_window(multi_workspace.into(), cx);

        let panel = workspace.update_in(&mut cx, |workspace, window, cx| {
            let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
            workspace.add_panel(panel.clone(), window, cx);
            panel
        });

        (workspace, panel, cx)
    }

    #[gpui::test]
    async fn test_initialize_from_source_does_not_overwrite_existing_content(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);
        cx.update(|cx| {
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
        });

        let fs = FakeFs::new(cx.executor());
        fs.insert_tree("/project_a", json!({ "file.txt": "" }))
            .await;
        fs.insert_tree("/project_b", json!({ "file.txt": "" }))
            .await;
        let project_a = Project::test(fs.clone(), [Path::new("/project_a")], cx).await;
        let project_b = Project::test(fs.clone(), [Path::new("/project_b")], cx).await;

        let multi_workspace =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));

        let workspace_a = multi_workspace
            .read_with(cx, |mw, _cx| mw.workspace().clone())
            .unwrap();

        let workspace_b = multi_workspace
            .update(cx, |multi_workspace, window, cx| {
                multi_workspace.test_add_workspace(project_b.clone(), window, cx)
            })
            .unwrap();

        let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);

        // Set up panel_a with draft text.
        let panel_a = workspace_a.update_in(cx, |workspace, window, cx| {
            let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
            workspace.add_panel(panel.clone(), window, cx);
            panel
        });
        cx.run_until_parked();

        panel_a.update_in(cx, |panel, window, cx| {
            panel.open_external_thread_with_server(
                Rc::new(StubAgentServer::default_response()),
                window,
                cx,
            );
        });
        cx.run_until_parked();

        let thread_view_a =
            panel_a.read_with(cx, |panel, cx| panel.active_thread_view(cx).unwrap());
        let editor_a = thread_view_a.read_with(cx, |view, _cx| view.message_editor.clone());
        editor_a.update_in(cx, |editor, window, cx| {
            editor.set_text("Draft from workspace A", window, cx);
        });

        // Set up panel_b with its OWN content — this is a non-fresh panel.
        let panel_b = workspace_b.update_in(cx, |workspace, window, cx| {
            let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
            workspace.add_panel(panel.clone(), window, cx);
            panel
        });
        cx.run_until_parked();

        panel_b.update_in(cx, |panel, window, cx| {
            panel.open_external_thread_with_server(
                Rc::new(StubAgentServer::default_response()),
                window,
                cx,
            );
        });
        cx.run_until_parked();

        let thread_view_b =
            panel_b.read_with(cx, |panel, cx| panel.active_thread_view(cx).unwrap());
        let editor_b = thread_view_b.read_with(cx, |view, _cx| view.message_editor.clone());
        editor_b.update_in(cx, |editor, window, cx| {
            editor.set_text("Existing work in workspace B", window, cx);
        });

        // Attempting to initialize panel_b from workspace_a should be rejected
        // because panel_b already has meaningful content.
        let transferred = panel_b.update_in(cx, |panel, window, cx| {
            panel.initialize_from_source_workspace_if_needed(workspace_a.downgrade(), window, cx)
        });
        assert!(
            !transferred,
            "destination panel with existing content should not be overwritten"
        );

        // Verify panel_b still has its original content.
        panel_b.read_with(cx, |panel, cx| {
            let thread_view = panel
                .active_thread_view(cx)
                .expect("panel_b should still have its thread view");
            let text = thread_view.read(cx).message_editor.read(cx).text(cx);
            assert_eq!(
                text, "Existing work in workspace B",
                "destination panel's content should be preserved"
            );
        });
    }

    #[gpui::test]
    async fn test_create_thread_with_options_retains_thread_and_restores_agent(
        cx: &mut TestAppContext,
    ) {
        let (panel, mut cx) = setup_panel(cx).await;
        let _stub_connection =
            crate::test_support::set_stub_agent_connection(StubAgentConnection::new());

        // Baseline: panel's selected_agent is the stub.
        panel.update(&mut cx, |panel, _cx| {
            panel.selected_agent = Agent::Stub;
        });

        // Case 1: no agent override. The new thread should land in
        // `retained_threads` and `selected_agent` should be unchanged.
        let no_override_id = panel.update_in(&mut cx, |panel, window, cx| {
            panel.create_thread_with_options(
                CreateThreadOptions::default(),
                AgentThreadSource::AgentPanel,
                window,
                cx,
            )
        });

        panel.read_with(&cx, |panel, _cx| {
            assert!(
                panel.retained_threads.contains_key(&no_override_id),
                "thread created via create_thread_with_options should be retained"
            );
            assert_eq!(
                panel.selected_agent,
                Agent::Stub,
                "selected_agent should be unchanged when no agent override is requested"
            );
        });

        // Case 2: an explicit agent override that differs from the panel's
        // selection. `create_agent_thread_inner` updates `selected_agent` as a
        // side effect; `create_thread_with_options` must restore it so the
        // user's last-used agent isn't silently flipped by an agent-initiated
        // call.
        let override_agent = Agent::Custom {
            id: "override-agent".into(),
        };
        let override_id = panel.update_in(&mut cx, |panel, window, cx| {
            panel.create_thread_with_options(
                CreateThreadOptions {
                    agent: Some(override_agent.clone()),
                    ..CreateThreadOptions::default()
                },
                AgentThreadSource::AgentPanel,
                window,
                cx,
            )
        });

        panel.read_with(&cx, |panel, _cx| {
            assert!(
                panel.retained_threads.contains_key(&override_id),
                "thread created with an agent override should also be retained"
            );
            assert_ne!(
                no_override_id, override_id,
                "each call should produce a distinct ThreadId"
            );
            assert_eq!(
                panel.selected_agent,
                Agent::Stub,
                "selected_agent should be restored to the original after an agent override"
            );
        });
    }
}
