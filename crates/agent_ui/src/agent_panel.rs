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

    pub fn toggle_focus(
        workspace: &mut Workspace,
        _: &ToggleFocus,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        if workspace
            .panel::<Self>(cx)
            .is_some_and(|panel| panel.read(cx).enabled(cx))
        {
            workspace.toggle_panel_focus::<Self>(window, cx);
        }
    }

    pub fn focus(
        workspace: &mut Workspace,
        _: &FocusAgent,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        if workspace
            .panel::<Self>(cx)
            .is_some_and(|panel| panel.read(cx).enabled(cx))
        {
            workspace.focus_panel::<Self>(window, cx);
        }
    }

    pub fn toggle(
        workspace: &mut Workspace,
        _: &Toggle,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        if workspace
            .panel::<Self>(cx)
            .is_some_and(|panel| panel.read(cx).enabled(cx))
        {
            if !workspace.toggle_panel_focus::<Self>(window, cx) {
                workspace.close_panel::<Self>(window, cx);
            }
        }
    }

    pub fn thread_store(&self) -> &Entity<ThreadStore> {
        &self.thread_store
    }

    pub fn connection_store(&self) -> &Entity<AgentConnectionStore> {
        &self.connection_store
    }

    pub fn selected_agent(&self, cx: &App) -> Agent {
        if self.project.read(cx).is_via_collab() {
            Agent::NativeAgent
        } else {
            self.selected_agent.clone()
        }
    }

    pub fn open_thread(
        &mut self,
        session_id: acp::SessionId,
        work_dirs: Option<PathList>,
        title: Option<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Share links / clipboard imports enter with only a session id. If
        // this machine already has a metadata row for the session, route
        // through the normal thread-id path.
        let existing_thread_id = ThreadMetadataStore::try_global(cx).and_then(|store| {
            store
                .read(cx)
                .entry_by_session(&session_id)
                .map(|m| m.thread_id)
        });
        if let Some(thread_id) = existing_thread_id {
            self.load_agent_thread(
                crate::Agent::NativeAgent,
                thread_id,
                work_dirs,
                title,
                true,
                AgentThreadSource::AgentPanel,
                window,
                cx,
            );
        } else {
            self.external_thread_by_session(
                crate::Agent::NativeAgent,
                session_id,
                work_dirs,
                title,
                true,
                AgentThreadSource::AgentPanel,
                window,
                cx,
            );
        }
    }

    fn external_thread_by_session(
        &mut self,
        agent: Agent,
        session_id: acp::SessionId,
        work_dirs: Option<PathList>,
        title: Option<SharedString>,
        focus: bool,
        source: AgentThreadSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let thread = self.create_agent_thread_with_server_for_external_session(
            agent, None, session_id, work_dirs, title, None, source, window, cx,
        );
        self.set_base_view(thread.into(), focus, window, cx);
    }

    pub(crate) fn context_server_registry(&self) -> &Entity<ContextServerRegistry> {
        &self.context_server_registry
    }

    pub fn is_visible(workspace: &Entity<Workspace>, cx: &App) -> bool {
        let workspace_read = workspace.read(cx);

        workspace_read
            .panel::<AgentPanel>(cx)
            .map(|panel| {
                let panel_id = Entity::entity_id(&panel);

                workspace_read.all_docks().iter().any(|dock| {
                    dock.read(cx)
                        .visible_panel()
                        .is_some_and(|visible_panel| visible_panel.panel_id() == panel_id)
                })
            })
            .unwrap_or(false)
    }

    /// Clear the active view, retaining any running thread in the background.
    pub fn clear_base_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let old_view = std::mem::replace(&mut self.base_view, BaseView::Uninitialized);
        self.retain_running_thread(old_view, cx);
        self.clear_overlay_state();
        self.activate_draft(false, AgentThreadSource::AgentPanel, window, cx);
        self.serialize(cx);
        cx.emit(AgentPanelEvent::ActiveViewChanged);
        cx.notify();
    }

    pub fn new_thread(&mut self, _action: &NewThread, window: &mut Window, cx: &mut Context<Self>) {
        if !self.has_open_project(cx) {
            return;
        }

        self.new_thread_with_workspace(None, window, cx);
    }

    fn new_thread_with_workspace(
        &mut self,
        workspace: Option<&Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.should_create_terminal_for_new_entry(cx) {
            self.new_terminal(workspace, AgentThreadSource::AgentPanel, window, cx);
        } else {
            self.activate_new_thread(true, AgentThreadSource::AgentPanel, window, cx);
        }
    }

    pub fn activate_new_thread(
        &mut self,
        focus: bool,
        source: AgentThreadSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.has_open_project(cx) {
            return;
        }

        self.set_last_created_entry_kind_from_user_action(AgentPanelEntryKind::Thread, cx);

        // If the user is viewing a *parked* draft and the ephemeral
        // new-draft slot is occupied, pressing `+` should just focus the
        // ephemeral draft — not park it and create yet another empty one.
        // This matches the mental model of `+` as "go to my new-thread
        // slot". The parked draft will be put back into `retained_threads`
        // by `set_base_view`'s `retain_running_thread` call.
        if let Some(draft) = self.draft_thread.clone()
            && self.active_thread_is_draft(cx)
            && !self.active_view_is_new_draft(cx)
            && *draft.read(cx).agent_key() == self.selected_agent
        {
            self.set_base_view(
                BaseView::AgentThread {
                    conversation_view: draft,
                },
                focus,
                window,
                cx,
            );
            return;
        }

        if let Some(draft) = self.draft_thread.clone() {
            if self.draft_has_content(&draft, cx) {
                let draft_id = draft.read(cx).thread_id;
                self.draft_thread = None;
                self._draft_editor_observation = None;
                self.retained_threads.insert(draft_id, draft);
            } else if *draft.read(cx).agent_key() != self.selected_agent {
                let old_draft_id = draft.read(cx).thread_id;
                ThreadMetadataStore::global(cx).update(cx, |store, cx| {
                    store.delete(old_draft_id, cx);
                });
                self.draft_thread = None;
                self._draft_editor_observation = None;
            }
        }
        self.activate_draft(focus, source, window, cx);
    }

    pub fn new_external_agent_thread(
        &mut self,
        action: &NewExternalAgentThread,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.has_open_project(cx) {
            return;
        }

        self.selected_agent = action.agent.clone().into();
        self.activate_new_thread(true, AgentThreadSource::AgentPanel, window, cx);
    }

    pub fn new_terminal(
        &mut self,
        workspace: Option<&Workspace>,
        source: AgentThreadSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.supports_terminal(cx) {
            return;
        }
        self.set_last_created_entry_kind_from_user_action(AgentPanelEntryKind::Terminal, cx);
        let working_directory = self.terminal_working_directory(workspace, cx);
        self.spawn_terminal(
            TerminalId::new(),
            working_directory,
            None,
            None,
            None,
            true,
            true,
            true,
            source,
            window,
            cx,
        );
    }

    fn terminal_working_directory(
        &self,
        workspace: Option<&Workspace>,
        cx: &App,
    ) -> Option<PathBuf> {
        workspace
            .map(|workspace| terminal_view::default_working_directory(workspace, cx))
            .unwrap_or_else(|| self.default_terminal_working_directory(cx))
    }

    pub fn supports_terminal(&self, cx: &App) -> bool {
        self.has_open_project(cx) && self.project.read(cx).supports_terminal(cx)
    }

    pub fn should_create_terminal_for_new_entry(&self, cx: &App) -> bool {
        self.last_created_entry_kind == AgentPanelEntryKind::Terminal
            && self.project.read(cx).supports_terminal(cx)
    }

    fn set_last_created_entry_kind_from_user_action(
        &mut self,
        entry_kind: AgentPanelEntryKind,
        cx: &mut Context<Self>,
    ) {
        if self.last_created_entry_kind != entry_kind {
            self.last_created_entry_kind = entry_kind;
            self.serialize(cx);
        }

        cx.background_spawn({
            let kvp = KeyValueStore::global(cx);
            async move {
                write_global_last_created_entry_kind(kvp, entry_kind).await;
            }
        })
        .detach();
    }

    fn spawn_terminal(
        &mut self,
        terminal_id: TerminalId,
        working_directory: Option<PathBuf>,
        custom_title: Option<SharedString>,
        initial_title: Option<SharedString>,
        created_at: Option<DateTime<Utc>>,
        select: bool,
        focus: bool,
        run_init_command: bool,
        source: AgentThreadSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let terminal_working_directory = working_directory.clone();
        let init_command = Self::terminal_init_command(run_init_command, cx);
        let terminal_task = self.project.update(cx, |project, cx| {
            project.create_terminal_shell(working_directory, cx)
        });
        let workspace = self.workspace.clone();
        let workspace_id = self.workspace_id;
        let project = self.project.downgrade();

        cx.spawn_in(window, async move |this, cx| {
            let terminal = match terminal_task.await {
                Ok(terminal) => terminal,
                Err(error) => {
                    log::error!("failed to spawn agent panel terminal: {error:#}");
                    workspace
                        .update(cx, |workspace, cx| workspace.show_error(error, cx))
                        .log_err();
                    this.update(cx, |this, cx| {
                        if this.pending_terminal_spawn == Some(terminal_id) {
                            this.pending_terminal_spawn = None;
                            cx.notify();
                        }
                    })
                    .log_err();
                    return anyhow::Ok(());
                }
            };
            this.update_in(cx, |this, window, cx| {
                let terminal_for_init_command = terminal.clone();
                let terminal_view = cx.new(|cx| {
                    let mut view =
                        TerminalView::new(terminal, workspace, workspace_id, project, window, cx);
                    view.set_show_workspace_actions(false, cx);
                    view
                });
                this.insert_terminal(
                    terminal_id,
                    terminal_view,
                    terminal_working_directory,
                    custom_title,
                    initial_title,
                    created_at,
                    select,
                    focus,
                    source,
                    window,
                    cx,
                );
                Self::write_terminal_init_command(&terminal_for_init_command, init_command, cx);
            })?;
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    fn terminal_init_command(run_init_command: bool, cx: &App) -> Option<String> {
        run_init_command
            .then(|| AgentSettings::get_global(cx).terminal_init_command.clone())
            .flatten()
            .filter(|command| !command.trim().is_empty())
    }

    fn write_terminal_init_command(
        terminal: &Entity<terminal::Terminal>,
        init_command: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(command) = init_command else {
            return;
        };

        if !terminal.read(cx).is_pty() {
            terminal.update(cx, |terminal, _| {
                terminal.write_init_command(Self::terminal_init_command_input(command))
            });
            return;
        }

        let startup = terminal.update(cx, |terminal, _| {
            terminal.start_init_command_startup_handshake()
        });

        let terminal = terminal.downgrade();
        cx.spawn(async move |_this, cx| {
            // Fall back to the timeout so the init command is still delivered if
            // the shell never echoes the marker.
            let timeout = cx
                .background_executor()
                .timer(TERMINAL_INIT_COMMAND_STARTUP_TIMEOUT);
            futures::select_biased! {
                _ = startup.fuse() => {}
                _ = timeout.fuse() => {}
            }

            let input = Self::terminal_init_command_input(command);
            if let Err(error) = terminal.update(cx, move |terminal, cx| {
                if !terminal.write_init_command_after_startup(input, cx) {
                    log::debug!(
                        "skipping terminal init command because the terminal is no longer eligible"
                    );
                }
            }) {
                log::debug!("skipping terminal init command because the terminal closed: {error}");
            }
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    fn terminal_init_command_input(command: String) -> Vec<u8> {
        let mut input = command.into_bytes();
        // CR, not "\r\n": "\r\n" puts PowerShell into continuation
        // mode (same convention as the activation-script writes in
        // `TerminalBuilder::new`).
        input.push(b'\x0d');
        input
    }

    fn insert_terminal(
        &mut self,
        terminal_id: TerminalId,
        terminal_view: Entity<TerminalView>,
        working_directory: Option<PathBuf>,
        custom_title: Option<SharedString>,
        initial_title: Option<SharedString>,
        created_at: Option<DateTime<Utc>>,
        select: bool,
        focus: bool,
        source: AgentThreadSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(custom_title) = custom_title {
            terminal_view.update(cx, |terminal_view, cx| {
                terminal_view.set_custom_title(Some(custom_title.to_string()), cx);
            });
        }
        let terminal_entity = terminal_view.read(cx).terminal().clone();
        let view_subscription = cx.subscribe(
            &terminal_view,
            move |this, _terminal_view, event: &ItemEvent, cx| match event {
                ItemEvent::UpdateTab | ItemEvent::UpdateBreadcrumbs => {
                    this.refresh_terminal_metadata(terminal_id, cx);
                }
                ItemEvent::CloseItem | ItemEvent::Edit => {}
            },
        );
        // Listen on the underlying `Terminal` entity for shell-driven metadata
        // changes and bell.
        let terminal_subscription = cx.subscribe_in(
            &terminal_entity,
            window,
            move |this, _terminal, event: &TerminalEvent, window, cx| match event {
                TerminalEvent::TitleChanged
                | TerminalEvent::Wakeup
                | TerminalEvent::BreadcrumbsChanged => {
                    this.refresh_terminal_metadata(terminal_id, cx);
                    this.report_terminal_program(terminal_id, source, cx);
                }
                TerminalEvent::Bell => this.mark_terminal_notification(terminal_id, window, cx),
                TerminalEvent::CloseTerminal => {
                    this.close_terminal_from_terminal_event(terminal_id, window, cx);
                }
                TerminalEvent::BlinkChanged(_)
                | TerminalEvent::SelectionsChanged
                | TerminalEvent::NewNavigationTarget(_)
                | TerminalEvent::Open(_) => {}
            },
        );

        let last_known_terminal_title = initial_title
            .map(|title| title.to_string())
            .unwrap_or_default();
        let mut terminal = AgentTerminal {
            view: terminal_view,
            title_editor: None,
            title_editor_initial_title: None,
            title_editor_subscription: None,
            last_known_title: last_known_terminal_title.clone(),
            last_known_terminal_title,
            last_observed_program: None,
            working_directory,
            created_at: created_at.unwrap_or_else(Utc::now),
            has_notification: false,
            notification_windows: Vec::new(),
            notification_subscriptions: Vec::new(),
            _subscriptions: vec![view_subscription, terminal_subscription],
        };
        if self.pending_terminal_spawn == Some(terminal_id) {
            self.pending_terminal_spawn = None;
        }
        terminal.refresh_metadata(cx);
        terminal.report_started_terminal_program(terminal_id, source, cx);
        self.terminals.insert(terminal_id, terminal);
        self.persist_terminal_metadata(terminal_id, cx);
        self.emit_terminal_thread_started(terminal_id, source, cx);
        if select {
            self.set_base_view(BaseView::Terminal { terminal_id }, focus, window, cx);
        }
        cx.emit(AgentPanelEvent::EntryChanged);
        cx.notify();
    }

    pub fn activate_terminal(
        &mut self,
        terminal_id: TerminalId,
        focus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(terminal) = self.terminals.get_mut(&terminal_id) else {
            return;
        };
        let had_notification = terminal.has_notification;
        terminal.has_notification = false;
        if had_notification {
            self.dismiss_terminal_notifications(terminal_id, cx);
        }
        self.set_base_view(BaseView::Terminal { terminal_id }, focus, window, cx);
        if had_notification {
            cx.emit(AgentPanelEvent::EntryChanged);
            cx.notify();
        }
    }

    pub fn close_terminal(
        &mut self,
        terminal_id: TerminalId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_terminal_internal(terminal_id, true, None, window, cx);
    }

    pub fn close_terminal_without_activating_draft(
        &mut self,
        terminal_id: TerminalId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_terminal_internal(terminal_id, false, None, window, cx);
    }

    fn close_terminal_internal(
        &mut self,
        terminal_id: TerminalId,
        activate_draft_after_close: bool,
        terminal_closed_metadata: Option<TerminalThreadMetadata>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let was_active = self.active_terminal_id() == Some(terminal_id);

        if self.pending_terminal_spawn == Some(terminal_id) {
            self.pending_terminal_spawn = None;
        }
        self.dismiss_terminal_notifications(terminal_id, cx);
        if self.terminals.remove(&terminal_id).is_none() {
            return;
        }
        if let Some(store) = TerminalThreadMetadataStore::try_global(cx) {
            store.update(cx, |store, cx| {
                store.delete(terminal_id, cx);
            });
        }
        if was_active {
            self.base_view = BaseView::Uninitialized;
            self.refresh_base_view_subscriptions(window, cx);
            if activate_draft_after_close {
                self.activate_draft(false, AgentThreadSource::AgentPanel, window, cx);
            }
        }

        if let Some(metadata) = terminal_closed_metadata {
            cx.emit(AgentPanelEvent::TerminalClosed { metadata });
        }
        cx.emit(AgentPanelEvent::EntryChanged);
        cx.notify();
    }

    fn close_terminal_from_terminal_event(
        &mut self,
        terminal_id: TerminalId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let metadata = self.terminal_metadata(terminal_id, cx);
        self.close_terminal_internal(terminal_id, false, metadata, window, cx);
    }

    fn emit_terminal_thread_started(
        &self,
        terminal_id: TerminalId,
        source: AgentThreadSource,
        cx: &App,
    ) {
        telemetry::event!(
            "Agent Thread Started",
            agent = TERMINAL_AGENT_TELEMETRY_ID,
            terminal_id = terminal_id.to_key_string(),
            source = source.as_str(),
            side = crate::sidebar_side(cx),
            thread_location = "current_worktree",
        );
    }

    fn refresh_terminal_metadata(&mut self, terminal_id: TerminalId, cx: &mut Context<Self>) {
        if let Some(terminal) = self.terminals.get_mut(&terminal_id)
            && terminal.refresh_metadata(cx)
        {
            self.persist_terminal_metadata(terminal_id, cx);
            cx.emit(AgentPanelEvent::EntryChanged);
            cx.notify();
        }
    }

    fn report_terminal_program(
        &mut self,
        terminal_id: TerminalId,
        source: AgentThreadSource,
        cx: &mut Context<Self>,
    ) {
        if let Some(terminal) = self.terminals.get_mut(&terminal_id) {
            terminal.report_started_terminal_program(terminal_id, source, cx);
        }
    }

    fn persist_all_terminal_metadata(&self, cx: &mut Context<Self>) {
        let terminal_ids = self.terminals.keys().copied().collect::<Vec<_>>();
        for terminal_id in terminal_ids {
            self.persist_terminal_metadata(terminal_id, cx);
        }
    }

    fn persist_terminal_metadata(&self, terminal_id: TerminalId, cx: &mut Context<Self>) {
        let Some(store) = TerminalThreadMetadataStore::try_global(cx) else {
            return;
        };
        let Some(metadata) = self.terminal_metadata(terminal_id, cx) else {
            return;
        };
        store.update(cx, |store, cx| {
            store.save(metadata, cx);
        });
    }

    fn terminal_metadata(
        &self,
        terminal_id: TerminalId,
        cx: &App,
    ) -> Option<TerminalThreadMetadata> {
        let terminal = self.terminals.get(&terminal_id)?;
        let project = self.project.read(cx);
        Some(TerminalThreadMetadata {
            terminal_id,
            title: terminal.terminal_title(cx),
            custom_title: terminal.custom_title(cx),
            created_at: terminal.created_at,
            worktree_paths: project.worktree_paths(cx),
            remote_connection: project.remote_connection_options(cx),
            working_directory: terminal.working_directory.clone(),
        })
    }

    pub fn restore_terminal(
        &mut self,
        metadata: TerminalThreadMetadata,
        focus: bool,
        source: AgentThreadSource,
        workspace: Option<&Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.has_terminal(metadata.terminal_id) {
            self.activate_terminal(metadata.terminal_id, focus, window, cx);
            return;
        }

        if !self.supports_terminal(cx) {
            return;
        }

        self.pending_terminal_spawn = Some(metadata.terminal_id);
        let working_directory = self.terminal_restore_working_directory(&metadata, workspace, cx);
        let initial_title = Self::terminal_restore_initial_title(&metadata);
        self.spawn_terminal(
            metadata.terminal_id,
            working_directory,
            metadata.custom_title.clone(),
            initial_title,
            Some(metadata.created_at),
            true,
            focus,
            true,
            source,
            window,
            cx,
        );
    }

    fn restore_terminal_for_panel_load(
        &mut self,
        metadata: TerminalThreadMetadata,
        focus: bool,
        source: AgentThreadSource,
        workspace: Option<&Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        #[cfg(test)]
        self.restore_test_terminal(metadata, focus, source, workspace, window, cx)
            .log_err();

        #[cfg(not(test))]
        self.restore_terminal(metadata, focus, source, workspace, window, cx);
    }

    fn terminal_restore_working_directory(
        &self,
        metadata: &TerminalThreadMetadata,
        workspace: Option<&Workspace>,
        cx: &App,
    ) -> Option<PathBuf> {
        if let Some(working_directory) = metadata.working_directory.clone() {
            return Some(working_directory);
        }

        if let Some(workspace) = workspace {
            return terminal_view::default_working_directory(workspace, cx);
        }

        self.default_terminal_working_directory(cx)
    }

    fn terminal_restore_initial_title(metadata: &TerminalThreadMetadata) -> Option<SharedString> {
        (!metadata.title.is_empty()).then(|| metadata.title.clone())
    }

    pub fn dismiss_all_notifications(&mut self, cx: &mut Context<Self>) -> bool {
        let mut dismissed = false;
        for conversation_view in self.conversation_views() {
            dismissed |= conversation_view.update(cx, |view, cx| view.dismiss_notifications(cx));
        }
        let had_terminal_notifications = self
            .terminals
            .values()
            .any(|t| !t.notification_windows.is_empty());
        if had_terminal_notifications {
            self.dismiss_all_terminal_notifications(cx);
            dismissed = true;
        }
        dismissed
    }

    fn manage_skills(
        &mut self,
        _action: &ManageSkills,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.dispatch_action(
            Box::new(mav_actions::OpenSettingsAt {
                path: mav_actions::AGENT_SKILLS_SETTINGS_PATH.to_string(),
                target: None,
            }),
            cx,
        );
    }

    /// Refresh the native agent's view of available skills
    pub fn refresh_skills(&mut self, cx: &mut Context<Self>) {
        if !self.has_open_project(cx) {
            return;
        }

        self.ensure_native_agent_connection(cx);
        let Some(connect_task) = self.connection_store.update(cx, |store, cx| {
            store
                .entry(&Agent::NativeAgent)
                .map(|entry| entry.read(cx).wait_for_connection())
        }) else {
            return;
        };
        let project = self.project.clone();
        cx.spawn(async move |_this, cx| -> Result<()> {
            let connected = connect_task.await?;
            if let Some(native_connection) = connected
                .connection
                .downcast::<agent::NativeAgentConnection>()
            {
                cx.update(|cx| native_connection.refresh_skills_for_project(project, cx));
            }
            Ok(())
        })
        .detach_and_log_err(cx);
    }

    fn expand_message_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(conversation_view) = self.active_conversation_view() else {
            return;
        };

        let Some(active_thread) = conversation_view.read(cx).root_thread_view() else {
            return;
        };

        active_thread.update(cx, |active_thread, cx| {
            active_thread.expand_message_editor(&ExpandMessageEditor, window, cx);
            active_thread.focus_handle(cx).focus(window, cx);
        })
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

    #[derive(Clone, Default)]
    struct SessionTrackingConnection {
        next_session_number: Arc<Mutex<usize>>,
        sessions: Arc<Mutex<HashSet<acp::SessionId>>>,
    }

    impl SessionTrackingConnection {
        fn new() -> Self {
            Self::default()
        }

        fn create_session(
            self: Rc<Self>,
            session_id: acp::SessionId,
            project: Entity<Project>,
            work_dirs: PathList,
            title: Option<SharedString>,
            cx: &mut App,
        ) -> Entity<AcpThread> {
            self.sessions.lock().insert(session_id.clone());

            let action_log = cx.new(|_| ActionLog::new(project.clone()));
            cx.new(|cx| {
                AcpThread::new(
                    None,
                    title,
                    Some(work_dirs),
                    self,
                    project,
                    action_log,
                    session_id,
                    watch::Receiver::constant(
                        acp::PromptCapabilities::new()
                            .image(true)
                            .audio(true)
                            .embedded_context(true),
                    ),
                    cx,
                )
            })
        }
    }

    impl AgentConnection for SessionTrackingConnection {
        fn agent_id(&self) -> AgentId {
            agent::MAV_AGENT_ID.clone()
        }

        fn telemetry_id(&self) -> SharedString {
            "session-tracking-test".into()
        }

        fn new_session(
            self: Rc<Self>,
            project: Entity<Project>,
            work_dirs: PathList,
            cx: &mut App,
        ) -> Task<Result<Entity<AcpThread>>> {
            let session_id = {
                let mut next_session_number = self.next_session_number.lock();
                let session_id = acp::SessionId::new(format!(
                    "session-tracking-session-{}",
                    *next_session_number
                ));
                *next_session_number += 1;
                session_id
            };
            let thread = self.create_session(session_id, project, work_dirs, None, cx);
            Task::ready(Ok(thread))
        }

        fn supports_load_session(&self) -> bool {
            true
        }

        fn load_session(
            self: Rc<Self>,
            session_id: acp::SessionId,
            project: Entity<Project>,
            work_dirs: PathList,
            title: Option<SharedString>,
            cx: &mut App,
        ) -> Task<Result<Entity<AcpThread>>> {
            let thread = self.create_session(session_id, project, work_dirs, title, cx);
            thread.update(cx, |thread, cx| {
                thread
                    .handle_session_update(
                        acp::SessionUpdate::UserMessageChunk(acp::ContentChunk::new(
                            "Restored user message".into(),
                        )),
                        cx,
                    )
                    .expect("restored user message should be applied");
                thread
                    .handle_session_update(
                        acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk::new(
                            "Restored assistant message".into(),
                        )),
                        cx,
                    )
                    .expect("restored assistant message should be applied");
            });
            Task::ready(Ok(thread))
        }

        fn supports_close_session(&self) -> bool {
            true
        }

        fn close_session(
            self: Rc<Self>,
            session_id: &acp::SessionId,
            _cx: &mut App,
        ) -> Task<Result<()>> {
            self.sessions.lock().remove(session_id);
            Task::ready(Ok(()))
        }

        fn auth_methods(&self) -> &[acp::AuthMethod] {
            &[]
        }

        fn authenticate(&self, _method_id: acp::AuthMethodId, _cx: &mut App) -> Task<Result<()>> {
            Task::ready(Ok(()))
        }

        fn prompt(
            &self,
            params: acp::PromptRequest,
            _cx: &mut App,
        ) -> Task<Result<acp::PromptResponse>> {
            if !self.sessions.lock().contains(&params.session_id) {
                return Task::ready(Err(anyhow!("Session not found")));
            }

            Task::ready(Ok(acp::PromptResponse::new(acp::StopReason::EndTurn)))
        }

        fn cancel(&self, _session_id: &acp::SessionId, _cx: &mut App) {}

        fn into_any(self: Rc<Self>) -> Rc<dyn Any> {
            self
        }
    }

    #[gpui::test]
    async fn test_active_thread_serialize_and_load_round_trip(cx: &mut TestAppContext) {
        init_test(cx);
        cx.update(|cx| {
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
        });

        // Create a MultiWorkspace window with two workspaces.
        let fs = FakeFs::new(cx.executor());
        fs.insert_tree("/project_a", json!({ "file.txt": "" }))
            .await;
        let project_a = Project::test(fs.clone(), [Path::new("/project_a")], cx).await;
        let project_b = Project::test(fs, [], cx).await;

        let multi_workspace =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));

        let workspace_a = multi_workspace
            .read_with(cx, |multi_workspace, _cx| {
                multi_workspace.workspace().clone()
            })
            .unwrap();

        let workspace_b = multi_workspace
            .update(cx, |multi_workspace, window, cx| {
                multi_workspace.test_add_workspace(project_b.clone(), window, cx)
            })
            .unwrap();

        workspace_a.update(cx, |workspace, _cx| {
            workspace.set_random_database_id();
        });
        workspace_b.update(cx, |workspace, _cx| {
            workspace.set_random_database_id();
        });

        let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);

        // Set up workspace A: with an active thread.
        let panel_a = workspace_a.update_in(cx, |workspace, window, cx| {
            cx.new(|cx| AgentPanel::new(workspace, window, cx))
        });

        panel_a.update_in(cx, |panel, window, cx| {
            panel.open_external_thread_with_server(
                Rc::new(StubAgentServer::default_response()),
                window,
                cx,
            );
        });

        cx.run_until_parked();

        panel_a.read_with(cx, |panel, cx| {
            assert!(
                panel.active_agent_thread(cx).is_some(),
                "workspace A should have an active thread after connection"
            );
        });

        send_message(&panel_a, cx);

        let agent_type_a = panel_a.read_with(cx, |panel, _cx| panel.selected_agent.clone());

        // Set up workspace B: ClaudeCode, no active thread.
        let panel_b = workspace_b.update_in(cx, |workspace, window, cx| {
            cx.new(|cx| AgentPanel::new(workspace, window, cx))
        });

        panel_b.update(cx, |panel, _cx| {
            panel.selected_agent = Agent::Custom {
                id: "claude-acp".into(),
            };
        });

        // Serialize both panels.
        panel_a.update(cx, |panel, cx| panel.serialize(cx));
        panel_b.update(cx, |panel, cx| panel.serialize(cx));
        cx.run_until_parked();

        let workspace_a_id = workspace_a
            .read_with(cx, |workspace, _cx| workspace.database_id())
            .expect("workspace A should have a database id");
        let kvp = cx.update(|_window, cx| KeyValueStore::global(cx));
        let serialized_a: SerializedAgentPanel = cx
            .background_spawn(async move { read_serialized_panel(workspace_a_id, &kvp) })
            .await
            .expect("workspace A should serialize panel state");
        assert!(
            serialized_a.last_active_thread.is_some(),
            "active thread should be the thread restore target"
        );
        assert!(
            serialized_a.last_active_terminal_id.is_none(),
            "active thread serialization should not also include a terminal restore target"
        );

        cx.update(|_window, cx| {
            ThreadMetadataStore::init_global(cx);
        });

        // Load fresh panels for each workspace and verify independent state.
        let async_cx = cx.update(|window, cx| window.to_async(cx));
        let loaded_a = AgentPanel::load(workspace_a.downgrade(), async_cx)
            .await
            .expect("panel A load should succeed");
        cx.run_until_parked();

        let async_cx = cx.update(|window, cx| window.to_async(cx));
        let loaded_b = AgentPanel::load(workspace_b.downgrade(), async_cx)
            .await
            .expect("panel B load should succeed");
        cx.run_until_parked();

        // Workspace A should restore its thread and agent type
        loaded_a.read_with(cx, |panel, _cx| {
            assert_eq!(
                panel.selected_agent, agent_type_a,
                "workspace A agent type should be restored"
            );
            assert!(
                panel.active_conversation_view().is_some(),
                "workspace A should have its active thread restored"
            );
        });

        // Workspace B should restore its own agent type but have no active thread.
        loaded_b.read_with(cx, |panel, _cx| {
            assert_eq!(
                panel.selected_agent,
                Agent::Custom {
                    id: "claude-acp".into()
                },
                "workspace B agent type should be restored"
            );
            assert!(
                panel.active_conversation_view().is_none(),
                "workspace B should have no active thread when it had no prior conversation"
            );
        });
    }

    #[gpui::test]
    async fn test_active_terminal_serialize_and_load_round_trip(cx: &mut TestAppContext) {
        init_test(cx);
        cx.update(|cx| {
            agent::ThreadStore::init_global(cx);
            TerminalThreadMetadataStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
        });

        let fs = FakeFs::new(cx.executor());
        fs.insert_tree("/project", json!({ "file.txt": "" })).await;
        cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));
        let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;

        let multi_workspace =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = multi_workspace
            .read_with(cx, |multi_workspace, _cx| {
                multi_workspace.workspace().clone()
            })
            .unwrap();
        workspace.update(cx, |workspace, _cx| {
            workspace.set_random_database_id();
        });

        let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);
        let panel = workspace.update_in(cx, |workspace, window, cx| {
            cx.new(|cx| AgentPanel::new(workspace, window, cx))
        });

        panel.update_in(cx, |panel, window, cx| {
            panel.activate_new_thread(false, AgentThreadSource::AgentPanel, window, cx);
        });
        let terminal_id = panel
            .update_in(cx, |panel, window, cx| {
                panel.insert_test_terminal("Dev Server", true, window, cx)
            })
            .expect("test terminal should be inserted");
        panel.update(cx, |panel, cx| panel.serialize(cx));
        cx.run_until_parked();

        let workspace_id = workspace
            .read_with(cx, |workspace, _cx| workspace.database_id())
            .expect("workspace should have a database id");
        let kvp = cx.update(|_window, cx| KeyValueStore::global(cx));
        let serialized: SerializedAgentPanel = cx
            .background_spawn(async move { read_serialized_panel(workspace_id, &kvp) })
            .await
            .expect("workspace should serialize panel state");
        assert_eq!(
            serialized.last_active_terminal_id,
            Some(terminal_id.to_key_string())
        );
        assert!(
            serialized.last_active_thread.is_none(),
            "active terminal serialization should not also include a thread restore target"
        );

        cx.update(|_window, cx| {
            TerminalThreadMetadataStore::init_global(cx);
        });
        let async_cx = cx.update(|window, cx| window.to_async(cx));
        let loaded = AgentPanel::load(workspace.downgrade(), async_cx)
            .await
            .expect("panel load should succeed");
        for _ in 0..8 {
            cx.run_until_parked();
        }

        loaded.read_with(cx, |panel, cx| {
            assert_eq!(panel.active_terminal_id(), Some(terminal_id));
            assert!(
                panel.active_conversation_view().is_none(),
                "the restored terminal should remain active instead of falling back to a draft"
            );
            assert!(
                panel
                    .terminals(cx)
                    .into_iter()
                    .any(|terminal| terminal.id == terminal_id),
                "active terminal metadata should be restored into the loaded panel"
            );
        });
    }

    #[gpui::test]
    async fn test_terminal_restore_working_directory_does_not_read_leased_workspace(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);
        cx.update(|cx| {
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);

            SettingsStore::update_global(cx, |store, cx| {
                store.update_user_settings(cx, |settings| {
                    settings
                        .terminal
                        .get_or_insert_default()
                        .project
                        .working_directory = Some(WorkingDirectory::AlwaysHome);
                });
            });
        });

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        project.update(cx, |project, _cx| {
            project.mark_as_collab_for_testing();
        });
        project.read_with(cx, |project, _cx| {
            assert!(project.is_remote());
        });

        let multi_workspace =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = multi_workspace
            .read_with(cx, |multi_workspace, _cx| {
                multi_workspace.workspace().clone()
            })
            .expect("multi workspace should have an active workspace");
        let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);
        let panel = workspace.update_in(cx, |workspace, window, cx| {
            cx.new(|cx| AgentPanel::new(workspace, window, cx))
        });

        assert_eq!(
            workspace.read_with(cx, |workspace, cx| {
                terminal_view::default_working_directory(workspace, cx)
            }),
            None
        );

        let metadata = TerminalThreadMetadata {
            terminal_id: TerminalId::new(),
            title: "Dev Server".into(),
            custom_title: None,
            created_at: Utc::now(),
            worktree_paths: project.read_with(cx, |project, cx| project.worktree_paths(cx)),
            remote_connection: None,
            working_directory: None,
        };
        assert_eq!(metadata.working_directory, None);

        let working_directory = workspace.update_in(cx, |workspace, _window, cx| {
            panel
                .read(cx)
                .terminal_restore_working_directory(&metadata, Some(workspace), cx)
        });

        assert_eq!(working_directory, None);
    }

    #[gpui::test]
    async fn test_pending_terminal_restore_prevents_initial_terminal_creation(
        cx: &mut TestAppContext,
    ) {
        let (panel, mut cx) = setup_panel(cx).await;

        panel.update_in(&mut cx, |panel, window, cx| {
            panel.last_created_entry_kind = AgentPanelEntryKind::Terminal;
            panel.pending_terminal_spawn = Some(TerminalId::new());
            panel.set_active(true, window, cx);
        });
        for _ in 0..4 {
            cx.run_until_parked();
        }

        panel.read_with(&cx, |panel, cx| {
            assert!(
                panel.terminals(cx).is_empty(),
                "activation while a terminal restore is pending should not create a second terminal"
            );
            assert!(
                panel.active_conversation_view().is_none(),
                "activation while a terminal restore is pending should not fall back to a draft"
            );
        });
    }

    #[gpui::test]
    async fn test_repeated_activation_only_creates_one_initial_terminal(cx: &mut TestAppContext) {
        let (panel, mut cx) = setup_panel(cx).await;

        panel.update_in(&mut cx, |panel, window, cx| {
            panel.last_created_entry_kind = AgentPanelEntryKind::Terminal;
            panel.set_active(true, window, cx);
            panel.set_active(true, window, cx);
        });
        for _ in 0..8 {
            cx.run_until_parked();
        }

        panel.read_with(&cx, |panel, cx| {
            assert_eq!(
                panel.terminals(cx).len(),
                1,
                "repeated activation should only enqueue one initial terminal"
            );
            assert!(
                panel.active_terminal_id().is_some(),
                "the single initial terminal should become active"
            );
        });
    }

    #[gpui::test]
    async fn test_restored_terminal_runs_init_command_once(cx: &mut TestAppContext) {
        let (panel, mut cx) = setup_panel(cx).await;
        cx.update(|_, cx| {
            let mut settings = AgentSettings::get_global(cx).clone();
            settings.terminal_init_command = Some(" claude --resume ".to_string());
            AgentSettings::override_global(settings, cx);
        });

        let metadata = TerminalThreadMetadata {
            terminal_id: TerminalId::new(),
            title: "Restored Terminal".into(),
            custom_title: None,
            created_at: Utc::now(),
            worktree_paths: WorktreePaths::from_folder_paths(&PathList::new(&[PathBuf::from(
                "/project",
            )])),
            remote_connection: None,
            working_directory: None,
        };
        let terminal_id = metadata.terminal_id;
        panel
            .update_in(&mut cx, |panel, window, cx| {
                panel.restore_test_terminal(
                    metadata.clone(),
                    true,
                    AgentThreadSource::AgentPanel,
                    None,
                    window,
                    cx,
                )
            })
            .expect("test terminal should be restored");
        cx.run_until_parked();

        let terminal = panel.read_with(&cx, |panel, cx| {
            panel
                .terminals
                .get(&terminal_id)
                .expect("terminal should exist")
                .view
                .read(cx)
                .terminal()
                .clone()
        });
        let input_log = terminal.update(&mut cx, |terminal, _| terminal.take_input_log());
        assert_eq!(input_log, vec![b" claude --resume \r".to_vec()]);
        assert!(
            !terminal.read_with(&cx, |terminal, _| terminal.keyboard_input_sent()),
            "writing the init command must not mark the terminal as having received \
             user keyboard input, otherwise a shell that fails to spawn would be \
             auto-closed before the user can see the error"
        );

        panel
            .update_in(&mut cx, |panel, window, cx| {
                panel.restore_test_terminal(
                    metadata,
                    true,
                    AgentThreadSource::AgentPanel,
                    None,
                    window,
                    cx,
                )
            })
            .expect("restoring an existing test terminal should succeed");
        cx.run_until_parked();

        let input_log = terminal.update(&mut cx, |terminal, _| terminal.take_input_log());
        assert!(
            input_log.is_empty(),
            "activating an already-restored terminal should not re-run the init command, got {input_log:?}"
        );
    }

    /// Exercises the real `spawn_terminal` path with a genuine shell PTY (not the
    /// display-only test terminal, where `write_to_pty` is a no-op) to verify the
    /// init command is actually delivered to the shell and executed.
    #[cfg(unix)]
    #[gpui::test]
    async fn test_spawn_terminal_runs_init_command_in_real_shell(cx: &mut TestAppContext) {
        let (panel, mut cx) = setup_panel(cx).await;
        cx.executor().allow_parking();
        cx.update(|_, cx| {
            let mut settings = AgentSettings::get_global(cx).clone();
            // `init_ran_42` is the command's output, not its echoed text, so finding
            // it proves the shell executed the command rather than just echoing it.
            settings.terminal_init_command = Some("printf 'init_ran_%s\\n' 42".to_string());
            AgentSettings::override_global(settings, cx);

            // Force a known POSIX shell so the test doesn't depend on the developer's login shell.
            let mut terminal_settings =
                terminal::terminal_settings::TerminalSettings::get_global(cx).clone();
            terminal_settings.shell = task::Shell::Program("/bin/sh".to_string());
            terminal::terminal_settings::TerminalSettings::override_global(terminal_settings, cx);
        });

        let terminal_id = TerminalId::new();
        panel.update_in(&mut cx, |panel, window, cx| {
            panel.spawn_terminal(
                terminal_id,
                // No working directory: the FakeFs project path doesn't exist on
                // the real filesystem the shell process runs against.
                None,
                None,
                None,
                None,
                true,
                true,
                true,
                AgentThreadSource::AgentPanel,
                window,
                cx,
            );
        });

        // The shell spawns on a background thread and produces output
        // asynchronously, so poll (with a deadline) rather than using a fixed
        // sleep, matching the real-PTY test in `acp_thread`.
        let deadline = Instant::now() + Duration::from_secs(10);
        let terminal = loop {
            cx.run_until_parked();
            let terminal = panel.read_with(&cx, |panel, cx| {
                panel
                    .terminals
                    .get(&terminal_id)
                    .map(|terminal| terminal.view.read(cx).terminal().clone())
            });
            if let Some(terminal) = &terminal
                && terminal
                    .read_with(&cx, |terminal, _| terminal.get_content())
                    .contains("init_ran_42")
            {
                break terminal.clone();
            }
            if Instant::now() >= deadline {
                let terminal_created = terminal.is_some();
                let (content, input_log) = if let Some(terminal) = terminal {
                    let content = terminal.read_with(&cx, |terminal, _| terminal.get_content());
                    let input_log =
                        terminal.update(&mut cx, |terminal, _| terminal.take_input_log());
                    (content, input_log)
                } else {
                    (String::new(), Vec::new())
                };
                panic!(
                    "init command output never appeared in the terminal; terminal_created={terminal_created}, content={content:?}, input_log={input_log:?}"
                );
            }
            cx.executor().timer(Duration::from_millis(50)).await;
        };

        let input_log = terminal.update(&mut cx, |terminal, _| terminal.take_input_log());
        assert_eq!(
            input_log,
            vec![b"printf 'init_ran_%s\\n' 42\r".to_vec()],
            "init command should be written only after terminal startup has settled"
        );
        assert!(
            !terminal.read_with(&cx, |terminal, _| terminal.keyboard_input_sent()),
            "writing the init command must not mark the terminal as having received \
             user keyboard input"
        );
    }

    #[gpui::test]
    async fn test_restored_terminal_does_not_update_global_entry_kind(cx: &mut TestAppContext) {
        let (panel, mut cx) = setup_panel(cx).await;
        cx.update(|_, cx| {
            TerminalThreadMetadataStore::init_global(cx);
        });

        panel.update_in(&mut cx, |panel, window, cx| {
            panel.activate_new_thread(false, AgentThreadSource::AgentPanel, window, cx);
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert_eq!(
                read_global_last_created_entry_kind(&KeyValueStore::global(cx)),
                Some(AgentPanelEntryKind::Thread)
            );
        });

        let metadata = TerminalThreadMetadata {
            terminal_id: TerminalId::new(),
            title: "Restored Terminal".into(),
            custom_title: None,
            created_at: Utc::now(),
            worktree_paths: WorktreePaths::from_folder_paths(&PathList::new(&[PathBuf::from(
                "/project",
            )])),
            remote_connection: None,
            working_directory: None,
        };
        panel
            .update_in(&mut cx, |panel, window, cx| {
                panel.restore_test_terminal(
                    metadata,
                    true,
                    AgentThreadSource::AgentPanel,
                    None,
                    window,
                    cx,
                )
            })
            .expect("test terminal should be restored");
        cx.run_until_parked();

        cx.update(|_, cx| {
            assert_eq!(
                read_global_last_created_entry_kind(&KeyValueStore::global(cx)),
                Some(AgentPanelEntryKind::Thread),
                "restoring a terminal should not change the global new-entry default"
            );
        });
    }

    #[gpui::test]
    async fn test_new_workspace_load_uses_global_terminal_entry_kind(cx: &mut TestAppContext) {
        init_test(cx);
        cx.update(|cx| {
            agent::ThreadStore::init_global(cx);
            TerminalThreadMetadataStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
        });

        let fs = FakeFs::new(cx.executor());
        fs.insert_tree("/project-a", json!({ "file.txt": "" }))
            .await;
        fs.insert_tree("/project-b", json!({ "file.txt": "" }))
            .await;
        cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

        let project_a = Project::test(fs.clone(), [Path::new("/project-a")], cx).await;
        let project_b = Project::test(fs.clone(), [Path::new("/project-b")], cx).await;
        let multi_workspace =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));
        let multi_workspace_entity = multi_workspace.root(cx).unwrap();
        let workspace_a = multi_workspace
            .read_with(cx, |multi_workspace, _cx| {
                multi_workspace.workspace().clone()
            })
            .unwrap();
        workspace_a.update(cx, |workspace, _cx| {
            workspace.set_random_database_id();
        });

        let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);
        let panel_a = workspace_a.update_in(cx, |workspace, window, cx| {
            cx.new(|cx| AgentPanel::new(workspace, window, cx))
        });
        panel_a
            .update_in(cx, |panel, window, cx| {
                panel.insert_test_terminal("Dev Server", true, window, cx)
            })
            .expect("test terminal should be inserted");
        cx.run_until_parked();

        cx.update(|_window, cx| {
            assert_eq!(
                read_global_last_created_entry_kind(&KeyValueStore::global(cx)),
                Some(AgentPanelEntryKind::Terminal)
            );
        });

        let workspace_b = multi_workspace_entity.update_in(cx, |multi_workspace, window, cx| {
            multi_workspace.test_add_workspace(project_b.clone(), window, cx)
        });
        workspace_b.update(cx, |workspace, _cx| {
            workspace.set_random_database_id();
        });

        let async_cx = cx.update(|window, cx| window.to_async(cx));
        let loaded = AgentPanel::load(workspace_b.downgrade(), async_cx)
            .await
            .expect("panel load should succeed");
        workspace_b.update_in(cx, |workspace, window, cx| {
            workspace.add_panel(loaded.clone(), window, cx);
        });
        loaded.update_in(cx, |panel, window, cx| {
            panel.set_active(true, window, cx);
        });
        for _ in 0..8 {
            cx.run_until_parked();
        }

        loaded.read_with(cx, |panel, cx| {
            assert!(
                panel.active_terminal_id().is_some(),
                "new workspace should initialize to a terminal when terminal was the globally last used entry kind"
            );
            assert!(
                panel.active_conversation_view().is_none(),
                "new workspace should not initialize to a draft when terminal is the global entry kind"
            );
            assert!(panel.should_create_terminal_for_new_entry(cx));
        });
    }

    #[gpui::test]
    async fn test_non_native_thread_without_metadata_is_not_restored(cx: &mut TestAppContext) {
        init_test(cx);
        cx.update(|cx| {
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
        });

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;

        let multi_workspace =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));

        let workspace = multi_workspace
            .read_with(cx, |multi_workspace, _cx| {
                multi_workspace.workspace().clone()
            })
            .unwrap();

        workspace.update(cx, |workspace, _cx| {
            workspace.set_random_database_id();
        });

        let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);

        let panel = workspace.update_in(cx, |workspace, window, cx| {
            cx.new(|cx| AgentPanel::new(workspace, window, cx))
        });

        panel.update_in(cx, |panel, window, cx| {
            panel.open_external_thread_with_server(
                Rc::new(StubAgentServer::default_response()),
                window,
                cx,
            );
        });

        cx.run_until_parked();

        panel.read_with(cx, |panel, cx| {
            assert!(
                panel.active_agent_thread(cx).is_some(),
                "should have an active thread after connection"
            );
        });

        // Serialize without ever sending a message, so no thread metadata exists.
        panel.update(cx, |panel, cx| panel.serialize(cx));
        cx.run_until_parked();

        let async_cx = cx.update(|window, cx| window.to_async(cx));
        let loaded = AgentPanel::load(workspace.downgrade(), async_cx)
            .await
            .expect("panel load should succeed");
        cx.run_until_parked();

        loaded.read_with(cx, |panel, _cx| {
            assert!(
                panel.active_conversation_view().is_none(),
                "thread without metadata should not be restored; the panel should have no active thread"
            );
        });
    }

    #[gpui::test]
    async fn test_serialize_preserves_session_id_in_load_error(cx: &mut TestAppContext) {
        use crate::conversation_view::tests::FlakyAgentServer;
        use crate::thread_metadata_store::{ThreadId, ThreadMetadata};
        use chrono::Utc;
        use project::{AgentId as ProjectAgentId, WorktreePaths};

        init_test(cx);
        cx.update(|cx| {
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
        });

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;

        let multi_workspace =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = multi_workspace
            .read_with(cx, |mw, _cx| mw.workspace().clone())
            .unwrap();
        workspace.update(cx, |workspace, _cx| {
            workspace.set_random_database_id();
        });
        let workspace_id = workspace
            .read_with(cx, |workspace, _cx| workspace.database_id())
            .expect("workspace should have a database id");

        let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);

        // Simulate a previous run that persisted metadata for this session.
        let resume_session_id = acp::SessionId::new("persistent-session");
        cx.update(|_window, cx| {
            ThreadMetadataStore::global(cx).update(cx, |store, cx| {
                store.save(
                    ThreadMetadata {
                        thread_id: ThreadId::new(),
                        session_id: Some(resume_session_id.clone()),
                        agent_id: ProjectAgentId::new("Flaky"),
                        title: Some("Persistent chat".into()),
                        title_override: None,
                        updated_at: Utc::now(),
                        created_at: Some(Utc::now()),
                        interacted_at: None,
                        worktree_paths: WorktreePaths::from_folder_paths(&PathList::default()),
                        remote_connection: None,
                        archived: false,
                    },
                    cx,
                );
            });
        });

        let panel = workspace.update_in(cx, |workspace, window, cx| {
            cx.new(|cx| AgentPanel::new(workspace, window, cx))
        });

        // Open a restored thread using a flaky server so the initial connect
        // fails and the view lands in LoadError — mirroring the cold-start
        // race against a custom agent over SSH.
        let (server, _fail) =
            FlakyAgentServer::new(StubAgentConnection::new().with_supports_load_session(true));
        panel.update_in(cx, |panel, window, cx| {
            panel.open_restored_thread_with_server(
                Rc::new(server),
                resume_session_id.clone(),
                window,
                cx,
            );
        });
        cx.run_until_parked();

        // Sanity: the view couldn't connect, so no live AcpThread exists.
        panel.read_with(cx, |panel, cx| {
            assert!(
                panel.active_agent_thread(cx).is_none(),
                "active_agent_thread should be None while the flaky server is failing"
            );
            let conversation_view = panel
                .active_conversation_view()
                .expect("panel should still have an active ConversationView");
            assert_eq!(
                conversation_view.read(cx).root_session_id.as_ref(),
                Some(&resume_session_id),
                "ConversationView should still hold the restored session id"
            );
        });

        // Serialize while in LoadError. Before the fix this wrote
        // `session_id=None` to the KVP and permanently lost the session.
        panel.update(cx, |panel, cx| panel.serialize(cx));
        cx.run_until_parked();

        let kvp = cx.update(|_window, cx| KeyValueStore::global(cx));
        let serialized: Option<SerializedAgentPanel> = cx
            .background_spawn(async move { read_serialized_panel(workspace_id, &kvp) })
            .await;
        let serialized_session_id = serialized
            .as_ref()
            .and_then(|p| p.last_active_thread.as_ref())
            .and_then(|t| t.session_id.clone());
        assert_eq!(
            serialized_session_id,
            Some(resume_session_id.0.to_string()),
            "serialize() must preserve the restored session id even while the \
             ConversationView is in LoadError; otherwise the bug survives a \
             restart because the KVP has been wiped"
        );
    }

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

    #[gpui::test]
    async fn test_draft_prompt_blocks_use_current_editor_snapshot(cx: &mut TestAppContext) {
        init_test(cx);
        cx.update(|cx| {
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
        });

        let fs = FakeFs::new(cx.executor());
        fs.insert_tree("/project", json!({ "file.txt": "" })).await;
        let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;

        let multi_workspace =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = multi_workspace
            .read_with(cx, |multi_workspace, _cx| {
                multi_workspace.workspace().clone()
            })
            .unwrap();
        let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);
        let panel = workspace.update_in(cx, |workspace, window, cx| {
            let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
            workspace.add_panel(panel.clone(), window, cx);
            panel
        });

        let _stub_connection =
            crate::test_support::set_stub_agent_connection(StubAgentConnection::new());
        panel.update_in(cx, |panel, window, cx| {
            panel.selected_agent = Agent::Stub;
            panel.activate_draft(true, AgentThreadSource::AgentPanel, window, cx);
        });
        cx.run_until_parked();

        let thread_id = active_thread_id(&panel, cx);
        let thread = panel.read_with(cx, |panel, cx| {
            panel
                .active_agent_thread(cx)
                .expect("draft thread should be active")
        });
        let message_editor = panel.read_with(cx, |panel, cx| {
            panel
                .active_thread_view(cx)
                .expect("draft thread view should be active")
                .read(cx)
                .message_editor
                .clone()
        });

        thread.update(cx, |thread, cx| {
            thread.set_draft_prompt(
                Some(vec![acp::ContentBlock::Text(acp::TextContent::new(
                    "stale prompt",
                ))]),
                cx,
            );
        });
        message_editor.update_in(cx, |editor, window, cx| {
            editor.set_text("fresh prompt", window, cx);
        });
        let blocks = panel.read_with(cx, |panel, cx| {
            panel
                .draft_prompt_blocks_if_in_memory(thread_id, cx)
                .expect("draft should be in memory")
        });
        assert_eq!(blocks.len(), 1);
        assert_eq!(expect_text_block(&blocks[0]), "fresh prompt");

        thread.update(cx, |thread, cx| {
            thread.set_draft_prompt(
                Some(vec![acp::ContentBlock::Text(acp::TextContent::new(
                    "stale prompt after clear",
                ))]),
                cx,
            );
        });
        message_editor.update_in(cx, |editor, window, cx| {
            editor.set_text("", window, cx);
        });
        let blocks = panel.read_with(cx, |panel, cx| {
            panel
                .draft_prompt_blocks_if_in_memory(thread_id, cx)
                .expect("draft should be in memory")
        });
        assert!(
            blocks.is_empty(),
            "cleared editor snapshot should override stale saved draft prompt"
        );
    }

    #[gpui::test]
    async fn test_draft_has_user_content_checks_all_live_copies(cx: &mut TestAppContext) {
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
            .read_with(cx, |multi_workspace, _cx| {
                multi_workspace.workspace().clone()
            })
            .unwrap();
        let workspace_b = multi_workspace
            .update(cx, |multi_workspace, window, cx| {
                multi_workspace.test_add_workspace(project_b.clone(), window, cx)
            })
            .unwrap();

        let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);
        let panel_a = workspace_a.update_in(cx, |workspace, window, cx| {
            let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
            workspace.add_panel(panel.clone(), window, cx);
            panel
        });
        let panel_b = workspace_b.update_in(cx, |workspace, window, cx| {
            let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
            workspace.add_panel(panel.clone(), window, cx);
            panel
        });

        let _stub_connection =
            crate::test_support::set_stub_agent_connection(StubAgentConnection::new());
        panel_a.update_in(cx, |panel, window, cx| {
            panel.selected_agent = Agent::Stub;
            panel.activate_draft(true, AgentThreadSource::AgentPanel, window, cx);
        });
        cx.run_until_parked();
        let thread_id = active_thread_id(&panel_a, cx);

        panel_b.update_in(cx, |panel, window, cx| {
            panel.load_agent_thread(
                Agent::Stub,
                thread_id,
                Some(PathList::new(&[PathBuf::from("/project_b")])),
                None,
                false,
                AgentThreadSource::AgentPanel,
                window,
                cx,
            );
        });
        cx.run_until_parked();

        crate::test_support::type_draft_prompt(&panel_b, "content in second panel", cx);
        let panel_a_blocks = panel_a.read_with(cx, |panel, cx| {
            panel
                .draft_prompt_blocks_if_in_memory(thread_id, cx)
                .expect("draft should be live in first panel")
        });
        assert!(
            panel_a_blocks.is_empty(),
            "first live draft copy should be empty"
        );

        let has_user_content = cx.update(|_, cx| {
            crate::draft_prompt_store::draft_has_user_content(
                thread_id,
                [&workspace_a, &workspace_b],
                cx,
            )
        });
        assert!(
            has_user_content,
            "a later live draft copy with content should keep the draft"
        );
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

    #[gpui::test]
    async fn test_draft_promotion_creates_metadata_and_new_session_on_reload(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);
        cx.update(|cx| {
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
        });

        let fs = FakeFs::new(cx.executor());
        fs.insert_tree("/project", json!({ "file.txt": "" })).await;
        let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;

        let multi_workspace =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));

        let workspace = multi_workspace
            .read_with(cx, |mw, _cx| mw.workspace().clone())
            .unwrap();

        workspace.update(cx, |workspace, _cx| {
            workspace.set_random_database_id();
        });

        let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);

        let panel = workspace.update_in(cx, |workspace, window, cx| {
            let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
            workspace.add_panel(panel.clone(), window, cx);
            panel
        });

        // Register a shared stub connection and use Agent::Stub so the draft
        // (and any reloaded draft) uses it.
        let stub_connection =
            crate::test_support::set_stub_agent_connection(StubAgentConnection::new());
        stub_connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
            acp::ContentChunk::new("Response".into()),
        )]);
        panel.update_in(cx, |panel, window, cx| {
            panel.selected_agent = Agent::Stub;
            panel.activate_draft(true, AgentThreadSource::AgentPanel, window, cx);
        });
        cx.run_until_parked();

        // Verify the thread is considered a draft.
        panel.read_with(cx, |panel, cx| {
            assert!(
                panel.active_thread_is_draft(cx),
                "thread should be a draft before any message is sent"
            );
            assert!(
                panel.draft_thread.is_some(),
                "draft_thread field should be set"
            );
        });
        let draft_session_id = active_session_id(&panel, cx);
        let thread_id = active_thread_id(&panel, cx);

        // A draft thread is persisted with session_id: None.
        cx.update(|_window, cx| {
            let store = ThreadMetadataStore::global(cx).read(cx);
            let entry = store
                .entry(thread_id)
                .expect("draft thread should have a metadata row");
            assert!(
                entry.is_draft(),
                "draft thread metadata should have session_id=None, got {:?}",
                entry.session_id,
            );
        });

        // Type into the message editor; the editor observer pushes the text
        // into `AcpThread.draft_prompt`, which emits `PromptUpdated` and
        // persists the prompt to the kvp store.
        crate::test_support::type_draft_prompt(&panel, "Hello from draft", cx);
        panel.update(cx, |panel, cx| panel.serialize(cx));
        cx.run_until_parked();

        let async_cx = cx.update(|window, cx| window.to_async(cx));
        let reloaded_panel = AgentPanel::load(workspace.downgrade(), async_cx)
            .await
            .expect("panel load with draft should succeed");
        cx.run_until_parked();

        reloaded_panel.read_with(cx, |panel, cx| {
            assert!(
                panel.active_thread_is_draft(cx),
                "reloaded panel should still show the draft as active"
            );
            assert!(
                panel.active_view_is_new_draft(cx),
                "reloaded draft should still occupy the new-draft slot: \
                 what's in the new-draft slot stays there across restarts, \
                 regardless of whether it's also the active view"
            );
            let active_entity = panel.active_conversation_view().map(|v| v.entity_id());
            let draft_entity = panel.draft_thread.as_ref().map(|v| v.entity_id());
            assert!(
                active_entity.is_some() && active_entity == draft_entity,
                "active view and draft slot should share a single ConversationView entity \
                 (active={active_entity:?}, draft={draft_entity:?})"
            );
        });

        // Thread identity is stable across reload — the metadata row we wrote
        // pre-reload maps back to the same ConversationView.
        let reloaded_thread_id = active_thread_id(&reloaded_panel, cx);
        assert_eq!(
            reloaded_thread_id, thread_id,
            "reloaded draft should preserve its ThreadId"
        );

        // ACP session_id is NOT preserved: drafts don't persist a session id,
        // so the reloaded ConversationView opens a fresh ACP session.
        let reloaded_session_id = active_session_id(&reloaded_panel, cx);
        assert_ne!(
            reloaded_session_id, draft_session_id,
            "reloaded draft should have a fresh ACP session ID"
        );

        let restored_text =
            reloaded_panel.read_with(cx, |panel, cx| panel.editor_text(reloaded_thread_id, cx));
        assert_eq!(
            restored_text.as_deref(),
            Some("Hello from draft"),
            "draft prompt text should be restored from the draft-prompt kvp store"
        );

        // Send a message on the reloaded panel — this promotes the draft to a
        // real thread. `ThreadId` stays the same; `session_id` is populated.
        let panel = reloaded_panel;
        let promoted_session_id = reloaded_session_id;
        send_message(&panel, cx);

        panel.read_with(cx, |panel, cx| {
            assert!(
                !panel.active_thread_is_draft(cx),
                "thread should no longer be a draft after sending a message"
            );
            assert!(
                panel.draft_thread.is_none(),
                "draft_thread should be None after promotion"
            );
            assert_eq!(
                panel.active_thread_id(cx),
                Some(thread_id),
                "same ThreadId should remain active after promotion"
            );
        });

        cx.update(|_window, cx| {
            let store = ThreadMetadataStore::global(cx).read(cx);
            let metadata = store
                .entry(thread_id)
                .expect("promoted thread should have metadata");
            assert!(
                !metadata.is_draft(),
                "promoted thread metadata should no longer be a draft"
            );
            assert_eq!(
                metadata.session_id.as_ref(),
                Some(&promoted_session_id),
                "metadata session_id should match the thread's ACP session"
            );
        });

        // Serialize the panel, then reload it again.
        panel.update(cx, |panel, cx| panel.serialize(cx));
        cx.run_until_parked();

        let async_cx = cx.update(|window, cx| window.to_async(cx));
        let loaded_panel = AgentPanel::load(workspace.downgrade(), async_cx)
            .await
            .expect("panel load should succeed");
        cx.run_until_parked();

        // The second load should restore the promoted real thread, keyed by
        // its session_id.
        loaded_panel.read_with(cx, |panel, cx| {
            let active_id = panel.active_thread_id(cx);
            assert_eq!(
                active_id,
                Some(thread_id),
                "loaded panel should restore the promoted thread"
            );
            assert!(
                !panel.active_thread_is_draft(cx),
                "restored thread should not be a draft"
            );
        });
    }

    #[gpui::test]
    async fn test_new_draft_survives_reload_when_real_thread_is_active(cx: &mut TestAppContext) {
        init_test(cx);
        cx.update(|cx| {
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
        });

        let fs = FakeFs::new(cx.executor());
        fs.insert_tree("/project", json!({ "file.txt": "" })).await;
        let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;

        let multi_workspace =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = multi_workspace
            .read_with(cx, |mw, _cx| mw.workspace().clone())
            .unwrap();
        workspace.update(cx, |workspace, _cx| workspace.set_random_database_id());

        let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);
        let panel = workspace.update_in(cx, |workspace, window, cx| {
            let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
            workspace.add_panel(panel.clone(), window, cx);
            panel
        });

        // Register a shared stub connection under `Agent::Stub` so every
        // ConversationView the panel creates in this test (including any
        // post-reload rehydrations) reaches Connected synchronously.
        let stub_connection =
            crate::test_support::set_stub_agent_connection(StubAgentConnection::new());
        stub_connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
            acp::ContentChunk::new("ok".into()),
        )]);

        // 1. Create a real thread by sending a message.
        panel.update_in(cx, |panel, window, cx| {
            panel.selected_agent = Agent::Stub;
            panel.activate_draft(true, AgentThreadSource::AgentPanel, window, cx);
        });
        cx.run_until_parked();
        crate::test_support::send_message(&panel, cx);
        let real_thread_id = crate::test_support::active_thread_id(&panel, cx);
        let real_session_id = crate::test_support::active_session_id(&panel, cx);
        cx.run_until_parked();

        // 2. Open a draft, type into it, then press Cmd-N again to
        //    park it into retained_threads as a *retained* draft.
        panel.update_in(cx, |panel, window, cx| {
            panel.activate_draft(true, AgentThreadSource::AgentPanel, window, cx);
        });
        cx.run_until_parked();
        let retained_draft_id = crate::test_support::active_thread_id(&panel, cx);
        crate::test_support::type_draft_prompt(&panel, "retained draft text", cx);

        panel.update_in(cx, |panel, window, cx| {
            panel.new_thread(&NewThread, window, cx);
        });
        cx.run_until_parked();

        // The pre-existing draft is now in retained_threads (parked),
        // and a fresh empty ephemeral new-draft is active.
        panel.read_with(cx, |panel, cx| {
            assert!(
                panel.retained_threads.contains_key(&retained_draft_id),
                "first draft with content should be parked into retained_threads"
            );
            assert_ne!(
                panel.active_thread_id(cx),
                Some(retained_draft_id),
                "active view should be a fresh ephemeral draft, not the retained one"
            );
        });

        // 3. Type into the new ephemeral draft.
        let draft_thread_id = crate::test_support::active_thread_id(&panel, cx);
        crate::test_support::type_draft_prompt(&panel, "in-flight draft text", cx);

        // Sanity-check: both drafts' text has been persisted to the kvp
        // store via the editor observer / PromptUpdated chain.
        let (ephemeral_kvp, retained_kvp) = cx.update(|_, cx| {
            (
                crate::draft_prompt_store::read(draft_thread_id, cx),
                crate::draft_prompt_store::read(retained_draft_id, cx),
            )
        });
        assert!(
            ephemeral_kvp.is_some(),
            "ephemeral draft's prompt should be in the kvp store"
        );
        assert!(
            retained_kvp.is_some(),
            "retained draft's prompt should be in the kvp store"
        );

        assert_ne!(real_thread_id, draft_thread_id);
        assert_ne!(retained_draft_id, draft_thread_id);
        panel.read_with(cx, |panel, cx| {
            assert!(
                panel.active_view_is_new_draft(cx),
                "draft should currently occupy the new-draft slot"
            );
        });

        // 4. Switch the active view back to the real thread. The ephemeral
        //    draft has content, so it gets parked into `retained_threads`
        //    immediately (the `draft_thread` slot is cleared).
        panel.update_in(cx, |panel, window, cx| {
            panel.load_agent_thread(
                Agent::Stub,
                real_thread_id,
                None,
                None,
                false,
                AgentThreadSource::AgentPanel,
                window,
                cx,
            );
        });
        cx.run_until_parked();

        panel.read_with(cx, |panel, cx| {
            assert_eq!(panel.active_thread_id(cx), Some(real_thread_id));
            assert!(!panel.active_view_is_new_draft(cx));
        });

        // 5. Serialize + reload.
        panel.update(cx, |panel, cx| panel.serialize(cx));
        cx.run_until_parked();
        let async_cx = cx.update(|window, cx| window.to_async(cx));
        let loaded_panel = AgentPanel::load(workspace.downgrade(), async_cx)
            .await
            .expect("panel load should succeed");
        cx.run_until_parked();

        // 6. The real thread is the active view on reload. The draft
        //    was parked when the user navigated away, so the draft_thread
        //    slot is empty.
        loaded_panel.read_with(cx, |panel, cx| {
            assert_eq!(
                panel.active_thread_id(cx),
                Some(real_thread_id),
                "real thread should be the active view after reload"
            );
            assert!(
                !panel.active_thread_is_draft(cx),
                "real thread is not a draft"
            );
            assert!(
                panel.draft_thread.is_none(),
                "draft_thread slot should be empty since the draft was parked on navigate-away"
            );
        });

        // 7. All three threads' metadata rows survive the reload.
        cx.update(|_window, cx| {
            let store = ThreadMetadataStore::global(cx).read(cx);
            let ephemeral_row = store
                .entry(draft_thread_id)
                .expect("ephemeral draft metadata row should survive reload");
            assert!(
                ephemeral_row.is_draft(),
                "ephemeral draft row should still be a draft"
            );
            let retained_row = store
                .entry(retained_draft_id)
                .expect("retained draft metadata row should survive reload");
            assert!(
                retained_row.is_draft(),
                "retained draft row should still be a draft"
            );
            let real_row = store
                .entry(real_thread_id)
                .expect("real thread metadata row should survive reload");
            assert_eq!(real_row.session_id.as_ref(), Some(&real_session_id));
        });

        // 8. Opening the parked draft via load_agent_thread activates
        //    a fresh ConversationView and exposes its kvp-seeded prompt
        //    text in the editor.
        loaded_panel.update_in(cx, |panel, window, cx| {
            panel.load_agent_thread(
                Agent::Stub,
                draft_thread_id,
                None,
                None,
                false,
                AgentThreadSource::AgentPanel,
                window,
                cx,
            );
        });
        cx.run_until_parked();

        let restored_ephemeral_text =
            loaded_panel.read_with(cx, |panel, cx| panel.editor_text(draft_thread_id, cx));
        assert_eq!(
            restored_ephemeral_text.as_deref(),
            Some("in-flight draft text"),
            "ephemeral draft prompt text should be restored from the kvp store"
        );

        // 9. Opening the retained draft via load_agent_thread builds a
        //    fresh ConversationView (since retained_threads was not
        //    carried across the reload) and seeds its editor from the
        //    kvp store.
        loaded_panel.update_in(cx, |panel, window, cx| {
            panel.load_agent_thread(
                Agent::Stub,
                retained_draft_id,
                None,
                None,
                false,
                AgentThreadSource::AgentPanel,
                window,
                cx,
            );
        });
        cx.run_until_parked();

        let restored_retained_text =
            loaded_panel.read_with(cx, |panel, cx| panel.editor_text(retained_draft_id, cx));
        assert_eq!(
            restored_retained_text.as_deref(),
            Some("retained draft text"),
            "retained draft prompt text should be restored from the kvp store"
        );
    }

    #[gpui::test]
    async fn test_reloaded_ephemeral_draft_preserves_original_agent(cx: &mut TestAppContext) {
        init_test(cx);
        cx.update(|cx| {
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
        });

        let fs = FakeFs::new(cx.executor());
        fs.insert_tree("/project", json!({ "file.txt": "" })).await;
        let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;

        let multi_workspace =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = multi_workspace
            .read_with(cx, |mw, _cx| mw.workspace().clone())
            .unwrap();
        workspace.update(cx, |workspace, _cx| workspace.set_random_database_id());

        let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);
        let panel = workspace.update_in(cx, |workspace, window, cx| {
            let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
            workspace.add_panel(panel.clone(), window, cx);
            panel
        });

        let _stub_connection =
            crate::test_support::set_stub_agent_connection(StubAgentConnection::new());
        panel.update_in(cx, |panel, window, cx| {
            panel.selected_agent = Agent::Stub;
            panel.activate_draft(true, AgentThreadSource::AgentPanel, window, cx);
        });
        cx.run_until_parked();

        let draft_thread_id = crate::test_support::active_thread_id(&panel, cx);
        crate::test_support::type_draft_prompt(&panel, "pinned to stub", cx);

        // Diverge `selected_agent` from the draft's bound agent before
        // serialize.
        let other_agent = Agent::Custom {
            id: "other-agent".into(),
        };
        panel.update(cx, |panel, _cx| {
            panel.selected_agent = other_agent.clone();
        });
        panel.update(cx, |panel, cx| panel.serialize(cx));
        cx.run_until_parked();

        // Sanity-check: the draft's metadata row has agent_id="stub",
        // not "other-agent".
        cx.update(|_, cx| {
            let store = ThreadMetadataStore::global(cx).read(cx);
            let row = store
                .entry(draft_thread_id)
                .expect("draft metadata row should exist");
            assert_eq!(
                row.agent_id.as_ref(),
                "stub",
                "draft metadata should retain its original agent binding"
            );
        });

        let async_cx = cx.update(|window, cx| window.to_async(cx));
        let reloaded_panel = AgentPanel::load(workspace.downgrade(), async_cx)
            .await
            .expect("panel load should succeed");
        cx.run_until_parked();

        reloaded_panel.read_with(cx, |panel, cx| {
            let draft_view = panel
                .draft_thread
                .as_ref()
                .expect("draft slot should be repopulated");
            assert_eq!(
                draft_view.read(cx).thread_id,
                draft_thread_id,
                "restored draft should have the same ThreadId"
            );
            assert_eq!(
                draft_view.read(cx).agent_key(),
                &Agent::Stub,
                "restored draft should still be bound to its original Agent::Stub, \
                 not the panel's current `selected_agent`"
            );
        });
    }

    #[gpui::test]
    async fn test_empty_workspace_does_not_create_agent_entries(cx: &mut TestAppContext) {
        init_test(cx);
        cx.update(|cx| {
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
        });

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs.clone(), [], cx).await;
        let multi_workspace =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = multi_workspace
            .read_with(cx, |multi_workspace, _cx| {
                multi_workspace.workspace().clone()
            })
            .unwrap();
        let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);

        let panel = workspace.update_in(cx, |workspace, window, cx| {
            let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
            workspace.add_panel(panel.clone(), window, cx);
            panel
        });

        panel.read_with(cx, |panel, cx| {
            assert_eq!(
                panel
                    .connection_store()
                    .read(cx)
                    .connection_status(&Agent::NativeAgent, cx),
                crate::agent_connection_store::AgentConnectionStatus::Disconnected,
                "empty workspaces should not start the native agent connection"
            );
        });

        panel.update_in(cx, |panel, window, cx| {
            panel.new_thread(&NewThread, window, cx);
            panel.activate_draft(true, AgentThreadSource::AgentPanel, window, cx);
            panel.new_external_agent_thread(
                &NewExternalAgentThread {
                    agent: AgentId::new("external-agent"),
                },
                window,
                cx,
            );
        });
        cx.run_until_parked();

        panel.read_with(cx, |panel, cx| {
            assert!(
                panel.active_conversation_view().is_none(),
                "empty workspaces should not create agent threads"
            );
            assert!(
                panel.draft_thread.is_none(),
                "empty workspaces should not create draft threads"
            );
            assert!(
                panel.terminals(cx).is_empty(),
                "empty workspaces should not create agent panel terminals"
            );
        });

        cx.update(|_, cx| {
            cx.update_flags(true, vec!["agent-panel-terminal".to_string()]);
        });
        panel.update_in(cx, |panel, window, cx| {
            panel.new_terminal(None, AgentThreadSource::AgentPanel, window, cx);
        });
        cx.run_until_parked();

        panel.read_with(cx, |panel, cx| {
            assert!(
                panel.terminals(cx).is_empty(),
                "empty workspaces should not create terminals after the terminal feature is enabled"
            );
            assert_eq!(
                panel
                    .connection_store()
                    .read(cx)
                    .connection_status(&Agent::NativeAgent, cx),
                crate::agent_connection_store::AgentConnectionStatus::Disconnected,
                "empty workspace actions should not start the native agent connection"
            );
        });
    }

    #[gpui::test]
    async fn test_add_selection_to_terminal_thread_pastes_mention(cx: &mut TestAppContext) {
        init_test(cx);
        cx.update(|cx| {
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
        });

        let fs = FakeFs::new(cx.executor());
        fs.insert_tree(
            "/project",
            json!({ "file.rs": "line one\nline two\nline three\n" }),
        )
        .await;
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

        // Make a terminal thread the active conversation. A display-only terminal
        // avoids spawning a real shell; its working directory is supplied directly
        // so the mention resolves relative to it. No agent is started inside it.
        let terminal_id = TerminalId::new();
        panel
            .update_in(&mut cx, |panel, window, cx| {
                panel.insert_display_only_terminal(
                    terminal_id,
                    Some(PathBuf::from("/project")),
                    Some("Terminal".into()),
                    None,
                    None,
                    true,
                    true,
                    false,
                    AgentThreadSource::AgentPanel,
                    window,
                    cx,
                )
            })
            .expect("display-only terminal should be inserted");
        cx.run_until_parked();

        panel.read_with(&cx, |panel, _cx| {
            assert_eq!(panel.active_terminal_id(), Some(terminal_id));
            assert!(panel.active_conversation_view().is_none());
        });

        // Open the file in the center pane so the selection comes from a
        // worktree-backed editor (with a project path).
        workspace
            .update_in(&mut cx, |workspace, window, cx| {
                workspace.open_paths(
                    vec![PathBuf::from("/project/file.rs")],
                    workspace::OpenOptions::default(),
                    None,
                    window,
                    cx,
                )
            })
            .await;
        cx.run_until_parked();

        let editor = workspace.update(&mut cx, |workspace, cx| {
            workspace
                .active_item(cx)
                .and_then(|item| item.act_as::<Editor>(cx))
                .expect("opened file should be an editor")
        });

        cx.focus(&editor);
        cx.run_until_parked();

        let terminal = panel.read_with(&cx, |panel, cx| {
            panel
                .terminals
                .get(&terminal_id)
                .expect("terminal should exist")
                .view
                .read(cx)
                .terminal()
                .clone()
        });
        // Drop any input the terminal may have received during setup.
        terminal.update(&mut cx, |terminal, _| {
            terminal.take_input_log();
        });

        // With only a cursor and nothing highlighted, the action is a no-op and
        // must not paste anything into the terminal.
        workspace.update_in(&mut cx, |_, window, cx| {
            window.dispatch_action(AddSelectionToThread.boxed_clone(), cx);
        });
        cx.run_until_parked();
        let pasted_without_selection =
            terminal.update(&mut cx, |terminal, _| terminal.take_input_log());
        assert!(
            pasted_without_selection.is_empty(),
            "no selection should paste nothing, got {pasted_without_selection:?}"
        );

        // Now highlight a portion of the file: from the start of line 2 into line 3.
        editor.update_in(&mut cx, |editor, window, cx| {
            editor.change_selections(Default::default(), window, cx, |selections| {
                selections.select_ranges([text::Point::new(1, 0)..text::Point::new(2, 4)]);
            });
        });
        cx.run_until_parked();

        workspace.update_in(&mut cx, |_, window, cx| {
            window.dispatch_action(AddSelectionToThread.boxed_clone(), cx);
        });
        cx.run_until_parked();

        let pasted: String = terminal
            .update(&mut cx, |terminal, _| terminal.take_input_log())
            .into_iter()
            .map(|bytes| String::from_utf8(bytes).expect("pasted bytes should be valid UTF-8"))
            .collect();

        // Lines are 1-based and inclusive; the path is presented as
        // `<rel-path>:<start>-<end>`, with a trailing space.
        assert_eq!(pasted, "file.rs:2-3 ");
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

    fn expected_terminal_drop_text(paths: &[PathBuf]) -> String {
        let mut text = String::new();
        for path in paths {
            text.push(' ');
            text.push_str(&format!("{path:?}"));
        }
        text.push(' ');
        text
    }

    #[gpui::test]
    async fn test_terminal_external_image_drop_writes_path(cx: &mut TestAppContext) {
        let (panel, mut cx) = setup_panel(cx).await;
        cx.update(|_, cx| {
            cx.update_flags(true, vec!["agent-panel-terminal".to_string()]);
        });

        let terminal_id = panel
            .update_in(&mut cx, |panel, window, cx| {
                panel.insert_test_terminal("Image Upload", true, window, cx)
            })
            .expect("test terminal should be inserted");
        cx.run_until_parked();

        let terminal = panel.read_with(&cx, |panel, cx| {
            panel
                .terminals
                .get(&terminal_id)
                .expect("terminal should remain in the panel")
                .view
                .read(cx)
                .terminal()
                .clone()
        });
        terminal.update(&mut cx, |terminal, _cx| terminal.take_input_log());

        let image_path = PathBuf::from("/tmp/dropped-image.png");
        panel.update_in(&mut cx, |panel, window, cx| {
            let external_paths = ExternalPaths(vec![image_path.clone()].into());
            panel.paste_external_paths_into_active_terminal(&external_paths, window, cx);
        });

        let mut input_log = terminal.update(&mut cx, |terminal, _cx| terminal.take_input_log());
        assert_eq!(input_log.len(), 1, "expected one write to the terminal");
        let written =
            String::from_utf8(input_log.remove(0)).expect("terminal write should be valid UTF-8");
        assert_eq!(
            written,
            expected_terminal_drop_text(std::slice::from_ref(&image_path))
        );
    }

    #[gpui::test]
    async fn test_terminal_external_paths_drop_handler_writes_image_path(cx: &mut TestAppContext) {
        let (panel, mut cx) = setup_panel(cx).await;
        cx.update(|_, cx| {
            cx.update_flags(true, vec!["agent-panel-terminal".to_string()]);
        });

        let terminal_id = panel
            .update_in(&mut cx, |panel, window, cx| {
                panel.insert_test_terminal("Image Upload", true, window, cx)
            })
            .expect("test terminal should be inserted");
        cx.run_until_parked();

        let terminal = panel.read_with(&cx, |panel, cx| {
            panel
                .terminals
                .get(&terminal_id)
                .expect("terminal should remain in the panel")
                .view
                .read(cx)
                .terminal()
                .clone()
        });
        terminal.update(&mut cx, |terminal, _cx| terminal.take_input_log());

        let image_path = PathBuf::from("/tmp/dropped-image.png");
        panel.update_in(&mut cx, |panel, window, cx| {
            let external_paths = ExternalPaths(vec![image_path.clone()].into());
            panel.handle_external_paths_drop(&external_paths, window, cx);
        });

        let mut input_log = terminal.update(&mut cx, |terminal, _cx| terminal.take_input_log());
        assert_eq!(input_log.len(), 1, "expected one write to the terminal");
        let written =
            String::from_utf8(input_log.remove(0)).expect("terminal write should be valid UTF-8");
        assert_eq!(
            written,
            expected_terminal_drop_text(std::slice::from_ref(&image_path))
        );
    }

    #[gpui::test]
    async fn test_external_file_drop_on_thread_does_not_paste_into_later_terminal(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);
        cx.update(|cx| {
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
            cx.update_flags(true, vec!["agent-panel-terminal".to_string()]);
        });

        let fs = FakeFs::new(cx.executor());
        cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));
        fs.insert_tree("/project", json!({ "file.txt": "content" }))
            .await;
        let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;

        let multi_workspace =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = multi_workspace
            .read_with(cx, |multi_workspace, _cx| {
                multi_workspace.workspace().clone()
            })
            .unwrap();
        let mut cx = VisualTestContext::from_window(multi_workspace.into(), cx);

        let panel = workspace.update_in(&mut cx, |workspace, window, cx| {
            let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
            workspace.add_panel(panel.clone(), window, cx);
            panel
        });
        open_thread_with_connection(&panel, StubAgentConnection::new(), &mut cx);
        let thread_id = active_thread_id(&panel, &cx);

        let file_path = PathBuf::from("/project/file.txt");
        panel.update_in(&mut cx, |panel, window, cx| {
            let external_paths = ExternalPaths(vec![file_path.clone()].into());
            panel.handle_external_paths_drop(&external_paths, window, cx);
        });

        let terminal_id = panel
            .update_in(&mut cx, |panel, window, cx| {
                panel.insert_test_terminal("Drop Target", true, window, cx)
            })
            .expect("test terminal should be inserted");
        let terminal = panel.read_with(&cx, |panel, cx| {
            panel
                .terminals
                .get(&terminal_id)
                .expect("terminal should remain in the panel")
                .view
                .read(cx)
                .terminal()
                .clone()
        });
        terminal.update(&mut cx, |terminal, _cx| terminal.take_input_log());

        cx.run_until_parked();

        let input_log = terminal.update(&mut cx, |terminal, _cx| terminal.take_input_log());
        assert!(
            input_log.is_empty(),
            "thread drop completion should not write to the active terminal"
        );

        let expected_uri = MentionUri::File {
            abs_path: file_path,
        }
        .to_uri()
        .to_string();
        let expected_text = format!("[@file.txt]({expected_uri}) ");
        let actual_text = panel.read_with(&cx, |panel, cx| panel.editor_text(thread_id, cx));
        assert_eq!(actual_text.as_deref(), Some(expected_text.as_str()));
    }

    #[gpui::test]
    async fn test_terminal_entry_kind_controls_new_entry(cx: &mut TestAppContext) {
        let (panel, mut cx) = setup_panel(cx).await;
        panel.read_with(&cx, |panel, cx| {
            assert!(panel.project.read(cx).supports_terminal(cx));
            assert!(!panel.should_create_terminal_for_new_entry(cx));
        });

        let terminal_id = panel
            .update_in(&mut cx, |panel, window, cx| {
                panel.insert_test_terminal("Dev Server", true, window, cx)
            })
            .expect("test terminal should be inserted");
        cx.run_until_parked();

        panel.read_with(&cx, |panel, cx| {
            assert_eq!(panel.active_terminal_id(), Some(terminal_id));
            assert!(panel.has_terminal(terminal_id));
            assert!(panel.should_create_terminal_for_new_entry(cx));
            let terminals = panel.terminals(cx);
            assert_eq!(terminals.len(), 1);
            assert_eq!(terminals[0].title.as_ref(), "Dev Server");
        });

        panel.update_in(&mut cx, |panel, window, cx| {
            panel.activate_new_thread(false, AgentThreadSource::AgentPanel, window, cx);
        });
        cx.run_until_parked();

        panel.read_with(&cx, |panel, cx| {
            assert_eq!(panel.active_terminal_id(), None);
            assert!(panel.has_terminal(terminal_id));
            assert!(!panel.should_create_terminal_for_new_entry(cx));
        });
    }

    #[gpui::test]
    async fn test_skills_menu_entry_shows_manage_skills_shortcut(cx: &mut TestAppContext) {
        init_test(cx);
        cx.update(|cx| {
            let default_key_bindings = settings::KeymapFile::load_asset_allow_partial_failure(
                "keymaps/default-macos.json",
                cx,
            )
            .unwrap();
            cx.bind_keys(default_key_bindings);
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
            .read_with(cx, |multi_workspace, _cx| {
                multi_workspace.workspace().clone()
            })
            .unwrap();
        let mut cx = VisualTestContext::from_window(multi_workspace.into(), cx);

        let panel = workspace.update_in(&mut cx, |workspace, window, cx| {
            let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
            workspace.add_panel(panel.clone(), window, cx);
            panel
        });
        open_thread_with_connection(&panel, StubAgentConnection::new(), &mut cx);
        workspace.update_in(&mut cx, |workspace, window, cx| {
            workspace.focus_panel::<AgentPanel>(window, cx);
        });
        cx.run_until_parked();

        panel.update_in(&mut cx, |panel, window, cx| {
            panel.toggle_options_menu(&ToggleOptionsMenu, window, cx);
        });
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("MENU_ITEM-Skills").is_some(),
            "Skills menu item should be visible"
        );
        assert!(
            cx.debug_bounds("KEY_BINDING-l").is_some(),
            "Skills menu item should show the ManageSkills shortcut"
        );
    }

    #[gpui::test]
    async fn test_terminal_close_event_closes_without_sidebar(cx: &mut TestAppContext) {
        let (panel, mut cx) = setup_panel(cx).await;
        cx.update(|_, cx| {
            TerminalThreadMetadataStore::init_global(cx);
        });

        let terminal_id = panel
            .update_in(&mut cx, |panel, window, cx| {
                panel.insert_test_terminal("Dev Server", true, window, cx)
            })
            .expect("test terminal should be inserted");
        cx.run_until_parked();

        panel.update(&mut cx, |panel, cx| {
            panel.emit_test_terminal_close(terminal_id, cx);
        });
        cx.run_until_parked();

        panel.read_with(&cx, |panel, _cx| {
            assert!(!panel.has_terminal(terminal_id));
        });
        cx.update(|_, cx| {
            assert!(
                TerminalThreadMetadataStore::global(cx)
                    .read(cx)
                    .entry(terminal_id)
                    .is_none(),
                "terminal metadata should be deleted by the fallback close"
            );
        });
    }

    #[gpui::test]
    async fn test_new_thread_dismisses_settings_overlay(cx: &mut TestAppContext) {
        let (panel, mut cx) = setup_panel(cx).await;

        // Put the panel on its ephemeral new-draft view so the base view
        // already contains the draft that `NewThread` would activate.
        panel.update_in(&mut cx, |panel, window, cx| {
            panel.activate_new_thread(true, AgentThreadSource::AgentPanel, window, cx);
        });
        cx.run_until_parked();

        panel.read_with(&cx, |panel, cx| {
            assert!(
                panel.active_view_is_new_draft(cx),
                "precondition: base view should be the ephemeral draft"
            );
            assert!(!panel.is_overlay_open());
        });

        // Simulate the Settings overlay being open on top of the draft.
        // We don't go through `open_configuration` here because it would
        // build provider configuration views, which call into
        // `LanguageModelProvider::configuration_view` — unimplemented for
        // the fake provider used in tests. The bug being exercised lives
        // entirely in the overlay/base-view bookkeeping, so toggling the
        // overlay flag directly is sufficient.
        panel.update_in(&mut cx, |panel, window, cx| {
            panel.set_overlay(OverlayView::Configuration, true, window, cx);
        });
        cx.run_until_parked();

        panel.read_with(&cx, |panel, _cx| {
            assert!(
                panel.is_overlay_open(),
                "precondition: Settings overlay should be open"
            );
        });

        // Dispatching `NewThread` while Settings is open must dismiss the
        // overlay so the user actually sees the new thread. Previously
        // this was a silent no-op: `activate_draft` early-returned without
        // clearing the overlay because the base view already held the
        // draft.
        panel.update_in(&mut cx, |panel, window, cx| {
            panel.new_thread(&NewThread, window, cx);
        });
        cx.run_until_parked();

        panel.read_with(&cx, |panel, cx| {
            assert!(
                !panel.is_overlay_open(),
                "Settings overlay should be dismissed when invoking NewThread"
            );
            assert!(panel.active_view_is_new_draft(cx));
        });
    }

    #[gpui::test]
    async fn test_terminal_title_omits_placeholder_title(cx: &mut TestAppContext) {
        let (panel, mut cx) = setup_panel(cx).await;
        let terminal_id = panel
            .update_in(&mut cx, |panel, window, cx| {
                panel.insert_test_terminal("", true, window, cx)
            })
            .expect("test terminal should be inserted");
        cx.run_until_parked();

        panel.read_with(&cx, |panel, cx| {
            let terminals = panel.terminals(cx);
            assert_eq!(terminals.len(), 1);
            assert_eq!(terminals[0].title.as_ref(), "");
            let terminal = panel
                .terminals
                .get(&terminal_id)
                .expect("terminal should remain in the panel");
            assert_eq!(terminal.title(cx).as_ref(), "");
        });

        let terminal_view = panel.read_with(&cx, |panel, _cx| {
            panel
                .terminals
                .get(&terminal_id)
                .expect("terminal should remain in the panel")
                .view
                .clone()
        });
        let terminal_entity =
            terminal_view.read_with(&cx, |terminal_view, _cx| terminal_view.terminal().clone());
        terminal_entity.update(&mut cx, |_terminal, cx| {
            cx.emit(TerminalEvent::TitleChanged);
        });
        cx.run_until_parked();

        panel.read_with(&cx, |panel, cx| {
            let terminals = panel.terminals(cx);
            assert_eq!(terminals.len(), 1);
            assert_eq!(terminals[0].title.as_ref(), "");
            let terminal = panel
                .terminals
                .get(&terminal_id)
                .expect("terminal should remain in the panel");
            assert_eq!(terminal.title(cx).as_ref(), "");
        });

        terminal_entity.update(&mut cx, |terminal, cx| {
            terminal.breadcrumb_text = "Shell Breadcrumb".to_string();
            cx.emit(TerminalEvent::BreadcrumbsChanged);
        });
        cx.run_until_parked();

        panel.read_with(&cx, |panel, cx| {
            let terminals = panel.terminals(cx);
            assert_eq!(terminals.len(), 1);
            assert_eq!(terminals[0].title.as_ref(), "Shell Breadcrumb");
            let terminal = panel
                .terminals
                .get(&terminal_id)
                .expect("terminal should remain in the panel");
            assert_eq!(terminal.title(cx).as_ref(), "Shell Breadcrumb");
        });
    }

    #[gpui::test]
    async fn test_title_edit_affordance_matches_threads_and_terminals(cx: &mut TestAppContext) {
        let (panel, mut cx) = setup_panel(cx).await;

        panel.update_in(&mut cx, |panel, window, cx| {
            panel.activate_draft(false, AgentThreadSource::AgentPanel, window, cx);
        });
        cx.run_until_parked();

        panel.update_in(&mut cx, |panel, window, cx| {
            assert!(matches!(
                panel.visible_surface(),
                VisibleSurface::AgentThread(_)
            ));
            assert!(panel.should_show_title_edit(window, cx));
        });

        let terminal_id = panel
            .update_in(&mut cx, |panel, window, cx| {
                panel.insert_test_terminal("Dev Server", true, window, cx)
            })
            .expect("test terminal should be inserted");
        cx.run_until_parked();

        panel.update_in(&mut cx, |panel, window, cx| {
            assert!(matches!(
                panel.visible_surface(),
                VisibleSurface::Terminal(_)
            ));
            assert!(panel.should_show_title_edit(window, cx));

            panel.edit_terminal_title(terminal_id, window, cx);
            assert!(!panel.should_show_title_edit(window, cx));
        });
    }

    #[gpui::test]
    async fn test_restored_terminal_uses_metadata_title_until_shell_title_arrives(
        cx: &mut TestAppContext,
    ) {
        let (panel, mut cx) = setup_panel(cx).await;
        let terminal_id = TerminalId::new();
        let now = Utc::now();
        let metadata = TerminalThreadMetadata {
            terminal_id,
            title: "Persisted Shell Title".into(),
            custom_title: None,
            created_at: now,
            worktree_paths: WorktreePaths::from_folder_paths(&PathList::new(&[PathBuf::from(
                "/project",
            )])),
            remote_connection: None,
            working_directory: None,
        };

        panel.update_in(&mut cx, |panel, window, cx| {
            panel
                .restore_test_terminal(metadata, true, AgentThreadSource::Sidebar, None, window, cx)
                .expect("test terminal should be restored");
        });
        cx.run_until_parked();

        let terminal_view = panel.read_with(&cx, |panel, cx| {
            let terminals = panel.terminals(cx);
            assert_eq!(terminals.len(), 1);
            assert_eq!(terminals[0].title.as_ref(), "Persisted Shell Title");
            panel
                .terminals
                .get(&terminal_id)
                .expect("terminal should be restored")
                .view
                .clone()
        });

        let terminal_entity =
            terminal_view.read_with(&cx, |terminal_view, _cx| terminal_view.terminal().clone());
        terminal_entity.update(&mut cx, |terminal, cx| {
            terminal.breadcrumb_text = "Fresh Shell Title".to_string();
            cx.emit(TerminalEvent::BreadcrumbsChanged);
        });
        cx.run_until_parked();

        panel.read_with(&cx, |panel, cx| {
            let terminals = panel.terminals(cx);
            assert_eq!(terminals.len(), 1);
            assert_eq!(terminals[0].title.as_ref(), "Fresh Shell Title");
        });
    }

    #[gpui::test]
    async fn test_restored_terminal_selects_without_focusing(cx: &mut TestAppContext) {
        let (panel, mut cx) = setup_panel(cx).await;
        let terminal_id = TerminalId::new();
        let now = Utc::now();
        let metadata = TerminalThreadMetadata {
            terminal_id,
            title: "Persisted Shell Title".into(),
            custom_title: None,
            created_at: now,
            worktree_paths: WorktreePaths::from_folder_paths(&PathList::new(&[PathBuf::from(
                "/project",
            )])),
            remote_connection: None,
            working_directory: None,
        };

        panel.update_in(&mut cx, |panel, window, cx| {
            panel
                .restore_test_terminal(
                    metadata,
                    false,
                    AgentThreadSource::Sidebar,
                    None,
                    window,
                    cx,
                )
                .expect("test terminal should be restored");
        });
        cx.run_until_parked();

        panel.read_with(&cx, |panel, _cx| {
            assert_eq!(panel.active_terminal_id(), Some(terminal_id));
        });
    }

    #[gpui::test]
    async fn test_terminal_working_directory_uses_active_workspace_while_workspace_is_updating(
        cx: &mut TestAppContext,
    ) {
        let (workspace, panel, mut cx) = setup_workspace_panel(cx).await;
        panel
            .update_in(&mut cx, |panel, window, cx| {
                panel.insert_test_terminal("Dev Server", false, window, cx)
            })
            .expect("test terminal should be inserted");
        cx.run_until_parked();

        panel.read_with(&cx, |panel, cx| {
            assert_eq!(panel.last_created_entry_kind, AgentPanelEntryKind::Terminal);
            assert!(panel.should_create_terminal_for_new_entry(cx));
        });

        workspace.update_in(&mut cx, |workspace, window, cx| {
            let panel = workspace
                .panel::<AgentPanel>(cx)
                .expect("agent panel should be registered in workspace");
            panel.read_with(cx, |panel, cx| {
                panel.terminal_working_directory(Some(workspace), cx);
            });
            workspace.focus_panel::<AgentPanel>(window, cx);
        });

        panel.read_with(&cx, |panel, cx| {
            assert_eq!(panel.last_created_entry_kind, AgentPanelEntryKind::Terminal);
            assert!(panel.should_create_terminal_for_new_entry(cx));
        });
    }

    #[gpui::test]
    async fn test_terminal_title_editor_is_created_only_while_editing(cx: &mut TestAppContext) {
        let (panel, mut cx) = setup_panel(cx).await;
        let terminal_id = panel
            .update_in(&mut cx, |panel, window, cx| {
                panel.insert_test_terminal("Dev Server", true, window, cx)
            })
            .expect("test terminal should be inserted");
        cx.run_until_parked();

        panel.read_with(&cx, |panel, _cx| {
            let terminal = panel
                .terminals
                .get(&terminal_id)
                .expect("terminal should remain in the panel");
            assert!(terminal.title_editor.is_none());
        });

        panel.update(&mut cx, |panel, cx| {
            panel.refresh_terminal_metadata(terminal_id, cx);
        });
        cx.run_until_parked();

        panel.read_with(&cx, |panel, _cx| {
            let terminal = panel
                .terminals
                .get(&terminal_id)
                .expect("terminal should remain in the panel");
            assert!(terminal.title_editor.is_none());
        });

        panel.update_in(&mut cx, |panel, window, cx| {
            panel.edit_terminal_title(terminal_id, window, cx);
        });
        cx.run_until_parked();

        panel.read_with(&cx, |panel, cx| {
            let terminal = panel
                .terminals
                .get(&terminal_id)
                .expect("terminal should remain in the panel");
            let title_editor = terminal
                .title_editor
                .as_ref()
                .expect("terminal title editor should be active while editing");
            assert_eq!(title_editor.read(cx).text(cx), "Dev Server");
        });

        panel.update_in(&mut cx, |panel, window, cx| {
            panel.stop_editing_terminal_title(terminal_id, false, window, cx);
        });
        cx.run_until_parked();

        panel.read_with(&cx, |panel, _cx| {
            let terminal = panel
                .terminals
                .get(&terminal_id)
                .expect("terminal should remain in the panel");
            assert!(terminal.title_editor.is_none());
        });
    }

    #[gpui::test]
    async fn test_terminal_title_editor_does_not_set_custom_title_when_unchanged(
        cx: &mut TestAppContext,
    ) {
        let (panel, mut cx) = setup_panel(cx).await;
        let terminal_id = panel
            .update_in(&mut cx, |panel, window, cx| {
                panel.insert_test_terminal("Initial Custom Title", true, window, cx)
            })
            .expect("test terminal should be inserted");
        cx.run_until_parked();

        let terminal_view = panel.read_with(&cx, |panel, _cx| {
            panel
                .terminals
                .get(&terminal_id)
                .expect("terminal should remain in the panel")
                .view
                .clone()
        });
        terminal_view.update(&mut cx, |terminal_view, cx| {
            terminal_view.set_custom_title(None, cx);
        });
        let terminal_entity =
            terminal_view.read_with(&cx, |terminal_view, _cx| terminal_view.terminal().clone());
        terminal_entity.update(&mut cx, |terminal, cx| {
            terminal.breadcrumb_text = "Shell Breadcrumb".to_string();
            cx.emit(TerminalEvent::BreadcrumbsChanged);
        });
        cx.run_until_parked();

        panel.read_with(&cx, |panel, cx| {
            let terminals = panel.terminals(cx);
            assert_eq!(terminals.len(), 1);
            assert_eq!(terminals[0].title.as_ref(), "Shell Breadcrumb");
        });

        panel.update_in(&mut cx, |panel, window, cx| {
            panel.edit_terminal_title(terminal_id, window, cx);
        });
        cx.run_until_parked();

        let title_editor = panel.read_with(&cx, |panel, cx| {
            let terminal = panel
                .terminals
                .get(&terminal_id)
                .expect("terminal should remain in the panel");
            let title_editor = terminal
                .title_editor
                .as_ref()
                .expect("terminal title editor should be active while editing")
                .clone();
            assert_eq!(title_editor.read(cx).text(cx), "Shell Breadcrumb");
            title_editor
        });

        panel.update_in(&mut cx, |panel, window, cx| {
            panel.handle_terminal_title_editor_event(
                terminal_id,
                &title_editor,
                &editor::EditorEvent::BufferEdited,
                window,
                cx,
            );
        });
        cx.run_until_parked();

        terminal_view.read_with(&cx, |terminal_view, _cx| {
            assert!(terminal_view.custom_title().is_none());
        });

        panel.update_in(&mut cx, |panel, window, cx| {
            panel.stop_editing_terminal_title(terminal_id, false, window, cx);
        });
        terminal_entity.update(&mut cx, |terminal, cx| {
            terminal.breadcrumb_text = "Updated Shell Breadcrumb".to_string();
            cx.emit(TerminalEvent::BreadcrumbsChanged);
        });
        cx.run_until_parked();

        panel.read_with(&cx, |panel, cx| {
            let terminals = panel.terminals(cx);
            assert_eq!(terminals.len(), 1);
            assert_eq!(terminals[0].title.as_ref(), "Updated Shell Breadcrumb");
        });
    }

    #[gpui::test]
    async fn test_terminal_custom_title_recomposes_with_live_spinner(cx: &mut TestAppContext) {
        let (panel, mut cx) = setup_panel(cx).await;
        let terminal_id = panel
            .update_in(&mut cx, |panel, window, cx| {
                panel.insert_test_terminal("Fix bug", true, window, cx)
            })
            .expect("test terminal should be inserted");
        cx.run_until_parked();

        let terminal_entity = panel.read_with(&cx, |panel, _cx| {
            panel
                .terminals
                .get(&terminal_id)
                .expect("terminal should remain in the panel")
                .view
                .clone()
        });
        let terminal_entity =
            terminal_entity.read_with(&cx, |terminal_view, _cx| terminal_view.terminal().clone());

        terminal_entity.update(&mut cx, |terminal, cx| {
            terminal.breadcrumb_text = "⠋ Thinking".to_string();
            cx.emit(TerminalEvent::BreadcrumbsChanged);
        });
        cx.run_until_parked();

        panel.read_with(&cx, |panel, cx| {
            let terminals = panel.terminals(cx);
            assert_eq!(terminals.len(), 1);
            assert_eq!(terminals[0].title.as_ref(), "⠋ Fix bug");
            let metadata = panel
                .terminal_metadata(terminal_id, cx)
                .expect("terminal metadata should be available");
            assert_eq!(metadata.title.as_ref(), "⠋ Thinking");
            assert_eq!(
                metadata.custom_title.as_ref().map(|title| title.as_ref()),
                Some("Fix bug")
            );
            assert_eq!(metadata.display_title().as_ref(), "⠋ Fix bug");
        });

        terminal_entity.update(&mut cx, |terminal, cx| {
            terminal.breadcrumb_text = "⠙ Thinking".to_string();
            cx.emit(TerminalEvent::BreadcrumbsChanged);
        });
        cx.run_until_parked();

        panel.read_with(&cx, |panel, cx| {
            let terminals = panel.terminals(cx);
            assert_eq!(terminals.len(), 1);
            assert_eq!(terminals[0].title.as_ref(), "⠙ Fix bug");
            let metadata = panel
                .terminal_metadata(terminal_id, cx)
                .expect("terminal metadata should be available");
            assert_eq!(metadata.title.as_ref(), "⠙ Thinking");
            assert_eq!(metadata.display_title().as_ref(), "⠙ Fix bug");
        });

        terminal_entity.update(&mut cx, |terminal, cx| {
            terminal.breadcrumb_text = "Thinking".to_string();
            cx.emit(TerminalEvent::BreadcrumbsChanged);
        });
        cx.run_until_parked();

        panel.read_with(&cx, |panel, cx| {
            let terminals = panel.terminals(cx);
            assert_eq!(terminals.len(), 1);
            assert_eq!(terminals[0].title.as_ref(), "Fix bug");
            let metadata = panel
                .terminal_metadata(terminal_id, cx)
                .expect("terminal metadata should be available");
            assert_eq!(metadata.title.as_ref(), "Thinking");
            assert_eq!(metadata.display_title().as_ref(), "Fix bug");
        });
    }

    #[gpui::test]
    async fn test_terminal_title_editor_excludes_spinner_prefix(cx: &mut TestAppContext) {
        let (panel, mut cx) = setup_panel(cx).await;
        let terminal_id = panel
            .update_in(&mut cx, |panel, window, cx| {
                panel.insert_test_terminal("Initial Custom Title", true, window, cx)
            })
            .expect("test terminal should be inserted");
        cx.run_until_parked();

        let terminal_view = panel.read_with(&cx, |panel, _cx| {
            panel
                .terminals
                .get(&terminal_id)
                .expect("terminal should remain in the panel")
                .view
                .clone()
        });
        terminal_view.update(&mut cx, |terminal_view, cx| {
            terminal_view.set_custom_title(None, cx);
        });
        let terminal_entity =
            terminal_view.read_with(&cx, |terminal_view, _cx| terminal_view.terminal().clone());
        terminal_entity.update(&mut cx, |terminal, cx| {
            terminal.breadcrumb_text = "⠋ Thinking".to_string();
            cx.emit(TerminalEvent::BreadcrumbsChanged);
        });
        cx.run_until_parked();

        panel.update_in(&mut cx, |panel, window, cx| {
            panel.edit_terminal_title(terminal_id, window, cx);
        });
        cx.run_until_parked();

        let title_editor = panel.read_with(&cx, |panel, cx| {
            let terminal = panel
                .terminals
                .get(&terminal_id)
                .expect("terminal should remain in the panel");
            let title_editor = terminal
                .title_editor
                .as_ref()
                .expect("terminal title editor should be active while editing")
                .clone();
            assert_eq!(title_editor.read(cx).text(cx), "Thinking");
            title_editor
        });

        title_editor.update_in(&mut cx, |editor, window, cx| {
            editor.set_text("Fix bug", window, cx);
            editor.focus_handle(cx).focus(window, cx);
        });
        panel.update_in(&mut cx, |panel, window, cx| {
            panel.handle_terminal_title_editor_event(
                terminal_id,
                &title_editor,
                &editor::EditorEvent::BufferEdited,
                window,
                cx,
            );
        });
        cx.run_until_parked();

        terminal_view.read_with(&cx, |terminal_view, _cx| {
            assert_eq!(terminal_view.custom_title(), Some("Fix bug"));
        });
        panel.read_with(&cx, |panel, cx| {
            let terminals = panel.terminals(cx);
            assert_eq!(terminals.len(), 1);
            assert_eq!(terminals[0].title.as_ref(), "⠋ Fix bug");
            let metadata = panel
                .terminal_metadata(terminal_id, cx)
                .expect("terminal metadata should be available");
            assert_eq!(metadata.title.as_ref(), "⠋ Thinking");
            assert_eq!(
                metadata.custom_title.as_ref().map(|title| title.as_ref()),
                Some("Fix bug")
            );
        });

        panel.update_in(&mut cx, |panel, window, cx| {
            panel.stop_editing_terminal_title(terminal_id, false, window, cx);
            panel.edit_terminal_title(terminal_id, window, cx);
        });
        cx.run_until_parked();

        panel.read_with(&cx, |panel, cx| {
            let terminal = panel
                .terminals
                .get(&terminal_id)
                .expect("terminal should remain in the panel");
            let title_editor = terminal
                .title_editor
                .as_ref()
                .expect("terminal title editor should be active while editing");
            assert_eq!(title_editor.read(cx).text(cx), "Fix bug");
        });
    }

    #[gpui::test]
    async fn test_terminal_bell_marks_and_activation_clears_notification(cx: &mut TestAppContext) {
        let (panel, mut cx) = setup_panel(cx).await;
        let first_terminal_id = panel
            .update_in(&mut cx, |panel, window, cx| {
                panel.insert_test_terminal("Build", true, window, cx)
            })
            .expect("first test terminal should be inserted");
        let second_terminal_id = panel
            .update_in(&mut cx, |panel, window, cx| {
                panel.insert_test_terminal("Server", true, window, cx)
            })
            .expect("second test terminal should be inserted");
        cx.run_until_parked();

        panel.read_with(&cx, |panel, _cx| {
            assert_eq!(panel.active_terminal_id(), Some(second_terminal_id));
        });

        panel.update(&mut cx, |panel, cx| {
            panel.emit_test_terminal_bell(first_terminal_id, cx);
        });
        cx.run_until_parked();

        panel.read_with(&cx, |panel, cx| {
            let first_terminal = panel
                .terminals(cx)
                .into_iter()
                .find(|terminal| terminal.id == first_terminal_id)
                .expect("first terminal should remain in the panel");
            assert!(first_terminal.has_notification);
        });

        panel.update_in(&mut cx, |panel, window, cx| {
            panel.activate_terminal(first_terminal_id, true, window, cx);
        });
        cx.run_until_parked();

        panel.read_with(&cx, |panel, cx| {
            let first_terminal = panel
                .terminals(cx)
                .into_iter()
                .find(|terminal| terminal.id == first_terminal_id)
                .expect("first terminal should remain in the panel");
            assert!(!first_terminal.has_notification);
        });
    }

    #[gpui::test]
    async fn test_visible_terminal_bell_is_suppressed(cx: &mut TestAppContext) {
        let (panel, mut cx) = setup_visible_panel(cx).await;
        let terminal_id = panel
            .update_in(&mut cx, |panel, window, cx| {
                panel.insert_test_terminal("Claude", true, window, cx)
            })
            .expect("test terminal should be inserted");
        cx.run_until_parked();

        cx.update(|window, cx| {
            assert!(window.is_window_active());
            assert!(panel.read(cx).focus_handle(cx).contains_focused(window, cx));
        });

        panel.update(&mut cx, |panel, cx| {
            panel.emit_test_terminal_bell(terminal_id, cx);
        });
        cx.run_until_parked();

        panel.read_with(&cx, |panel, cx| {
            let terminal = panel
                .terminals(cx)
                .into_iter()
                .find(|terminal| terminal.id == terminal_id)
                .expect("terminal should remain in the panel");
            assert!(!terminal.has_notification);
        });
        assert!(
            cx.windows()
                .iter()
                .all(|window| window.downcast::<AgentNotification>().is_none())
        );
    }

    #[gpui::test]
    async fn test_visible_terminal_bell_is_suppressed_without_focus(cx: &mut TestAppContext) {
        let (panel, mut cx) = setup_visible_panel(cx).await;
        let terminal_id = panel
            .update_in(&mut cx, |panel, window, cx| {
                panel.insert_test_terminal("Claude", true, window, cx)
            })
            .expect("test terminal should be inserted");
        cx.run_until_parked();

        let workspace = cx.update(|window, cx| {
            window
                .root::<MultiWorkspace>()
                .flatten()
                .expect("test window should have a MultiWorkspace root")
                .read(cx)
                .workspace()
                .clone()
        });
        workspace.update_in(&mut cx, |workspace, window, cx| {
            workspace.focus_handle(cx).focus(window, cx);
        });
        cx.update(|window, cx| {
            assert!(window.is_window_active());
            assert!(workspace.read(cx).focus_handle(cx).is_focused(window));
            assert!(!panel.read(cx).focus_handle(cx).contains_focused(window, cx));
        });

        panel.update(&mut cx, |panel, cx| {
            panel.emit_test_terminal_bell(terminal_id, cx);
        });
        cx.run_until_parked();

        panel.read_with(&cx, |panel, cx| {
            let terminal = panel
                .terminals(cx)
                .into_iter()
                .find(|terminal| terminal.id == terminal_id)
                .expect("terminal should remain in the panel");
            assert!(!terminal.has_notification);
        });
        assert!(
            cx.windows()
                .iter()
                .all(|window| window.downcast::<AgentNotification>().is_none())
        );
    }

    #[gpui::test]
    async fn test_terminal_bell_notifies_when_configuration_overlay_covers_terminal(
        cx: &mut TestAppContext,
    ) {
        let (panel, mut cx) = setup_visible_panel(cx).await;
        let terminal_id = panel
            .update_in(&mut cx, |panel, window, cx| {
                panel.insert_test_terminal("Claude", true, window, cx)
            })
            .expect("test terminal should be inserted");
        cx.run_until_parked();

        panel.update_in(&mut cx, |panel, window, cx| {
            panel.set_overlay(OverlayView::Configuration, true, window, cx);
        });
        panel.update(&mut cx, |panel, cx| {
            panel.emit_test_terminal_bell(terminal_id, cx);
        });
        cx.run_until_parked();

        panel.read_with(&cx, |panel, cx| {
            let terminal = panel
                .terminals(cx)
                .into_iter()
                .find(|terminal| terminal.id == terminal_id)
                .expect("terminal should remain in the panel");
            assert!(terminal.has_notification);
        });
        cx.windows()
            .iter()
            .find_map(|window| window.downcast::<AgentNotification>())
            .expect("covered terminal bell should show a notification");
    }

    #[gpui::test]
    async fn test_thread_notification_shows_when_configuration_overlay_covers_thread(
        cx: &mut TestAppContext,
    ) {
        let (panel, mut cx) = setup_visible_panel(cx).await;
        let connection = StubAgentConnection::new();
        connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
            acp::ContentChunk::new("Default response".into()),
        )]);
        open_thread_with_connection(&panel, connection, &mut cx);

        panel.update_in(&mut cx, |panel, window, cx| {
            panel.set_overlay(OverlayView::Configuration, true, window, cx);
        });
        send_message(&panel, &mut cx);

        cx.windows()
            .iter()
            .find_map(|window| window.downcast::<AgentNotification>())
            .expect("covered thread should show a notification");
    }

    #[gpui::test]
    async fn test_terminal_bell_marks_without_popup_when_sidebar_open(cx: &mut TestAppContext) {
        let (panel, mut cx) = setup_visible_panel(cx).await;
        let first_terminal_id = panel
            .update_in(&mut cx, |panel, window, cx| {
                panel.insert_test_terminal("Build", true, window, cx)
            })
            .expect("first test terminal should be inserted");
        let second_terminal_id = panel
            .update_in(&mut cx, |panel, window, cx| {
                panel.insert_test_terminal("Server", true, window, cx)
            })
            .expect("second test terminal should be inserted");
        cx.run_until_parked();

        panel.read_with(&cx, |panel, _cx| {
            assert_eq!(panel.active_terminal_id(), Some(second_terminal_id));
        });
        cx.update(|window, cx| {
            let multi_workspace = window
                .root::<MultiWorkspace>()
                .flatten()
                .expect("test window should have a MultiWorkspace root");
            multi_workspace.update(cx, |multi_workspace, cx| {
                multi_workspace.open_sidebar(cx);
            });
        });
        cx.run_until_parked();

        panel.update(&mut cx, |panel, cx| {
            panel.emit_test_terminal_bell(first_terminal_id, cx);
        });
        cx.run_until_parked();

        panel.read_with(&cx, |panel, cx| {
            let first_terminal = panel
                .terminals(cx)
                .into_iter()
                .find(|terminal| terminal.id == first_terminal_id)
                .expect("first terminal should remain in the panel");
            assert!(first_terminal.has_notification);
        });
        assert!(
            cx.windows()
                .iter()
                .all(|window| window.downcast::<AgentNotification>().is_none())
        );
    }

    #[gpui::test]
    async fn test_terminal_bell_notifies_when_sidebar_history_open(cx: &mut TestAppContext) {
        let (panel, mut cx) = setup_visible_panel_with_sidebar(cx, false).await;
        let first_terminal_id = panel
            .update_in(&mut cx, |panel, window, cx| {
                panel.insert_test_terminal("Build", true, window, cx)
            })
            .expect("first test terminal should be inserted");
        let second_terminal_id = panel
            .update_in(&mut cx, |panel, window, cx| {
                panel.insert_test_terminal("Server", true, window, cx)
            })
            .expect("second test terminal should be inserted");
        cx.run_until_parked();

        panel.read_with(&cx, |panel, _cx| {
            assert_eq!(panel.active_terminal_id(), Some(second_terminal_id));
        });
        cx.update(|window, cx| {
            let multi_workspace = window
                .root::<MultiWorkspace>()
                .flatten()
                .expect("test window should have a MultiWorkspace root");
            multi_workspace.update(cx, |multi_workspace, cx| {
                multi_workspace.open_sidebar(cx);
            });
        });
        cx.run_until_parked();

        panel.update(&mut cx, |panel, cx| {
            panel.emit_test_terminal_bell(first_terminal_id, cx);
        });
        cx.run_until_parked();

        panel.read_with(&cx, |panel, cx| {
            let first_terminal = panel
                .terminals(cx)
                .into_iter()
                .find(|terminal| terminal.id == first_terminal_id)
                .expect("first terminal should remain in the panel");
            assert!(first_terminal.has_notification);
        });
        cx.windows()
            .iter()
            .find_map(|window| window.downcast::<AgentNotification>())
            .expect("terminal bell should notify when the sidebar thread list is hidden");
    }

    #[gpui::test]
    async fn test_terminal_notification_dismissed_when_sidebar_opens(cx: &mut TestAppContext) {
        let (panel, mut cx) = setup_visible_panel(cx).await;
        let first_terminal_id = panel
            .update_in(&mut cx, |panel, window, cx| {
                panel.insert_test_terminal("Build", true, window, cx)
            })
            .expect("first test terminal should be inserted");
        let second_terminal_id = panel
            .update_in(&mut cx, |panel, window, cx| {
                panel.insert_test_terminal("Server", true, window, cx)
            })
            .expect("second test terminal should be inserted");
        cx.run_until_parked();

        panel.read_with(&cx, |panel, _cx| {
            assert_eq!(panel.active_terminal_id(), Some(second_terminal_id));
        });
        panel.update(&mut cx, |panel, cx| {
            panel.emit_test_terminal_bell(first_terminal_id, cx);
        });
        cx.run_until_parked();

        cx.windows()
            .iter()
            .find_map(|window| window.downcast::<AgentNotification>())
            .expect("inactive terminal bell should show a notification");

        cx.update(|window, cx| {
            let multi_workspace = window
                .root::<MultiWorkspace>()
                .flatten()
                .expect("test window should have a MultiWorkspace root");
            multi_workspace.update(cx, |multi_workspace, cx| {
                multi_workspace.open_sidebar(cx);
            });
        });
        cx.run_until_parked();

        panel.read_with(&cx, |panel, cx| {
            let first_terminal = panel
                .terminals(cx)
                .into_iter()
                .find(|terminal| terminal.id == first_terminal_id)
                .expect("first terminal should remain in the panel");
            assert!(first_terminal.has_notification);
        });
        assert!(
            cx.windows()
                .iter()
                .all(|window| window.downcast::<AgentNotification>().is_none())
        );
    }

    #[gpui::test]
    async fn test_focused_terminal_bell_notifies_when_window_inactive(cx: &mut TestAppContext) {
        let (panel, mut cx) = setup_visible_panel(cx).await;
        let terminal_id = panel
            .update_in(&mut cx, |panel, window, cx| {
                panel.insert_test_terminal("Claude", true, window, cx)
            })
            .expect("test terminal should be inserted");
        cx.run_until_parked();

        cx.update(|window, cx| {
            assert!(window.is_window_active());
            assert!(panel.read(cx).focus_handle(cx).contains_focused(window, cx));
        });
        cx.deactivate_window();
        cx.update(|window, _cx| {
            assert!(!window.is_window_active());
        });

        panel.update(&mut cx, |panel, cx| {
            panel.emit_test_terminal_bell(terminal_id, cx);
        });
        cx.run_until_parked();

        panel.read_with(&cx, |panel, cx| {
            let terminal = panel
                .terminals(cx)
                .into_iter()
                .find(|terminal| terminal.id == terminal_id)
                .expect("terminal should remain in the panel");
            assert!(terminal.has_notification);
        });
        cx.windows()
            .iter()
            .find_map(|window| window.downcast::<AgentNotification>())
            .expect("background terminal bell should show a notification");
    }

    #[gpui::test]
    async fn test_active_terminal_notification_clears_when_window_reactivates(
        cx: &mut TestAppContext,
    ) {
        let (panel, mut cx) = setup_visible_panel(cx).await;
        let terminal_id = panel
            .update_in(&mut cx, |panel, window, cx| {
                panel.insert_test_terminal("Claude", true, window, cx)
            })
            .expect("test terminal should be inserted");
        cx.run_until_parked();

        cx.deactivate_window();
        panel.update(&mut cx, |panel, cx| {
            panel.emit_test_terminal_bell(terminal_id, cx);
        });
        cx.run_until_parked();

        panel.read_with(&cx, |panel, cx| {
            let terminal = panel
                .terminals(cx)
                .into_iter()
                .find(|terminal| terminal.id == terminal_id)
                .expect("terminal should remain in the panel");
            assert!(terminal.has_notification);
        });
        cx.windows()
            .iter()
            .find_map(|window| window.downcast::<AgentNotification>())
            .expect("background terminal bell should show a notification");

        cx.update(|window, _cx| {
            window.activate_window();
        });
        cx.run_until_parked();

        panel.read_with(&cx, |panel, cx| {
            let terminal = panel
                .terminals(cx)
                .into_iter()
                .find(|terminal| terminal.id == terminal_id)
                .expect("terminal should remain in the panel");
            assert!(!terminal.has_notification);
        });
        assert!(
            cx.windows()
                .iter()
                .all(|window| window.downcast::<AgentNotification>().is_none())
        );
    }

    #[gpui::test]
    async fn test_terminal_notification_dismissed_when_active_terminal_becomes_visible(
        cx: &mut TestAppContext,
    ) {
        let (panel, mut cx) = setup_panel(cx).await;
        cx.update(|_window, cx| {
            AgentSettings::override_global(
                AgentSettings {
                    notify_when_agent_waiting: NotifyWhenAgentWaiting::PrimaryScreen,
                    ..AgentSettings::get_global(cx).clone()
                },
                cx,
            );
        });
        let terminal_id = panel
            .update_in(&mut cx, |panel, window, cx| {
                panel.insert_test_terminal("Claude", true, window, cx)
            })
            .expect("test terminal should be inserted");
        cx.run_until_parked();

        panel.update(&mut cx, |panel, cx| {
            panel.emit_test_terminal_bell(terminal_id, cx);
        });
        cx.run_until_parked();

        panel.read_with(&cx, |panel, cx| {
            let terminal = panel
                .terminals(cx)
                .into_iter()
                .find(|terminal| terminal.id == terminal_id)
                .expect("terminal should remain in the panel");
            assert!(terminal.has_notification);
        });
        cx.windows()
            .iter()
            .find_map(|window| window.downcast::<AgentNotification>())
            .expect("hidden terminal bell should show a notification");

        let workspace = cx.update(|window, cx| {
            window
                .root::<MultiWorkspace>()
                .flatten()
                .expect("test window should have a MultiWorkspace root")
                .read(cx)
                .workspace()
                .clone()
        });
        workspace.update_in(&mut cx, |workspace, window, cx| {
            workspace.add_panel(panel.clone(), window, cx);
            workspace.focus_panel::<AgentPanel>(window, cx);
        });
        cx.run_until_parked();

        panel.read_with(&cx, |panel, cx| {
            let terminal = panel
                .terminals(cx)
                .into_iter()
                .find(|terminal| terminal.id == terminal_id)
                .expect("terminal should remain in the panel");
            assert!(!terminal.has_notification);
        });
        assert!(
            cx.windows()
                .iter()
                .all(|window| window.downcast::<AgentNotification>().is_none())
        );
    }

    #[gpui::test]
    async fn test_terminal_notification_closed_when_panel_dropped(cx: &mut TestAppContext) {
        let (panel, mut cx) = setup_panel(cx).await;
        cx.update(|_window, cx| {
            AgentSettings::override_global(
                AgentSettings {
                    notify_when_agent_waiting: NotifyWhenAgentWaiting::PrimaryScreen,
                    ..AgentSettings::get_global(cx).clone()
                },
                cx,
            );
        });
        let terminal_id = panel
            .update_in(&mut cx, |panel, window, cx| {
                panel.insert_test_terminal("Claude", true, window, cx)
            })
            .expect("test terminal should be inserted");
        let weak_panel = panel.downgrade();
        cx.run_until_parked();

        panel.update(&mut cx, |panel, cx| {
            panel.emit_test_terminal_bell(terminal_id, cx);
        });
        cx.run_until_parked();

        cx.windows()
            .iter()
            .find_map(|window| window.downcast::<AgentNotification>())
            .expect("hidden terminal bell should show a notification");

        drop(panel);
        cx.update(|_window, _cx| {});
        cx.run_until_parked();

        assert!(
            !weak_panel.is_upgradable(),
            "agent panel should be released after dropping the last handle"
        );
        assert!(
            cx.windows()
                .iter()
                .all(|window| window.downcast::<AgentNotification>().is_none())
        );
    }

    #[gpui::test]
    async fn test_terminal_notification_view_activates_terminal_workspace(cx: &mut TestAppContext) {
        init_test(cx);
        cx.update(|cx| {
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
            cx.update_flags(true, vec!["agent-panel-terminal".to_string()]);
            AgentSettings::override_global(
                AgentSettings {
                    notify_when_agent_waiting: NotifyWhenAgentWaiting::PrimaryScreen,
                    ..AgentSettings::get_global(cx).clone()
                },
                cx,
            );
        });

        let fs = FakeFs::new(cx.executor());
        fs.insert_tree("/project_a", json!({ "file.txt": "" }))
            .await;
        fs.insert_tree("/project_b", json!({ "file.txt": "" }))
            .await;
        let project_a = Project::test(fs.clone(), [Path::new("/project_a")], cx).await;
        let project_b = Project::test(fs, [Path::new("/project_b")], cx).await;

        let multi_workspace =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));
        let workspace_a = multi_workspace
            .read_with(cx, |multi_workspace, _cx| {
                multi_workspace.workspace().clone()
            })
            .unwrap();
        let workspace_b = multi_workspace
            .update(cx, |multi_workspace, window, cx| {
                multi_workspace.test_add_workspace(project_b.clone(), window, cx)
            })
            .unwrap();

        let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);
        let panel_a = workspace_a.update_in(cx, |workspace, window, cx| {
            let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
            workspace.add_panel(panel.clone(), window, cx);
            panel
        });

        let first_terminal_id = panel_a
            .update_in(cx, |panel, window, cx| {
                panel.insert_test_terminal("Build", true, window, cx)
            })
            .expect("first test terminal should be inserted");
        let second_terminal_id = panel_a
            .update_in(cx, |panel, window, cx| {
                panel.insert_test_terminal("Server", true, window, cx)
            })
            .expect("second test terminal should be inserted");
        cx.run_until_parked();

        multi_workspace
            .read_with(cx, |multi_workspace, _cx| {
                assert_eq!(multi_workspace.workspace(), &workspace_b);
            })
            .unwrap();
        panel_a.read_with(cx, |panel, _cx| {
            assert_eq!(panel.active_terminal_id(), Some(second_terminal_id));
        });

        panel_a.update(cx, |panel, cx| {
            panel.emit_test_terminal_bell(first_terminal_id, cx);
        });
        cx.run_until_parked();

        let notification = cx
            .windows()
            .iter()
            .find_map(|window| window.downcast::<AgentNotification>())
            .expect("terminal bell should show a notification");
        notification
            .update(cx, |notification, _window, cx| notification.accept(cx))
            .unwrap();
        assert!(
            cx.windows()
                .iter()
                .all(|window| window.downcast::<AgentNotification>().is_none())
        );
        cx.run_until_parked();

        multi_workspace
            .read_with(cx, |multi_workspace, _cx| {
                assert_eq!(multi_workspace.workspace(), &workspace_a);
            })
            .unwrap();
        panel_a.read_with(cx, |panel, cx| {
            assert_eq!(panel.active_terminal_id(), Some(first_terminal_id));
            let first_terminal = panel
                .terminals(cx)
                .into_iter()
                .find(|terminal| terminal.id == first_terminal_id)
                .expect("first terminal should remain in the panel");
            assert!(!first_terminal.has_notification);
        });
    }

    #[gpui::test]
    async fn test_running_thread_retained_when_navigating_away(cx: &mut TestAppContext) {
        let (panel, mut cx) = setup_panel(cx).await;

        let connection_a = StubAgentConnection::new();
        open_thread_with_connection(&panel, connection_a.clone(), &mut cx);
        send_message(&panel, &mut cx);

        let session_id_a = active_session_id(&panel, &cx);
        let thread_id_a = active_thread_id(&panel, &cx);

        // Send a chunk to keep thread A generating (don't end the turn).
        cx.update(|_, cx| {
            connection_a.send_update(
                session_id_a.clone(),
                acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk::new("chunk".into())),
                cx,
            );
        });
        cx.run_until_parked();

        // Verify thread A is generating.
        panel.read_with(&cx, |panel, cx| {
            let thread = panel.active_agent_thread(cx).unwrap();
            assert_eq!(thread.read(cx).status(), ThreadStatus::Generating);
            assert!(panel.retained_threads.is_empty());
        });

        // Open a new thread B — thread A should be retained in background.
        let connection_b = StubAgentConnection::new();
        open_thread_with_connection(&panel, connection_b, &mut cx);

        panel.read_with(&cx, |panel, _cx| {
            assert_eq!(
                panel.retained_threads.len(),
                1,
                "Running thread A should be retained in retained_threads"
            );
            assert!(
                panel.retained_threads.contains_key(&thread_id_a),
                "Retained thread should be keyed by thread A's thread ID"
            );
        });
    }

    #[gpui::test]
    async fn test_idle_non_loadable_thread_retained_when_navigating_away(cx: &mut TestAppContext) {
        let (panel, mut cx) = setup_panel(cx).await;

        let connection_a = StubAgentConnection::new();
        connection_a.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
            acp::ContentChunk::new("Response".into()),
        )]);
        open_thread_with_connection(&panel, connection_a, &mut cx);
        send_message(&panel, &mut cx);

        let weak_view_a = panel.read_with(&cx, |panel, _cx| {
            panel.active_conversation_view().unwrap().downgrade()
        });
        let thread_id_a = active_thread_id(&panel, &cx);

        // Thread A should be idle (auto-completed via set_next_prompt_updates).
        panel.read_with(&cx, |panel, cx| {
            let thread = panel.active_agent_thread(cx).unwrap();
            assert_eq!(thread.read(cx).status(), ThreadStatus::Idle);
        });

        // Open a new thread B — thread A should be retained because it is not loadable.
        let connection_b = StubAgentConnection::new();
        open_thread_with_connection(&panel, connection_b, &mut cx);

        panel.read_with(&cx, |panel, _cx| {
            assert_eq!(
                panel.retained_threads.len(),
                1,
                "Idle non-loadable thread A should be retained in retained_threads"
            );
            assert!(
                panel.retained_threads.contains_key(&thread_id_a),
                "Retained thread should be keyed by thread A's thread ID"
            );
        });

        assert!(
            weak_view_a.upgrade().is_some(),
            "Idle non-loadable ConnectionView should still be retained"
        );
    }

    #[gpui::test]
    async fn test_background_thread_promoted_via_load(cx: &mut TestAppContext) {
        let (panel, mut cx) = setup_panel(cx).await;

        let connection_a = StubAgentConnection::new();
        open_thread_with_connection(&panel, connection_a.clone(), &mut cx);
        send_message(&panel, &mut cx);

        let session_id_a = active_session_id(&panel, &cx);
        let thread_id_a = active_thread_id(&panel, &cx);

        // Keep thread A generating.
        cx.update(|_, cx| {
            connection_a.send_update(
                session_id_a.clone(),
                acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk::new("chunk".into())),
                cx,
            );
        });
        cx.run_until_parked();

        // Open thread B — thread A goes to background.
        let connection_b = StubAgentConnection::new();
        open_thread_with_connection(&panel, connection_b, &mut cx);
        send_message(&panel, &mut cx);

        let thread_id_b = active_thread_id(&panel, &cx);

        panel.read_with(&cx, |panel, _cx| {
            assert_eq!(panel.retained_threads.len(), 1);
            assert!(panel.retained_threads.contains_key(&thread_id_a));
        });

        // Load thread A back via load_agent_thread — should promote from background.
        panel.update_in(&mut cx, |panel, window, cx| {
            panel.load_agent_thread(
                panel.selected_agent(cx),
                thread_id_a,
                None,
                None,
                true,
                AgentThreadSource::AgentPanel,
                window,
                cx,
            );
        });

        // Thread A should now be the active view, promoted from background.
        let active_session = active_session_id(&panel, &cx);
        assert_eq!(
            active_session, session_id_a,
            "Thread A should be the active thread after promotion"
        );

        panel.read_with(&cx, |panel, _cx| {
            assert!(
                !panel.retained_threads.contains_key(&thread_id_a),
                "Promoted thread A should no longer be in retained_threads"
            );
            assert!(
                panel.retained_threads.contains_key(&thread_id_b),
                "Thread B (idle, non-loadable) should remain retained in retained_threads"
            );
        });
    }

    #[gpui::test]
    async fn test_reopening_visible_thread_keeps_thread_usable(cx: &mut TestAppContext) {
        let (panel, mut cx) = setup_panel(cx).await;
        cx.run_until_parked();

        panel.update(&mut cx, |panel, cx| {
            panel.connection_store.update(cx, |store, cx| {
                store.restart_connection(
                    Agent::NativeAgent,
                    Rc::new(StubAgentServer::new(SessionTrackingConnection::new())),
                    cx,
                );
            });
        });
        cx.run_until_parked();

        panel.update_in(&mut cx, |panel, window, cx| {
            panel.external_thread(
                Some(Agent::NativeAgent),
                None,
                None,
                None,
                None,
                true,
                AgentThreadSource::AgentPanel,
                window,
                cx,
            );
        });
        cx.run_until_parked();
        send_message(&panel, &mut cx);

        let session_id = active_session_id(&panel, &cx);

        panel.update_in(&mut cx, |panel, window, cx| {
            panel.open_thread(session_id.clone(), None, None, window, cx);
        });
        cx.run_until_parked();

        send_message(&panel, &mut cx);

        panel.read_with(&cx, |panel, cx| {
            let active_view = panel
                .active_conversation_view()
                .expect("visible conversation should remain open after reopening");
            let connected = active_view
                .read(cx)
                .as_connected()
                .expect("visible conversation should still be connected in the UI");
            assert!(
                !connected.has_thread_error(cx),
                "reopening an already-visible session should keep the thread usable"
            );
        });
    }

    #[gpui::test]
    async fn test_initial_content_for_thread_summary_uses_own_session_id(cx: &mut TestAppContext) {
        init_test(cx);
        cx.update(|cx| {
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
        });

        let source_session_id = acp::SessionId::new("source-thread-session");
        let source_title: SharedString = "Source Thread Title".into();
        let db_thread = agent::DbThread {
            title: source_title.clone(),
            messages: Vec::new(),
            updated_at: Utc::now(),
            detailed_summary: None,
            initial_project_snapshot: None,
            cumulative_token_usage: Default::default(),
            request_token_usage: HashMap::default(),
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
        };

        let thread_store = cx.update(|cx| ThreadStore::global(cx));
        thread_store
            .update(cx, |store, cx| {
                store.save_thread(
                    source_session_id.clone(),
                    db_thread,
                    PathList::default(),
                    cx,
                )
            })
            .await
            .expect("saving source thread should succeed");
        cx.run_until_parked();

        thread_store.read_with(cx, |store, _cx| {
            let entry = store
                .thread_from_session_id(&source_session_id)
                .expect("saved thread should be listed in the store");
            assert!(
                entry.parent_session_id.is_none(),
                "saved thread is a root thread with no parent session"
            );
        });

        let content = cx
            .update(|cx| {
                AgentPanel::initial_content_for_thread_summary(source_session_id.clone(), cx)
            })
            .expect("initial content should be produced for a root thread");

        match content {
            AgentInitialContent::ThreadSummary { session_id, title } => {
                assert_eq!(
                    session_id, source_session_id,
                    "thread-summary mention should use the source thread's own session id"
                );
                assert_eq!(title, Some(source_title.clone()));
            }
            _ => panic!("expected AgentInitialContent::ThreadSummary"),
        }

        // Unknown session ids should still produce no content.
        let missing = cx.update(|cx| {
            AgentPanel::initial_content_for_thread_summary(
                acp::SessionId::new("does-not-exist"),
                cx,
            )
        });
        assert!(
            missing.is_none(),
            "unknown session ids should not produce initial content"
        );
    }

    #[gpui::test]
    async fn test_cleanup_retained_threads_keeps_five_most_recent_idle_loadable_threads(
        cx: &mut TestAppContext,
    ) {
        let (panel, mut cx) = setup_panel(cx).await;
        let connection = StubAgentConnection::new()
            .with_supports_load_session(true)
            .with_agent_id("loadable-stub".into())
            .with_telemetry_id("loadable-stub".into());
        let mut session_ids = Vec::new();
        let mut thread_ids = Vec::new();

        for _ in 0..7 {
            let (session_id, thread_id) =
                open_generating_thread_with_loadable_connection(&panel, &connection, &mut cx);
            session_ids.push(session_id);
            thread_ids.push(thread_id);
        }

        let base_time = Instant::now();

        for session_id in session_ids.iter().take(6) {
            connection.end_turn(session_id.clone(), acp::StopReason::EndTurn);
        }
        cx.run_until_parked();

        panel.update(&mut cx, |panel, cx| {
            for (index, thread_id) in thread_ids.iter().take(6).enumerate() {
                let conversation_view = panel
                    .retained_threads
                    .get(thread_id)
                    .expect("retained thread should exist")
                    .clone();
                conversation_view.update(cx, |view, cx| {
                    view.set_updated_at(base_time + Duration::from_secs(index as u64), cx);
                });
            }
            panel.cleanup_retained_threads(cx);
        });

        panel.read_with(&cx, |panel, _cx| {
            assert_eq!(
                panel.retained_threads.len(),
                5,
                "cleanup should keep at most five idle loadable retained threads"
            );
            assert!(
                !panel.retained_threads.contains_key(&thread_ids[0]),
                "oldest idle loadable retained thread should be removed"
            );
            for thread_id in &thread_ids[1..6] {
                assert!(
                    panel.retained_threads.contains_key(thread_id),
                    "more recent idle loadable retained threads should be retained"
                );
            }
            assert!(
                !panel.retained_threads.contains_key(&thread_ids[6]),
                "the active thread should not also be stored as a retained thread"
            );
        });
    }

    #[gpui::test]
    async fn test_cleanup_retained_threads_preserves_idle_non_loadable_threads(
        cx: &mut TestAppContext,
    ) {
        let (panel, mut cx) = setup_panel(cx).await;

        let non_loadable_connection = StubAgentConnection::new();
        let (_non_loadable_session_id, non_loadable_thread_id) =
            open_idle_thread_with_non_loadable_connection(
                &panel,
                &non_loadable_connection,
                &mut cx,
            );

        let loadable_connection = StubAgentConnection::new()
            .with_supports_load_session(true)
            .with_agent_id("loadable-stub".into())
            .with_telemetry_id("loadable-stub".into());
        let mut loadable_session_ids = Vec::new();
        let mut loadable_thread_ids = Vec::new();

        for _ in 0..7 {
            let (session_id, thread_id) = open_generating_thread_with_loadable_connection(
                &panel,
                &loadable_connection,
                &mut cx,
            );
            loadable_session_ids.push(session_id);
            loadable_thread_ids.push(thread_id);
        }

        let base_time = Instant::now();

        for session_id in loadable_session_ids.iter().take(6) {
            loadable_connection.end_turn(session_id.clone(), acp::StopReason::EndTurn);
        }
        cx.run_until_parked();

        panel.update(&mut cx, |panel, cx| {
            for (index, thread_id) in loadable_thread_ids.iter().take(6).enumerate() {
                let conversation_view = panel
                    .retained_threads
                    .get(thread_id)
                    .expect("retained thread should exist")
                    .clone();
                conversation_view.update(cx, |view, cx| {
                    view.set_updated_at(base_time + Duration::from_secs(index as u64), cx);
                });
            }
            panel.cleanup_retained_threads(cx);
        });

        panel.read_with(&cx, |panel, _cx| {
            assert_eq!(
                panel.retained_threads.len(),
                6,
                "cleanup should keep the non-loadable idle thread in addition to five loadable ones"
            );
            assert!(
                panel.retained_threads.contains_key(&non_loadable_thread_id),
                "idle non-loadable retained threads should not be cleanup candidates"
            );
            assert!(
                !panel.retained_threads.contains_key(&loadable_thread_ids[0]),
                "oldest idle loadable retained thread should still be removed"
            );
            for thread_id in &loadable_thread_ids[1..6] {
                assert!(
                    panel.retained_threads.contains_key(thread_id),
                    "more recent idle loadable retained threads should be retained"
                );
            }
            assert!(
                !panel.retained_threads.contains_key(&loadable_thread_ids[6]),
                "the active loadable thread should not also be stored as a retained thread"
            );
        });
    }

    #[test]
    fn test_deserialize_agent_variants() {
        // PascalCase (legacy AgentType format, persisted in panel state)
        assert_eq!(
            serde_json::from_str::<Agent>(r#""NativeAgent""#).unwrap(),
            Agent::NativeAgent,
        );
        assert_eq!(
            serde_json::from_str::<Agent>(r#"{"Custom":{"name":"my-agent"}}"#).unwrap(),
            Agent::Custom {
                id: "my-agent".into(),
            },
        );

        // Legacy TextThread variant deserializes to NativeAgent
        assert_eq!(
            serde_json::from_str::<Agent>(r#""TextThread""#).unwrap(),
            Agent::NativeAgent,
        );

        // snake_case (canonical format)
        assert_eq!(
            serde_json::from_str::<Agent>(r#""native_agent""#).unwrap(),
            Agent::NativeAgent,
        );
        assert_eq!(
            serde_json::from_str::<Agent>(r#"{"custom":{"name":"my-agent"}}"#).unwrap(),
            Agent::Custom {
                id: "my-agent".into(),
            },
        );

        // Serialization uses snake_case
        assert_eq!(
            serde_json::to_string(&Agent::NativeAgent).unwrap(),
            r#""native_agent""#,
        );
        assert_eq!(
            serde_json::to_string(&Agent::Custom {
                id: "my-agent".into()
            })
            .unwrap(),
            r#"{"custom":{"name":"my-agent"}}"#,
        );
    }

    #[gpui::test]
    fn test_resolve_worktree_branch_target() {
        let resolved = git_ui::worktree_service::resolve_worktree_branch_target(
            &NewWorktreeBranchTarget::ExistingBranch {
                name: "feature".to_string(),
            },
        );
        assert_eq!(resolved, Some("feature".to_string()));

        let resolved = git_ui::worktree_service::resolve_worktree_branch_target(
            &NewWorktreeBranchTarget::CurrentBranch,
        );
        assert_eq!(resolved, None);

        let resolved = git_ui::worktree_service::resolve_worktree_branch_target(
            &NewWorktreeBranchTarget::RemoteBranch {
                remote_name: "origin".to_string(),
                branch_name: "main".to_string(),
            },
        );
        assert_eq!(resolved, Some("refs/remotes/origin/main".to_string()));
    }

    #[gpui::test]
    async fn test_work_dirs_update_when_worktrees_change(cx: &mut TestAppContext) {
        use crate::thread_metadata_store::ThreadMetadataStore;

        init_test(cx);
        cx.update(|cx| {
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
        });

        // Set up a project with one worktree.
        let fs = FakeFs::new(cx.executor());
        fs.insert_tree("/project_a", json!({ "file.txt": "" }))
            .await;
        let project = Project::test(fs.clone(), [Path::new("/project_a")], cx).await;

        let multi_workspace =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = multi_workspace
            .read_with(cx, |mw, _cx| mw.workspace().clone())
            .unwrap();
        let mut cx = VisualTestContext::from_window(multi_workspace.into(), cx);

        let panel = workspace.update_in(&mut cx, |workspace, window, cx| {
            cx.new(|cx| AgentPanel::new(workspace, window, cx))
        });

        // Open thread A and send a message. With empty next_prompt_updates it
        // stays generating, so opening B will move A to retained_threads.
        let connection_a = StubAgentConnection::new().with_agent_id("agent-a".into());
        open_thread_with_custom_connection(&panel, connection_a.clone(), &mut cx);
        send_message(&panel, &mut cx);
        let session_id_a = active_session_id(&panel, &cx);
        let thread_id_a = active_thread_id(&panel, &cx);

        // Open thread C — thread A (generating) moves to background.
        // Thread C completes immediately (idle), then opening B moves C to background too.
        let connection_c = StubAgentConnection::new().with_agent_id("agent-c".into());
        connection_c.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
            acp::ContentChunk::new("done".into()),
        )]);
        open_thread_with_custom_connection(&panel, connection_c.clone(), &mut cx);
        send_message(&panel, &mut cx);
        let thread_id_c = active_thread_id(&panel, &cx);

        // Open thread B — thread C (idle, non-loadable) is retained in background.
        let connection_b = StubAgentConnection::new().with_agent_id("agent-b".into());
        open_thread_with_custom_connection(&panel, connection_b.clone(), &mut cx);
        send_message(&panel, &mut cx);
        let session_id_b = active_session_id(&panel, &cx);
        let _thread_id_b = active_thread_id(&panel, &cx);

        let metadata_store = cx.update(|_, cx| ThreadMetadataStore::global(cx));

        panel.read_with(&cx, |panel, _cx| {
            assert!(
                panel.retained_threads.contains_key(&thread_id_a),
                "Thread A should be in retained_threads"
            );
            assert!(
                panel.retained_threads.contains_key(&thread_id_c),
                "Thread C should be in retained_threads"
            );
        });

        // Verify initial work_dirs for thread B contain only /project_a.
        let initial_b_paths = panel.read_with(&cx, |panel, cx| {
            let thread = panel.active_agent_thread(cx).unwrap();
            thread.read(cx).work_dirs().cloned().unwrap()
        });
        assert_eq!(
            initial_b_paths.ordered_paths().collect::<Vec<_>>(),
            vec![&PathBuf::from("/project_a")],
            "Thread B should initially have only /project_a"
        );

        // Now add a second worktree to the project.
        fs.insert_tree("/project_b", json!({ "other.txt": "" }))
            .await;
        let (new_tree, _) = project
            .update(&mut cx, |project, cx| {
                project.find_or_create_worktree("/project_b", true, cx)
            })
            .await
            .unwrap();
        cx.read(|cx| new_tree.read(cx).as_local().unwrap().scan_complete())
            .await;
        cx.run_until_parked();

        // Verify thread B's (active) work_dirs now include both worktrees.
        let updated_b_paths = panel.read_with(&cx, |panel, cx| {
            let thread = panel.active_agent_thread(cx).unwrap();
            thread.read(cx).work_dirs().cloned().unwrap()
        });
        let mut b_paths_sorted = updated_b_paths.ordered_paths().cloned().collect::<Vec<_>>();
        b_paths_sorted.sort();
        assert_eq!(
            b_paths_sorted,
            vec![PathBuf::from("/project_a"), PathBuf::from("/project_b")],
            "Thread B work_dirs should include both worktrees after adding /project_b"
        );

        // Verify thread A's (background) work_dirs are also updated.
        let updated_a_paths = panel.read_with(&cx, |panel, cx| {
            let bg_view = panel.retained_threads.get(&thread_id_a).unwrap();
            let root_thread = bg_view.read(cx).root_thread_view().unwrap();
            root_thread
                .read(cx)
                .thread
                .read(cx)
                .work_dirs()
                .cloned()
                .unwrap()
        });
        let mut a_paths_sorted = updated_a_paths.ordered_paths().cloned().collect::<Vec<_>>();
        a_paths_sorted.sort();
        assert_eq!(
            a_paths_sorted,
            vec![PathBuf::from("/project_a"), PathBuf::from("/project_b")],
            "Thread A work_dirs should include both worktrees after adding /project_b"
        );

        // Verify thread idle C was also updated.
        let updated_c_paths = panel.read_with(&cx, |panel, cx| {
            let bg_view = panel.retained_threads.get(&thread_id_c).unwrap();
            let root_thread = bg_view.read(cx).root_thread_view().unwrap();
            root_thread
                .read(cx)
                .thread
                .read(cx)
                .work_dirs()
                .cloned()
                .unwrap()
        });
        let mut c_paths_sorted = updated_c_paths.ordered_paths().cloned().collect::<Vec<_>>();
        c_paths_sorted.sort();
        assert_eq!(
            c_paths_sorted,
            vec![PathBuf::from("/project_a"), PathBuf::from("/project_b")],
            "Thread C (idle background) work_dirs should include both worktrees after adding /project_b"
        );

        // Verify the metadata store reflects the new paths for running threads only.
        cx.run_until_parked();
        for (label, session_id) in [("thread B", &session_id_b), ("thread A", &session_id_a)] {
            let metadata_paths = metadata_store.read_with(&cx, |store, _cx| {
                let metadata = store
                    .entry_by_session(session_id)
                    .unwrap_or_else(|| panic!("{label} thread metadata should exist"));
                metadata.folder_paths().clone()
            });
            let mut sorted = metadata_paths.ordered_paths().cloned().collect::<Vec<_>>();
            sorted.sort();
            assert_eq!(
                sorted,
                vec![PathBuf::from("/project_a"), PathBuf::from("/project_b")],
                "{label} thread metadata folder_paths should include both worktrees"
            );
        }

        // Now remove a worktree and verify work_dirs shrink.
        let worktree_b_id = new_tree.read_with(&cx, |tree, _| tree.id());
        project.update(&mut cx, |project, cx| {
            project.remove_worktree(worktree_b_id, cx);
        });
        cx.run_until_parked();

        let after_remove_b = panel.read_with(&cx, |panel, cx| {
            let thread = panel.active_agent_thread(cx).unwrap();
            thread.read(cx).work_dirs().cloned().unwrap()
        });
        assert_eq!(
            after_remove_b.ordered_paths().collect::<Vec<_>>(),
            vec![&PathBuf::from("/project_a")],
            "Thread B work_dirs should revert to only /project_a after removing /project_b"
        );

        let after_remove_a = panel.read_with(&cx, |panel, cx| {
            let bg_view = panel.retained_threads.get(&thread_id_a).unwrap();
            let root_thread = bg_view.read(cx).root_thread_view().unwrap();
            root_thread
                .read(cx)
                .thread
                .read(cx)
                .work_dirs()
                .cloned()
                .unwrap()
        });
        assert_eq!(
            after_remove_a.ordered_paths().collect::<Vec<_>>(),
            vec![&PathBuf::from("/project_a")],
            "Thread A work_dirs should revert to only /project_a after removing /project_b"
        );
    }

    #[gpui::test]
    async fn test_new_workspace_inherits_global_last_used_agent(cx: &mut TestAppContext) {
        init_test(cx);
        cx.update(|cx| {
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
            // Use an isolated DB so parallel tests can't overwrite our global key.
            cx.set_global(db::AppDatabase::test_new());
        });

        let custom_agent = Agent::Custom {
            id: "my-preferred-agent".into(),
        };

        // Write a known agent to the global KVP to simulate a user who has
        // previously used this agent in another workspace.
        let kvp = cx.update(|cx| KeyValueStore::global(cx));
        write_global_last_used_agent(kvp, custom_agent.clone()).await;

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs.clone(), [], cx).await;

        let multi_workspace =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));

        let workspace = multi_workspace
            .read_with(cx, |multi_workspace, _cx| {
                multi_workspace.workspace().clone()
            })
            .unwrap();

        workspace.update(cx, |workspace, _cx| {
            workspace.set_random_database_id();
        });

        let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);

        // Load the panel via `load()`, which reads the global fallback
        // asynchronously when no per-workspace state exists.
        let async_cx = cx.update(|window, cx| window.to_async(cx));
        let panel = AgentPanel::load(workspace.downgrade(), async_cx)
            .await
            .expect("panel load should succeed");
        cx.run_until_parked();

        panel.read_with(cx, |panel, _cx| {
            assert_eq!(
                panel.selected_agent, custom_agent,
                "new workspace should inherit the global last-used agent"
            );
        });
    }

    #[gpui::test]
    async fn test_workspaces_maintain_independent_agent_selection(cx: &mut TestAppContext) {
        init_test(cx);
        cx.update(|cx| {
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
        });

        let fs = FakeFs::new(cx.executor());
        let project_a = Project::test(fs.clone(), [], cx).await;
        let project_b = Project::test(fs, [], cx).await;

        let multi_workspace =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));

        let workspace_a = multi_workspace
            .read_with(cx, |multi_workspace, _cx| {
                multi_workspace.workspace().clone()
            })
            .unwrap();

        let workspace_b = multi_workspace
            .update(cx, |multi_workspace, window, cx| {
                multi_workspace.test_add_workspace(project_b.clone(), window, cx)
            })
            .unwrap();

        workspace_a.update(cx, |workspace, _cx| {
            workspace.set_random_database_id();
        });
        workspace_b.update(cx, |workspace, _cx| {
            workspace.set_random_database_id();
        });

        let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);

        let agent_a = Agent::Custom {
            id: "agent-alpha".into(),
        };
        let agent_b = Agent::Custom {
            id: "agent-beta".into(),
        };

        // Set up workspace A with agent_a
        let panel_a = workspace_a.update_in(cx, |workspace, window, cx| {
            cx.new(|cx| AgentPanel::new(workspace, window, cx))
        });
        panel_a.update(cx, |panel, _cx| {
            panel.selected_agent = agent_a.clone();
        });

        // Set up workspace B with agent_b
        let panel_b = workspace_b.update_in(cx, |workspace, window, cx| {
            cx.new(|cx| AgentPanel::new(workspace, window, cx))
        });
        panel_b.update(cx, |panel, _cx| {
            panel.selected_agent = agent_b.clone();
        });

        // Serialize both panels
        panel_a.update(cx, |panel, cx| panel.serialize(cx));
        panel_b.update(cx, |panel, cx| panel.serialize(cx));
        cx.run_until_parked();

        // Load fresh panels from serialized state and verify independence
        let async_cx = cx.update(|window, cx| window.to_async(cx));
        let loaded_a = AgentPanel::load(workspace_a.downgrade(), async_cx)
            .await
            .expect("panel A load should succeed");
        cx.run_until_parked();

        let async_cx = cx.update(|window, cx| window.to_async(cx));
        let loaded_b = AgentPanel::load(workspace_b.downgrade(), async_cx)
            .await
            .expect("panel B load should succeed");
        cx.run_until_parked();

        loaded_a.read_with(cx, |panel, _cx| {
            assert_eq!(
                panel.selected_agent, agent_a,
                "workspace A should restore agent-alpha, not agent-beta"
            );
        });

        loaded_b.read_with(cx, |panel, _cx| {
            assert_eq!(
                panel.selected_agent, agent_b,
                "workspace B should restore agent-beta, not agent-alpha"
            );
        });
    }

    #[gpui::test]
    async fn test_new_thread_uses_workspace_selected_agent(cx: &mut TestAppContext) {
        init_test(cx);
        cx.update(|cx| {
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
        });

        let fs = FakeFs::new(cx.executor());
        fs.insert_tree("/project", json!({ "file.txt": "" })).await;
        let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;

        let multi_workspace =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));

        let workspace = multi_workspace
            .read_with(cx, |multi_workspace, _cx| {
                multi_workspace.workspace().clone()
            })
            .unwrap();

        workspace.update(cx, |workspace, _cx| {
            workspace.set_random_database_id();
        });

        let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);

        let custom_agent = Agent::Custom {
            id: "my-custom-agent".into(),
        };

        let panel = workspace.update_in(cx, |workspace, window, cx| {
            let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
            workspace.add_panel(panel.clone(), window, cx);
            panel
        });

        // Set selected_agent to a custom agent
        panel.update(cx, |panel, _cx| {
            panel.selected_agent = custom_agent.clone();
        });

        // Call new_thread, which internally calls external_thread(None, ...)
        // This resolves the agent from self.selected_agent
        panel.update_in(cx, |panel, window, cx| {
            panel.new_thread(&NewThread, window, cx);
        });

        panel.read_with(cx, |panel, _cx| {
            assert_eq!(
                panel.selected_agent, custom_agent,
                "selected_agent should remain the custom agent after new_thread"
            );
            assert!(
                panel.active_conversation_view().is_some(),
                "a thread should have been created"
            );
        });
    }

    #[gpui::test]
    async fn test_draft_replaced_when_selected_agent_changes(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        cx.update(|cx| {
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
            <dyn fs::Fs>::set_global(fs.clone(), cx);
        });

        fs.insert_tree("/project", json!({ "file.txt": "" })).await;
        let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;

        let multi_workspace =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));

        let workspace = multi_workspace
            .read_with(cx, |multi_workspace, _cx| {
                multi_workspace.workspace().clone()
            })
            .unwrap();

        workspace.update(cx, |workspace, _cx| {
            workspace.set_random_database_id();
        });

        let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);

        let panel = workspace.update_in(cx, |workspace, window, cx| {
            let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
            workspace.add_panel(panel.clone(), window, cx);
            panel
        });

        // Create a draft with the default NativeAgent.
        panel.update_in(cx, |panel, window, cx| {
            panel.activate_draft(true, AgentThreadSource::AgentPanel, window, cx);
        });

        let first_draft_id = panel.read_with(cx, |panel, cx| {
            assert!(panel.draft_thread.is_some());
            assert_eq!(panel.selected_agent, Agent::NativeAgent);
            let draft = panel.draft_thread.as_ref().unwrap();
            assert_eq!(*draft.read(cx).agent_key(), Agent::NativeAgent);
            draft.entity_id()
        });

        // Switch selected_agent to a custom agent, then activate_draft again.
        // The stale NativeAgent draft should be replaced.
        let custom_agent = Agent::Custom {
            id: "my-custom-agent".into(),
        };
        panel.update_in(cx, |panel, window, cx| {
            panel.selected_agent = custom_agent.clone();
            panel.activate_draft(true, AgentThreadSource::AgentPanel, window, cx);
        });

        panel.read_with(cx, |panel, cx| {
            let draft = panel.draft_thread.as_ref().expect("draft should exist");
            assert_ne!(
                draft.entity_id(),
                first_draft_id,
                "a new draft should have been created"
            );
            assert_eq!(
                *draft.read(cx).agent_key(),
                custom_agent,
                "the new draft should use the custom agent"
            );
        });

        // Calling activate_draft again with the same agent should return the
        // cached draft (no replacement).
        let second_draft_id = panel.read_with(cx, |panel, _cx| {
            panel.draft_thread.as_ref().unwrap().entity_id()
        });

        panel.update_in(cx, |panel, window, cx| {
            panel.activate_draft(true, AgentThreadSource::AgentPanel, window, cx);
        });

        panel.read_with(cx, |panel, _cx| {
            assert_eq!(
                panel.draft_thread.as_ref().unwrap().entity_id(),
                second_draft_id,
                "draft should be reused when the agent has not changed"
            );
        });
    }

    #[gpui::test]
    async fn test_activate_draft_preserves_typed_content(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        cx.update(|cx| {
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
            <dyn fs::Fs>::set_global(fs.clone(), cx);
        });

        fs.insert_tree("/project", json!({ "file.txt": "" })).await;
        let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;

        let multi_workspace =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));

        let workspace = multi_workspace
            .read_with(cx, |multi_workspace, _cx| {
                multi_workspace.workspace().clone()
            })
            .unwrap();

        workspace.update(cx, |workspace, _cx| {
            workspace.set_random_database_id();
        });

        let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);

        let panel = workspace.update_in(cx, |workspace, window, cx| {
            let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
            workspace.add_panel(panel.clone(), window, cx);
            panel
        });

        // Create a draft using the Stub agent, which connects synchronously.
        panel.update_in(cx, |panel, window, cx| {
            panel.selected_agent = Agent::Stub;
            panel.activate_draft(true, AgentThreadSource::AgentPanel, window, cx);
        });
        cx.run_until_parked();

        let initial_draft_id = panel.read_with(cx, |panel, _cx| {
            panel.draft_thread.as_ref().unwrap().entity_id()
        });
        let initial_thread_id =
            panel.read_with(cx, |panel, cx| panel.active_thread_id(cx).unwrap());

        // Type some text into the draft editor.
        let thread_view = panel.read_with(cx, |panel, cx| panel.active_thread_view(cx).unwrap());
        let message_editor = thread_view.read_with(cx, |view, _cx| view.message_editor.clone());
        message_editor.update_in(cx, |editor, window, cx| {
            editor.set_text("Don't lose me!", window, cx);
        });

        // Press cmd-n on a typed draft — the draft is parked into
        // `retained_threads` so the user can return to it from the
        // sidebar, and a fresh, *empty* ephemeral draft becomes active.
        // The parked draft retains the prompt; the new one is a blank
        // slate.
        cx.dispatch_action(NewThread);
        cx.run_until_parked();

        panel.read_with(cx, |panel, _cx| {
            assert!(
                panel.retained_threads.contains_key(&initial_thread_id),
                "typed draft should have been parked into retained_threads"
            );
            let active_draft_id = panel.draft_thread.as_ref().unwrap().entity_id();
            assert_ne!(
                active_draft_id, initial_draft_id,
                "cmd-n should produce a fresh ephemeral draft"
            );
        });

        // The parked draft still holds the typed prompt.
        let parked_text = panel.read_with(cx, |panel, cx| panel.editor_text(initial_thread_id, cx));
        assert_eq!(
            parked_text.as_deref(),
            Some("Don't lose me!"),
            "parked draft should retain the typed prompt"
        );

        // The new active draft starts empty — no carry-over.
        let active_thread_id = panel.read_with(cx, |panel, cx| panel.active_thread_id(cx).unwrap());
        let active_text = panel.read_with(cx, |panel, cx| panel.editor_text(active_thread_id, cx));
        assert_eq!(
            active_text, None,
            "fresh ephemeral draft should start empty, not carry the parked draft's prompt"
        );
    }

    /// When the user is viewing a *parked* draft (selected from the
    /// sidebar) and presses `+`, the panel should just focus the
    /// ephemeral new-draft slot — not park it and create yet another
    /// empty draft. `+` is "go to my new-thread slot", not "reset state".
    #[gpui::test]
    async fn test_plus_with_parked_draft_active_focuses_ephemeral(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        cx.update(|cx| {
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
            <dyn fs::Fs>::set_global(fs.clone(), cx);
        });

        fs.insert_tree("/project", json!({ "file.txt": "" })).await;
        let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;
        let multi_workspace =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = multi_workspace
            .read_with(cx, |multi_workspace, _cx| {
                multi_workspace.workspace().clone()
            })
            .unwrap();
        workspace.update(cx, |workspace, _cx| workspace.set_random_database_id());
        let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);
        let panel = workspace.update_in(cx, |workspace, window, cx| {
            let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
            workspace.add_panel(panel.clone(), window, cx);
            panel
        });

        // Open an initial draft, type into it, then press `+` to park it
        // and create a fresh ephemeral. The fresh ephemeral is what we'll
        // expect to refocus later.
        panel.update_in(cx, |panel, window, cx| {
            panel.selected_agent = Agent::Stub;
            panel.activate_draft(true, AgentThreadSource::AgentPanel, window, cx);
        });
        cx.run_until_parked();
        let parked_thread_id = crate::test_support::active_thread_id(&panel, cx);
        crate::test_support::type_draft_prompt(&panel, "parked draft prompt", cx);
        panel.update_in(cx, |panel, window, cx| {
            panel.new_thread(&NewThread, window, cx);
        });
        cx.run_until_parked();

        let ephemeral_thread_id = crate::test_support::active_thread_id(&panel, cx);
        let ephemeral_entity_id = panel.read_with(cx, |panel, _cx| {
            panel.draft_thread.as_ref().unwrap().entity_id()
        });
        assert_ne!(
            ephemeral_thread_id, parked_thread_id,
            "sanity: parking should have produced a fresh ephemeral draft"
        );

        // Activate the parked draft (simulates clicking it in the sidebar).
        panel.update_in(cx, |panel, window, cx| {
            panel.load_agent_thread(
                Agent::Stub,
                parked_thread_id,
                None,
                None,
                true,
                AgentThreadSource::Sidebar,
                window,
                cx,
            );
        });
        cx.run_until_parked();
        assert_eq!(
            crate::test_support::active_thread_id(&panel, cx),
            parked_thread_id,
            "sanity: parked draft should be the active view after load_agent_thread"
        );
        // The parked draft has content, so it was NOT reclaimed as
        // ephemeral. The previous ephemeral draft should still be in
        // the draft_thread slot.
        panel.read_with(cx, |panel, _cx| {
            assert_eq!(
                panel.draft_thread.as_ref().unwrap().entity_id(),
                ephemeral_entity_id,
                "ephemeral draft slot should still hold the fresh draft"
            );
        });

        // Now press `+`. The ephemeral draft should become the active
        // view since it matches the selected agent.
        panel.update_in(cx, |panel, window, cx| {
            panel.new_thread(&NewThread, window, cx);
        });
        cx.run_until_parked();

        panel.read_with(cx, |panel, cx| {
            assert_eq!(
                panel.active_thread_id(cx),
                Some(ephemeral_thread_id),
                "`+` should have switched back to the existing ephemeral draft"
            );
            assert_eq!(
                panel.draft_thread.as_ref().unwrap().entity_id(),
                ephemeral_entity_id,
                "`+` should not have replaced the ephemeral draft"
            );
            assert!(
                panel.retained_threads.contains_key(&parked_thread_id),
                "parked draft should remain in `retained_threads`"
            );
        });
    }

    /// When viewing a parked draft (agent A) and selecting a different
    /// agent (B) from the dropdown menu, the panel should create a fresh
    /// draft for agent B — not reuse the existing ephemeral draft that
    /// was bound to agent A.
    #[gpui::test]
    async fn test_new_external_agent_replaces_mismatched_ephemeral_draft(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        cx.update(|cx| {
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
            <dyn fs::Fs>::set_global(fs.clone(), cx);
        });

        fs.insert_tree("/project", json!({ "file.txt": "" })).await;
        let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;
        let multi_workspace =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = multi_workspace
            .read_with(cx, |multi_workspace, _cx| {
                multi_workspace.workspace().clone()
            })
            .unwrap();
        workspace.update(cx, |workspace, _cx| workspace.set_random_database_id());
        let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);
        let panel = workspace.update_in(cx, |workspace, window, cx| {
            let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
            workspace.add_panel(panel.clone(), window, cx);
            panel
        });

        // Create a draft with Stub agent, type into it, then press `+`
        // to park it — this also creates a fresh ephemeral draft (Stub).
        panel.update_in(cx, |panel, window, cx| {
            panel.selected_agent = Agent::Stub;
            panel.activate_draft(true, AgentThreadSource::AgentPanel, window, cx);
        });
        cx.run_until_parked();
        let parked_thread_id = crate::test_support::active_thread_id(&panel, cx);
        crate::test_support::type_draft_prompt(&panel, "parked prompt", cx);
        panel.update_in(cx, |panel, window, cx| {
            panel.new_thread(&NewThread, window, cx);
        });
        cx.run_until_parked();

        let ephemeral_thread_id = crate::test_support::active_thread_id(&panel, cx);
        assert_ne!(ephemeral_thread_id, parked_thread_id);
        panel.read_with(cx, |panel, cx| {
            assert_eq!(
                panel.draft_thread.as_ref().unwrap().read(cx).agent_key(),
                &Agent::Stub,
                "ephemeral draft should be Stub agent"
            );
        });

        // Navigate back to the parked draft (simulates sidebar click).
        panel.update_in(cx, |panel, window, cx| {
            panel.load_agent_thread(
                Agent::Stub,
                parked_thread_id,
                None,
                None,
                true,
                AgentThreadSource::Sidebar,
                window,
                cx,
            );
        });
        cx.run_until_parked();
        assert_eq!(
            crate::test_support::active_thread_id(&panel, cx),
            parked_thread_id,
        );

        // Now switch to NativeAgent (simulates selecting a different
        // agent from the toolbar dropdown). This should NOT reuse the
        // Stub ephemeral draft — it should replace it with one bound to
        // NativeAgent.
        panel.update_in(cx, |panel, window, cx| {
            panel.selected_agent = Agent::NativeAgent;
            panel.activate_new_thread(true, AgentThreadSource::AgentPanel, window, cx);
        });
        cx.run_until_parked();

        panel.read_with(cx, |panel, cx| {
            let draft = panel.draft_thread.as_ref().expect("draft should exist");
            assert_eq!(
                draft.read(cx).agent_key(),
                &Agent::NativeAgent,
                "ephemeral draft should be bound to NativeAgent, not Stub"
            );
            let active_id = panel.active_thread_id(cx).unwrap();
            assert_ne!(
                active_id, ephemeral_thread_id,
                "old Stub ephemeral draft should have been replaced"
            );
            assert!(
                panel.retained_threads.contains_key(&parked_thread_id),
                "parked draft should still be in retained_threads"
            );
        });
    }

    #[gpui::test]
    async fn test_typed_draft_is_parked_when_switching_agents(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        cx.update(|cx| {
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
            <dyn fs::Fs>::set_global(fs.clone(), cx);
        });

        fs.insert_tree("/project", json!({ "file.txt": "" })).await;
        let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;

        let multi_workspace =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));

        let workspace = multi_workspace
            .read_with(cx, |multi_workspace, _cx| {
                multi_workspace.workspace().clone()
            })
            .unwrap();

        workspace.update(cx, |workspace, _cx| {
            workspace.set_random_database_id();
        });

        let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);

        let panel = workspace.update_in(cx, |workspace, window, cx| {
            let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
            workspace.add_panel(panel.clone(), window, cx);
            panel
        });

        // Create a draft with a custom stub server that connects synchronously.
        panel.update_in(cx, |panel, window, cx| {
            panel.open_draft_with_server(
                Rc::new(StubAgentServer::new(StubAgentConnection::new())),
                window,
                cx,
            );
        });
        cx.run_until_parked();

        let initial_draft_id = panel.read_with(cx, |panel, _cx| {
            panel.draft_thread.as_ref().unwrap().entity_id()
        });
        let initial_thread_id =
            panel.read_with(cx, |panel, cx| panel.active_thread_id(cx).unwrap());

        // Type text into the first draft's editor.
        let thread_view = panel.read_with(cx, |panel, cx| panel.active_thread_view(cx).unwrap());
        let message_editor = thread_view.read_with(cx, |view, _cx| view.message_editor.clone());
        message_editor.update_in(cx, |editor, window, cx| {
            editor.set_text("saved prompt", window, cx);
        });

        // Switch to a different agent. The typed draft should be parked
        // into `retained_threads` (keeping the user's prompt accessible
        // from the sidebar) and a fresh empty draft on the new agent
        // should become active.
        cx.dispatch_action(NewExternalAgentThread {
            agent: Agent::Stub.id(),
        });
        cx.run_until_parked();

        // A new draft should have been created for the Stub agent.
        panel.read_with(cx, |panel, cx| {
            let draft = panel.draft_thread.as_ref().expect("draft should exist");
            assert_ne!(
                draft.entity_id(),
                initial_draft_id,
                "a new draft should have been created for the new agent"
            );
            assert_eq!(
                *draft.read(cx).agent_key(),
                Agent::Stub,
                "new draft should use the new agent"
            );
            assert!(
                panel.retained_threads.contains_key(&initial_thread_id),
                "typed draft should have been parked into retained_threads"
            );
        });

        // The parked draft retains the prompt.
        let parked_text = panel.read_with(cx, |panel, cx| panel.editor_text(initial_thread_id, cx));
        assert_eq!(
            parked_text.as_deref(),
            Some("saved prompt"),
            "parked draft should retain the user's prompt"
        );

        // The new draft on the new agent starts empty.
        let active_thread_id = panel.read_with(cx, |panel, cx| panel.active_thread_id(cx).unwrap());
        let active_text = panel.read_with(cx, |panel, cx| panel.editor_text(active_thread_id, cx));
        assert_eq!(
            active_text, None,
            "new draft on the new agent should start empty, not carry the parked draft's prompt"
        );
    }

    #[gpui::test]
    async fn test_rollback_all_succeed_returns_ok(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        cx.update(|cx| {
            cx.update_flags(true, vec!["agent-v2".to_string()]);
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
            <dyn fs::Fs>::set_global(fs.clone(), cx);
        });

        fs.insert_tree(
            "/project",
            json!({
                ".git": {},
                "src": { "main.rs": "fn main() {}" }
            }),
        )
        .await;

        let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;
        cx.executor().run_until_parked();

        let repository = project.read_with(cx, |project, cx| {
            project.repositories(cx).values().next().unwrap().clone()
        });

        let multi_workspace =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));

        let path_a = PathBuf::from("/worktrees/branch/project_a");
        let path_b = PathBuf::from("/worktrees/branch/project_b");

        let (sender_a, receiver_a) = futures::channel::oneshot::channel::<Result<()>>();
        let (sender_b, receiver_b) = futures::channel::oneshot::channel::<Result<()>>();
        sender_a.send(Ok(())).unwrap();
        sender_b.send(Ok(())).unwrap();

        let creation_infos = vec![
            (repository.clone(), path_a.clone(), receiver_a),
            (repository.clone(), path_b.clone(), receiver_b),
        ];

        let fs_clone = fs.clone();
        let result = multi_workspace
            .update(cx, |_, window, cx| {
                window.spawn(cx, async move |cx| {
                    git_ui::worktree_service::await_and_rollback_on_failure(
                        creation_infos,
                        fs_clone,
                        cx,
                    )
                    .await
                })
            })
            .unwrap()
            .await;

        let paths = result.expect("all succeed should return Ok");
        assert_eq!(paths, vec![path_a, path_b]);
    }

    #[gpui::test]
    async fn test_rollback_on_failure_attempts_all_worktrees(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        cx.update(|cx| {
            cx.update_flags(true, vec!["agent-v2".to_string()]);
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
            <dyn fs::Fs>::set_global(fs.clone(), cx);
        });

        fs.insert_tree(
            "/project",
            json!({
                ".git": {},
                "src": { "main.rs": "fn main() {}" }
            }),
        )
        .await;

        let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;
        cx.executor().run_until_parked();

        let repository = project.read_with(cx, |project, cx| {
            project.repositories(cx).values().next().unwrap().clone()
        });

        // Actually create a worktree so it exists in FakeFs for rollback to find.
        let success_path = PathBuf::from("/worktrees/branch/project");
        cx.update(|cx| {
            repository.update(cx, |repo, _| {
                repo.create_worktree(
                    git::repository::CreateWorktreeTarget::NewBranch {
                        branch_name: "branch".to_string(),
                        base_sha: None,
                    },
                    success_path.clone(),
                )
            })
        })
        .await
        .unwrap()
        .unwrap();
        cx.executor().run_until_parked();

        // Verify the worktree directory exists before rollback.
        assert!(
            fs.is_dir(&success_path).await,
            "worktree directory should exist before rollback"
        );

        let multi_workspace =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));

        // Build creation_infos: one success, one failure.
        let failed_path = PathBuf::from("/worktrees/branch/failed_project");

        let (sender_ok, receiver_ok) = futures::channel::oneshot::channel::<Result<()>>();
        let (sender_err, receiver_err) = futures::channel::oneshot::channel::<Result<()>>();
        sender_ok.send(Ok(())).unwrap();
        sender_err
            .send(Err(anyhow!("branch already exists")))
            .unwrap();

        let creation_infos = vec![
            (repository.clone(), success_path.clone(), receiver_ok),
            (repository.clone(), failed_path.clone(), receiver_err),
        ];

        let fs_clone = fs.clone();
        let result = multi_workspace
            .update(cx, |_, window, cx| {
                window.spawn(cx, async move |cx| {
                    git_ui::worktree_service::await_and_rollback_on_failure(
                        creation_infos,
                        fs_clone,
                        cx,
                    )
                    .await
                })
            })
            .unwrap()
            .await;

        assert!(
            result.is_err(),
            "should return error when any creation fails"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("branch already exists"),
            "error should mention the original failure: {err_msg}"
        );

        // The successful worktree should have been rolled back by git.
        cx.executor().run_until_parked();
        assert!(
            !fs.is_dir(&success_path).await,
            "successful worktree directory should be removed by rollback"
        );
    }

    #[gpui::test]
    async fn test_rollback_on_canceled_receiver(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        cx.update(|cx| {
            cx.update_flags(true, vec!["agent-v2".to_string()]);
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
            <dyn fs::Fs>::set_global(fs.clone(), cx);
        });

        fs.insert_tree(
            "/project",
            json!({
                ".git": {},
                "src": { "main.rs": "fn main() {}" }
            }),
        )
        .await;

        let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;
        cx.executor().run_until_parked();

        let repository = project.read_with(cx, |project, cx| {
            project.repositories(cx).values().next().unwrap().clone()
        });

        let multi_workspace =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));

        let path = PathBuf::from("/worktrees/branch/project");

        // Drop the sender to simulate a canceled receiver.
        let (_sender, receiver) = futures::channel::oneshot::channel::<Result<()>>();
        drop(_sender);

        let creation_infos = vec![(repository.clone(), path.clone(), receiver)];

        let fs_clone = fs.clone();
        let result = multi_workspace
            .update(cx, |_, window, cx| {
                window.spawn(cx, async move |cx| {
                    git_ui::worktree_service::await_and_rollback_on_failure(
                        creation_infos,
                        fs_clone,
                        cx,
                    )
                    .await
                })
            })
            .unwrap()
            .await;

        assert!(
            result.is_err(),
            "should return error when receiver is canceled"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("canceled"),
            "error should mention cancellation: {err_msg}"
        );
    }

    #[gpui::test]
    async fn test_rollback_cleans_up_orphan_directories(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        cx.update(|cx| {
            cx.update_flags(true, vec!["agent-v2".to_string()]);
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
            <dyn fs::Fs>::set_global(fs.clone(), cx);
        });

        fs.insert_tree(
            "/project",
            json!({
                ".git": {},
                "src": { "main.rs": "fn main() {}" }
            }),
        )
        .await;

        let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;
        cx.executor().run_until_parked();

        let repository = project.read_with(cx, |project, cx| {
            project.repositories(cx).values().next().unwrap().clone()
        });

        let multi_workspace =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));

        // Simulate the orphan state: create_dir_all was called but git
        // worktree add failed, leaving a directory with leftover files.
        let orphan_path = PathBuf::from("/worktrees/branch/orphan_project");
        fs.insert_tree(
            "/worktrees/branch/orphan_project",
            json!({ "leftover.txt": "junk" }),
        )
        .await;

        assert!(
            fs.is_dir(&orphan_path).await,
            "orphan dir should exist before rollback"
        );

        let (sender, receiver) = futures::channel::oneshot::channel::<Result<()>>();
        sender.send(Err(anyhow!("hook failed"))).unwrap();

        let creation_infos = vec![(repository.clone(), orphan_path.clone(), receiver)];

        let fs_clone = fs.clone();
        let result = multi_workspace
            .update(cx, |_, window, cx| {
                window.spawn(cx, async move |cx| {
                    git_ui::worktree_service::await_and_rollback_on_failure(
                        creation_infos,
                        fs_clone,
                        cx,
                    )
                    .await
                })
            })
            .unwrap()
            .await;

        cx.executor().run_until_parked();

        assert!(result.is_err());
        assert!(
            !fs.is_dir(&orphan_path).await,
            "orphan worktree directory should be removed by filesystem cleanup"
        );
    }

    #[gpui::test]
    async fn test_selected_agent_syncs_when_navigating_between_threads(cx: &mut TestAppContext) {
        let (panel, mut cx) = setup_panel(cx).await;

        let stub_agent = Agent::Custom { id: "Test".into() };

        // Open thread A and send a message so it is retained.
        let connection_a = StubAgentConnection::new();
        connection_a.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
            acp::ContentChunk::new("response a".into()),
        )]);
        open_thread_with_connection(&panel, connection_a, &mut cx);
        let _session_id_a = active_session_id(&panel, &cx);
        let thread_id_a = active_thread_id(&panel, &cx);
        send_message(&panel, &mut cx);
        cx.run_until_parked();

        panel.read_with(&cx, |panel, _cx| {
            assert_eq!(panel.selected_agent, stub_agent);
        });

        // Open thread B with a different agent — thread A goes to retained.
        let custom_agent = Agent::Custom {
            id: "my-custom-agent".into(),
        };
        let connection_b = StubAgentConnection::new()
            .with_agent_id("my-custom-agent".into())
            .with_telemetry_id("my-custom-agent".into());
        connection_b.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
            acp::ContentChunk::new("response b".into()),
        )]);
        open_thread_with_custom_connection(&panel, connection_b, &mut cx);
        send_message(&panel, &mut cx);
        cx.run_until_parked();

        panel.read_with(&cx, |panel, _cx| {
            assert_eq!(
                panel.selected_agent, custom_agent,
                "selected_agent should have changed to the custom agent"
            );
        });

        // Navigate back to thread A via load_agent_thread.
        panel.update_in(&mut cx, |panel, window, cx| {
            panel.load_agent_thread(
                stub_agent.clone(),
                thread_id_a,
                None,
                None,
                true,
                AgentThreadSource::AgentPanel,
                window,
                cx,
            );
        });

        panel.read_with(&cx, |panel, _cx| {
            assert_eq!(
                panel.selected_agent, stub_agent,
                "selected_agent should sync back to thread A's agent"
            );
        });
    }

    #[gpui::test]
    async fn test_classify_worktrees_skips_non_git_root_with_nested_repo(cx: &mut TestAppContext) {
        init_test(cx);
        cx.update(|cx| {
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
        });

        let fs = FakeFs::new(cx.executor());
        fs.insert_tree(
            "/repo_a",
            json!({
                ".git": {},
                "src": { "main.rs": "" }
            }),
        )
        .await;
        fs.insert_tree(
            "/repo_b",
            json!({
                ".git": {},
                "src": { "lib.rs": "" }
            }),
        )
        .await;
        // `plain_dir` is NOT a git repo, but contains a nested git repo.
        fs.insert_tree(
            "/plain_dir",
            json!({
                "nested_repo": {
                    ".git": {},
                    "src": { "lib.rs": "" }
                }
            }),
        )
        .await;

        let project = Project::test(
            fs.clone(),
            [
                Path::new("/repo_a"),
                Path::new("/repo_b"),
                Path::new("/plain_dir"),
            ],
            cx,
        )
        .await;

        // Let the worktree scanner discover all `.git` directories.
        cx.executor().run_until_parked();

        let multi_workspace =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));

        let workspace = multi_workspace
            .read_with(cx, |mw, _cx| mw.workspace().clone())
            .unwrap();

        let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);

        let panel = workspace.update_in(cx, |workspace, window, cx| {
            cx.new(|cx| AgentPanel::new(workspace, window, cx))
        });

        cx.run_until_parked();

        panel.read_with(cx, |panel, cx| {
            let (git_repos, non_git_paths) =
                git_ui::worktree_service::classify_worktrees(panel.project.read(cx), cx);

            let git_work_dirs: Vec<PathBuf> = git_repos
                .iter()
                .map(|repo| repo.read(cx).work_directory_abs_path.to_path_buf())
                .collect();

            assert_eq!(
                git_repos.len(),
                2,
                "only repo_a and repo_b should be classified as git repos, \
                 but got: {git_work_dirs:?}"
            );
            assert!(
                git_work_dirs.contains(&PathBuf::from("/repo_a")),
                "repo_a should be in git_repos: {git_work_dirs:?}"
            );
            assert!(
                git_work_dirs.contains(&PathBuf::from("/repo_b")),
                "repo_b should be in git_repos: {git_work_dirs:?}"
            );

            assert_eq!(
                non_git_paths,
                vec![PathBuf::from("/plain_dir")],
                "plain_dir should be classified as a non-git path \
                 (not matched to nested_repo inside it)"
            );
        });
    }
    #[gpui::test]
    async fn test_vim_search_does_not_steal_focus_from_agent_panel(cx: &mut TestAppContext) {
        init_test(cx);
        cx.update(|cx| {
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
            vim::init(cx);
            search::init(cx);

            // Enable vim mode
            settings::SettingsStore::update_global(cx, |store, cx| {
                store.update_user_settings(cx, |s| s.vim_mode = Some(true));
            });

            // Load vim keybindings
            let mut vim_key_bindings =
                settings::KeymapFile::load_asset_allow_partial_failure("keymaps/vim.json", cx)
                    .unwrap();
            for key_binding in &mut vim_key_bindings {
                key_binding.set_meta(settings::KeybindSource::Vim.meta());
            }
            cx.bind_keys(vim_key_bindings);
        });

        // Create a project with a file so we have a buffer in the center pane.
        let fs = FakeFs::new(cx.executor());
        fs.insert_tree("/project", json!({ "file.txt": "hello world" }))
            .await;
        let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;

        let multi_workspace =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = multi_workspace
            .read_with(cx, |mw, _cx| mw.workspace().clone())
            .unwrap();
        let mut cx = VisualTestContext::from_window(multi_workspace.into(), cx);

        // Open a file in the center pane.
        workspace
            .update_in(&mut cx, |workspace, window, cx| {
                workspace.open_paths(
                    vec![PathBuf::from("/project/file.txt")],
                    workspace::OpenOptions::default(),
                    None,
                    window,
                    cx,
                )
            })
            .await;
        cx.run_until_parked();

        // Add a BufferSearchBar to the center pane's toolbar, as a real
        // workspace would have.
        workspace.update_in(&mut cx, |workspace, window, cx| {
            workspace.active_pane().update(cx, |pane, cx| {
                pane.toolbar().update(cx, |toolbar, cx| {
                    let search_bar = cx.new(|cx| search::BufferSearchBar::new(None, window, cx));
                    toolbar.add_item(search_bar, window, cx);
                });
            });
        });

        // Create the agent panel and add it to the workspace.
        let panel = workspace.update_in(&mut cx, |workspace, window, cx| {
            let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
            workspace.add_panel(panel.clone(), window, cx);
            panel
        });

        // Open a thread so the panel has an active editor.
        open_thread_with_connection(&panel, StubAgentConnection::new(), &mut cx);

        // Focus the agent panel.
        workspace.update_in(&mut cx, |workspace, window, cx| {
            workspace.focus_panel::<AgentPanel>(window, cx);
        });
        cx.run_until_parked();

        // Verify the agent panel has focus.
        workspace.update_in(&mut cx, |_, window, cx| {
            assert!(
                panel.read(cx).focus_handle(cx).contains_focused(window, cx),
                "Agent panel should be focused before pressing '/'"
            );
        });

        // Press '/' — the vim search keybinding.
        cx.simulate_keystrokes("/");

        // Focus should remain on the agent panel.
        workspace.update_in(&mut cx, |_, window, cx| {
            assert!(
                panel.read(cx).focus_handle(cx).contains_focused(window, cx),
                "Focus should remain on the agent panel after pressing '/'"
            );
        });
    }

    /// Connection that tracks closed sessions and detects prompts against
    /// sessions that no longer exist, used to reproduce session disassociation.
    #[derive(Clone, Default)]
    struct DisassociationTrackingConnection {
        next_session_number: Arc<Mutex<usize>>,
        sessions: Arc<Mutex<HashSet<acp::SessionId>>>,
        closed_sessions: Arc<Mutex<Vec<acp::SessionId>>>,
        missing_prompt_sessions: Arc<Mutex<Vec<acp::SessionId>>>,
    }

    impl DisassociationTrackingConnection {
        fn new() -> Self {
            Self::default()
        }

        fn create_session(
            self: Rc<Self>,
            session_id: acp::SessionId,
            project: Entity<Project>,
            work_dirs: PathList,
            title: Option<SharedString>,
            cx: &mut App,
        ) -> Entity<AcpThread> {
            self.sessions.lock().insert(session_id.clone());

            let action_log = cx.new(|_| ActionLog::new(project.clone()));
            cx.new(|cx| {
                AcpThread::new(
                    None,
                    title,
                    Some(work_dirs),
                    self,
                    project,
                    action_log,
                    session_id,
                    watch::Receiver::constant(
                        acp::PromptCapabilities::new()
                            .image(true)
                            .audio(true)
                            .embedded_context(true),
                    ),
                    cx,
                )
            })
        }
    }

    impl AgentConnection for DisassociationTrackingConnection {
        fn agent_id(&self) -> AgentId {
            agent::MAV_AGENT_ID.clone()
        }

        fn telemetry_id(&self) -> SharedString {
            "disassociation-tracking-test".into()
        }

        fn new_session(
            self: Rc<Self>,
            project: Entity<Project>,
            work_dirs: PathList,
            cx: &mut App,
        ) -> Task<Result<Entity<AcpThread>>> {
            let session_id = {
                let mut next_session_number = self.next_session_number.lock();
                let session_id = acp::SessionId::new(format!(
                    "disassociation-tracking-session-{}",
                    *next_session_number
                ));
                *next_session_number += 1;
                session_id
            };
            let thread = self.create_session(session_id, project, work_dirs, None, cx);
            Task::ready(Ok(thread))
        }

        fn supports_load_session(&self) -> bool {
            true
        }

        fn load_session(
            self: Rc<Self>,
            session_id: acp::SessionId,
            project: Entity<Project>,
            work_dirs: PathList,
            title: Option<SharedString>,
            cx: &mut App,
        ) -> Task<Result<Entity<AcpThread>>> {
            let thread = self.create_session(session_id, project, work_dirs, title, cx);
            thread.update(cx, |thread, cx| {
                thread
                    .handle_session_update(
                        acp::SessionUpdate::UserMessageChunk(acp::ContentChunk::new(
                            "Restored user message".into(),
                        )),
                        cx,
                    )
                    .expect("restored user message should be applied");
                thread
                    .handle_session_update(
                        acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk::new(
                            "Restored assistant message".into(),
                        )),
                        cx,
                    )
                    .expect("restored assistant message should be applied");
            });
            Task::ready(Ok(thread))
        }

        fn supports_close_session(&self) -> bool {
            true
        }

        fn close_session(
            self: Rc<Self>,
            session_id: &acp::SessionId,
            _cx: &mut App,
        ) -> Task<Result<()>> {
            self.sessions.lock().remove(session_id);
            self.closed_sessions.lock().push(session_id.clone());
            Task::ready(Ok(()))
        }

        fn auth_methods(&self) -> &[acp::AuthMethod] {
            &[]
        }

        fn authenticate(&self, _method_id: acp::AuthMethodId, _cx: &mut App) -> Task<Result<()>> {
            Task::ready(Ok(()))
        }

        fn prompt(
            &self,
            params: acp::PromptRequest,
            _cx: &mut App,
        ) -> Task<Result<acp::PromptResponse>> {
            if !self.sessions.lock().contains(&params.session_id) {
                self.missing_prompt_sessions.lock().push(params.session_id);
                return Task::ready(Err(anyhow!("Session not found")));
            }

            Task::ready(Ok(acp::PromptResponse::new(acp::StopReason::EndTurn)))
        }

        fn cancel(&self, _session_id: &acp::SessionId, _cx: &mut App) {}

        fn into_any(self: Rc<Self>) -> Rc<dyn Any> {
            self
        }
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

    /// Reproduces the retained-thread reset race:
    ///
    /// 1. Thread A is active and Connected.
    /// 2. User switches to thread B → A goes to retained_threads.
    /// 3. A thread_error is set on retained A's thread view.
    /// 4. AgentServersUpdated fires → retained A's handle_agent_servers_updated
    ///    sees has_thread_error=true → calls reset() → close_all_sessions →
    ///    session X removed, state = Loading.
    /// 5. User reopens thread X via open_thread → load_agent_thread checks
    ///    retained A's has_session → returns false (state is Loading) →
    ///    creates new ConversationView C.
    /// 6. Both A's reload task and C's load task complete → both call
    ///    load_session(X) → both get Connected with session X.
    /// 7. A is eventually cleaned up → on_release → close_all_sessions →
    ///    removes session X.
    /// 8. C sends → "Session not found".
    #[gpui::test]
    async fn test_retained_thread_reset_race_disassociates_session(cx: &mut TestAppContext) {
        let (_workspace, panel, mut cx) = setup_workspace_panel(cx).await;
        cx.run_until_parked();

        let connection = DisassociationTrackingConnection::new();
        panel.update(&mut cx, |panel, cx| {
            panel.connection_store.update(cx, |store, cx| {
                store.restart_connection(
                    Agent::Stub,
                    Rc::new(StubAgentServer::new(connection.clone())),
                    cx,
                );
            });
        });
        cx.run_until_parked();

        // Step 1: Open thread A and send a message.
        panel.update_in(&mut cx, |panel, window, cx| {
            panel.external_thread(
                Some(Agent::Stub),
                None,
                None,
                None,
                None,
                true,
                AgentThreadSource::AgentPanel,
                window,
                cx,
            );
        });
        cx.run_until_parked();
        send_message(&panel, &mut cx);

        let session_id_a = active_session_id(&panel, &cx);
        let _thread_id_a = active_thread_id(&panel, &cx);

        // Step 2: Open thread B → A goes to retained_threads.
        panel.update_in(&mut cx, |panel, window, cx| {
            panel.external_thread(
                Some(Agent::Stub),
                None,
                None,
                None,
                None,
                true,
                AgentThreadSource::AgentPanel,
                window,
                cx,
            );
        });
        cx.run_until_parked();
        send_message(&panel, &mut cx);

        // Confirm A is retained.
        panel.read_with(&cx, |panel, _cx| {
            assert!(
                panel.retained_threads.contains_key(&_thread_id_a),
                "thread A should be in retained_threads after switching to B"
            );
        });

        // Step 3: Set a thread_error on retained A's active thread view.
        // This simulates an API error that occurred before the user switched
        // away, or a transient failure.
        let retained_conversation_a = panel.read_with(&cx, |panel, _cx| {
            panel
                .retained_threads
                .get(&_thread_id_a)
                .expect("thread A should be retained")
                .clone()
        });
        retained_conversation_a.update(&mut cx, |conversation, cx| {
            if let Some(thread_view) = conversation.active_thread() {
                thread_view.update(cx, |view, cx| {
                    view.handle_thread_error(
                        crate::conversation_view::ThreadError::Other {
                            message: "simulated error".into(),
                            acp_error_code: None,
                        },
                        cx,
                    );
                });
            }
        });

        // Confirm the thread error is set.
        retained_conversation_a.read_with(&cx, |conversation, cx| {
            let connected = conversation.as_connected().expect("should be connected");
            assert!(
                connected.has_thread_error(cx),
                "retained A should have a thread error"
            );
        });

        // Step 4: Emit AgentServersUpdated → retained A's
        // handle_agent_servers_updated sees has_thread_error=true,
        // calls reset(), which closes session X and sets state=Loading.
        //
        // Critically, we do NOT call run_until_parked between the emit
        // and open_thread. The emit's synchronous effects (event delivery
        // → reset() → close_all_sessions → state=Loading) happen during
        // the update's flush_effects. But the async reload task spawned
        // by initial_state has NOT been polled yet.
        panel.update(&mut cx, |panel, cx| {
            panel.project.update(cx, |project, cx| {
                project
                    .agent_server_store()
                    .update(cx, |_store, cx| cx.emit(project::AgentServersUpdated));
            });
        });
        // After this update returns, the retained ConversationView is in
        // Loading state (reset ran synchronously), but its async reload
        // task hasn't executed yet.

        // Step 5: Immediately open thread X via open_thread, BEFORE
        // the retained view's async reload completes. load_agent_thread
        // checks retained A's has_session → returns false (state is
        // Loading) → creates a NEW ConversationView C for session X.
        panel.update_in(&mut cx, |panel, window, cx| {
            panel.open_thread(session_id_a.clone(), None, None, window, cx);
        });

        // NOW settle everything: both async tasks (A's reload and C's load)
        // complete, both register session X.
        cx.run_until_parked();

        // Verify session A is the active session via C.
        panel.read_with(&cx, |panel, cx| {
            let active_session = panel
                .active_agent_thread(cx)
                .map(|t| t.read(cx).session_id().clone());
            assert_eq!(
                active_session,
                Some(session_id_a.clone()),
                "session A should be the active session after open_thread"
            );
        });

        // Step 6: Force the retained ConversationView A to be dropped
        // while the active view (C) still has the same session.
        // We can't use remove_thread because C shares the same ThreadId
        // and remove_thread would kill the active view too. Instead,
        // directly remove from retained_threads and drop the handle
        // so on_release → close_all_sessions fires only on A.
        drop(retained_conversation_a);
        panel.update(&mut cx, |panel, _cx| {
            panel.retained_threads.remove(&_thread_id_a);
        });
        cx.run_until_parked();

        // The key assertion: sending messages on the ACTIVE view (C)
        // must succeed. If the session was disassociated by A's cleanup,
        // this will fail with "Session not found".
        send_message(&panel, &mut cx);
        send_message(&panel, &mut cx);

        let missing = connection.missing_prompt_sessions.lock().clone();
        assert!(
            missing.is_empty(),
            "session should not be disassociated after retained thread reset race, \
             got missing prompt sessions: {:?}",
            missing
        );

        panel.read_with(&cx, |panel, cx| {
            let active_view = panel
                .active_conversation_view()
                .expect("conversation should remain open");
            let connected = active_view
                .read(cx)
                .as_connected()
                .expect("conversation should be connected");
            assert!(
                !connected.has_thread_error(cx),
                "conversation should not have a thread error"
            );
        });
    }

    #[gpui::test]
    async fn test_initialize_from_source_transfers_draft_to_fresh_panel(cx: &mut TestAppContext) {
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

        // Set up panel_a with an active thread and type draft text.
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

        // Set up panel_b on workspace_b — starts as a fresh, empty panel.
        let panel_b = workspace_b.update_in(cx, |workspace, window, cx| {
            let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
            workspace.add_panel(panel.clone(), window, cx);
            panel
        });
        cx.run_until_parked();

        // Initializing panel_b from workspace_a should transfer the draft,
        // even if panel_b already has an auto-created empty draft thread
        // (which set_active creates during add_panel).
        let transferred = panel_b.update_in(cx, |panel, window, cx| {
            panel.initialize_from_source_workspace_if_needed(workspace_a.downgrade(), window, cx)
        });
        assert!(
            transferred,
            "fresh destination panel should accept source content"
        );

        // Verify the panel was initialized: the base_view should now be an
        // AgentThread (not Uninitialized) and a draft_thread should be set.
        // We can't check the message editor text directly because the thread
        // needs a connected server session (not available in unit tests without
        // a stub server). The `transferred == true` return already proves that
        // source_panel_initialization read the content successfully.
        panel_b.read_with(cx, |panel, _cx| {
            assert!(
                panel.active_conversation_view().is_some(),
                "panel_b should have a conversation view after initialization"
            );
            assert!(
                panel.draft_thread.is_some(),
                "panel_b should have a draft_thread set after initialization"
            );
        });
    }

    #[gpui::test]
    async fn test_initialize_from_source_inherits_agent_without_draft_content(
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

        let panel_a = workspace_a.update_in(cx, |workspace, window, cx| {
            let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
            workspace.add_panel(panel.clone(), window, cx);
            panel
        });

        panel_a.update(cx, |panel, _cx| {
            panel.selected_agent = Agent::Stub;
        });

        let panel_b = workspace_b.update_in(cx, |workspace, window, cx| {
            let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
            workspace.add_panel(panel.clone(), window, cx);
            panel
        });

        let initialized = panel_b.update_in(cx, |panel, window, cx| {
            panel.initialize_from_source_workspace_if_needed(workspace_a.downgrade(), window, cx)
        });
        assert!(
            initialized,
            "fresh destination panel should inherit the source agent"
        );

        panel_b.read_with(cx, |panel, _cx| {
            assert_eq!(
                panel.selected_agent,
                Agent::Stub,
                "destination panel should inherit the source panel's selected agent"
            );
            assert!(
                panel.active_conversation_view().is_none(),
                "agent-only initialization should not create a draft thread"
            );
        });
    }

    #[gpui::test]
    async fn test_initialize_from_source_retargets_empty_destination_draft_agent(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);
        cx.update(|cx| {
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
        });

        let fs = FakeFs::new(cx.executor());
        cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));
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

        let panel_a = workspace_a.update_in(cx, |workspace, window, cx| {
            let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
            workspace.add_panel(panel.clone(), window, cx);
            panel
        });

        panel_a.update(cx, |panel, _cx| {
            panel.selected_agent = Agent::Stub;
        });

        let panel_b = workspace_b.update_in(cx, |workspace, window, cx| {
            let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
            workspace.add_panel(panel.clone(), window, cx);
            panel
        });
        panel_b.update_in(cx, |panel, window, cx| {
            panel.activate_new_thread(false, AgentThreadSource::AgentPanel, window, cx);
        });

        let original_draft = panel_b.read_with(cx, |panel, cx| {
            let draft = panel.draft_thread.as_ref().expect("draft should exist");
            assert_eq!(
                *draft.read(cx).agent_key(),
                Agent::NativeAgent,
                "destination draft should start on the default agent"
            );
            draft.entity_id()
        });

        let initialized = panel_b.update_in(cx, |panel, window, cx| {
            panel.initialize_from_source_workspace_if_needed(workspace_a.downgrade(), window, cx)
        });
        assert!(
            initialized,
            "fresh destination draft should inherit the source agent"
        );

        panel_b.read_with(cx, |panel, cx| {
            let draft = panel.draft_thread.as_ref().expect("draft should exist");
            assert_ne!(
                draft.entity_id(),
                original_draft,
                "empty destination draft should be replaced when the inherited agent differs"
            );
            assert_eq!(
                *draft.read(cx).agent_key(),
                Agent::Stub,
                "empty destination draft should be rebound to the inherited agent"
            );
        });
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
