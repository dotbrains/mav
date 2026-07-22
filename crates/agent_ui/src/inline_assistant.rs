use language_models::provider::anthropic::telemetry::{
    AnthropicCompletionType, AnthropicEventData, AnthropicEventType, report_anthropic_event,
};
use std::mem;
use std::ops::Range;
use std::sync::Arc;
use uuid::Uuid;

use crate::context::load_context;
use crate::mention_set::MentionSet;
use crate::{
    AgentPanel,
    buffer_codegen::{BufferCodegen, CodegenAlternative, CodegenEvent},
    inline_prompt_editor::{CodegenStatus, InlineAssistId, PromptEditor, PromptEditorEvent},
    terminal_inline_assistant::TerminalInlineAssistant,
};
use agent::ThreadStore;
use agent_settings::AgentSettings;
use anyhow::{Context as _, Result};
use collections::{HashMap, HashSet, VecDeque, hash_map};
use editor::EditorSnapshot;
use editor::MultiBufferOffset;
use editor::RowExt;
use editor::SelectionEffects;
use editor::scroll::ScrollOffset;
use editor::{
    Anchor, AnchorRangeExt, Editor, EditorEvent, HighlightKey, MultiBuffer, MultiBufferSnapshot,
    ToOffset as _, ToPoint,
    actions::SelectAll,
    display_map::{
        BlockContext, BlockPlacement, BlockProperties, BlockStyle, CustomBlockId, EditorMargins,
        RenderBlock, ToDisplayPoint,
    },
};
use fs::Fs;
use futures::{FutureExt, channel::mpsc};
use gpui::{
    App, Context, Entity, Focusable, Global, HighlightStyle, Subscription, Task, TaskExt,
    UpdateGlobal, WeakEntity, Window, point,
};
use language::{Buffer, Point, Selection, TransactionId};
use language_model::{ConfigurationError, ConfiguredModel, LanguageModelRegistry};
use multi_buffer::MultiBufferRow;
use parking_lot::Mutex;
use project::{DisableAiSettings, Project};
use prompt_store::PromptBuilder;
use settings::{Settings, SettingsStore};

use mav_actions::agent::OpenSettings;
use terminal_view::{TerminalView, terminal_panel::TerminalPanel};
use ui::prelude::*;
use util::{RangeExt, ResultExt, maybe};
use workspace::{Toast, Workspace, dock::Panel, notifications::NotificationId};

mod assist_flow;
mod assist_lifecycle;
mod assist_state;
mod editor_updates;
mod event_handlers;

#[cfg(all(test, feature = "unit-eval"))]
pub mod evals;

use assist_state::{
    EditorInlineAssists, InlineAssist, InlineAssistGroup, InlineAssistGroupId,
    InlineAssistScrollLock, build_assist_editor_renderer, merge_ranges,
};

pub fn init(fs: Arc<dyn Fs>, prompt_builder: Arc<PromptBuilder>, cx: &mut App) {
    cx.set_global(InlineAssistant::new(fs, prompt_builder));

    cx.observe_global::<SettingsStore>(|cx| {
        if DisableAiSettings::get_global(cx).disable_ai {
            // Hide any active inline assist UI when AI is disabled
            InlineAssistant::update_global(cx, |assistant, cx| {
                assistant.cancel_all_active_completions(cx);
            });
        }
    })
    .detach();

    cx.observe_new(|_workspace: &mut Workspace, window, cx| {
        let Some(window) = window else {
            return;
        };
        let workspace = cx.entity();
        InlineAssistant::update_global(cx, |inline_assistant, cx| {
            inline_assistant.register_workspace(&workspace, window, cx)
        });
    })
    .detach();
}

const PROMPT_HISTORY_MAX_LEN: usize = 20;

enum InlineAssistTarget {
    Editor(Entity<Editor>),
    Terminal(Entity<TerminalView>),
}

pub struct InlineAssistant {
    next_assist_id: InlineAssistId,
    next_assist_group_id: InlineAssistGroupId,
    assists: HashMap<InlineAssistId, InlineAssist>,
    assists_by_editor: HashMap<WeakEntity<Editor>, EditorInlineAssists>,
    assist_groups: HashMap<InlineAssistGroupId, InlineAssistGroup>,
    confirmed_assists: HashMap<InlineAssistId, Entity<CodegenAlternative>>,
    prompt_history: VecDeque<String>,
    prompt_builder: Arc<PromptBuilder>,
    fs: Arc<dyn Fs>,
    _inline_assistant_completions: Option<mpsc::UnboundedSender<anyhow::Result<InlineAssistId>>>,
}

impl Global for InlineAssistant {}

impl InlineAssistant {
    pub fn new(fs: Arc<dyn Fs>, prompt_builder: Arc<PromptBuilder>) -> Self {
        Self {
            next_assist_id: InlineAssistId::default(),
            next_assist_group_id: InlineAssistGroupId::default(),
            assists: HashMap::default(),
            assists_by_editor: HashMap::default(),
            assist_groups: HashMap::default(),
            confirmed_assists: HashMap::default(),
            prompt_history: VecDeque::default(),
            prompt_builder,
            fs,
            _inline_assistant_completions: None,
        }
    }

    pub fn register_workspace(
        &mut self,
        workspace: &Entity<Workspace>,
        window: &mut Window,
        cx: &mut App,
    ) {
        window
            .subscribe(workspace, cx, |workspace, event, window, cx| {
                Self::update_global(cx, |this, cx| {
                    this.handle_workspace_event(workspace, event, window, cx)
                });
            })
            .detach();

        let workspace_weak = workspace.downgrade();
        cx.observe_global::<SettingsStore>(move |cx| {
            let Some(workspace) = workspace_weak.upgrade() else {
                return;
            };
            let Some(terminal_panel) = workspace.read(cx).panel::<TerminalPanel>(cx) else {
                return;
            };
            let enabled = AgentSettings::get_global(cx).enabled(cx);
            terminal_panel.update(cx, |terminal_panel, cx| {
                terminal_panel.set_assistant_enabled(enabled, cx)
            });
        })
        .detach();

        cx.observe(workspace, |workspace, cx| {
            let Some(terminal_panel) = workspace.read(cx).panel::<TerminalPanel>(cx) else {
                return;
            };
            let enabled = AgentSettings::get_global(cx).enabled(cx);
            if terminal_panel.read(cx).assistant_enabled() != enabled {
                terminal_panel.update(cx, |terminal_panel, cx| {
                    terminal_panel.set_assistant_enabled(enabled, cx)
                });
            }
        })
        .detach();
    }

    /// Hides all active inline assists when AI is disabled
    pub fn cancel_all_active_completions(&mut self, cx: &mut App) {
        // Cancel all active completions in editors
        for (editor_handle, _) in self.assists_by_editor.iter() {
            if let Some(editor) = editor_handle.upgrade() {
                let windows = cx.windows();
                if !windows.is_empty() {
                    let window = windows[0];
                    let _ = window.update(cx, |_, window, cx| {
                        editor.update(cx, |editor, cx| {
                            if editor.has_active_edit_prediction() {
                                editor.cancel(&Default::default(), window, cx);
                            }
                        });
                    });
                }
            }
        }
    }

    fn handle_workspace_event(
        &mut self,
        _workspace: Entity<Workspace>,
        event: &workspace::Event,
        window: &mut Window,
        cx: &mut App,
    ) {
        match event {
            workspace::Event::UserSavedItem { item, .. } => {
                // When the user manually saves an editor, automatically accepts all finished transformations.
                if let Some(editor) = item.upgrade().and_then(|item| item.act_as::<Editor>(cx))
                    && let Some(editor_assists) = self.assists_by_editor.get(&editor.downgrade())
                {
                    for assist_id in editor_assists.assist_ids.clone() {
                        let assist = &self.assists[&assist_id];
                        if let CodegenStatus::Done = assist.codegen.read(cx).status(cx) {
                            self.finish_assist(assist_id, false, window, cx)
                        }
                    }
                }
            }
            _ => (),
        }
    }

    pub fn inline_assist(
        workspace: &mut Workspace,
        action: &mav_actions::assistant::InlineAssist,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        if !AgentSettings::get_global(cx).enabled(cx) {
            return;
        }

        let Some(inline_assist_target) = Self::resolve_inline_assist_target(workspace, window, cx)
        else {
            return;
        };

        let configuration_error = |cx| {
            let model_registry = LanguageModelRegistry::read_global(cx);
            model_registry.configuration_error(model_registry.inline_assistant_model(), cx)
        };

        let Some(agent_panel) = workspace.panel::<AgentPanel>(cx) else {
            return;
        };
        let agent_panel = agent_panel.read(cx);

        let thread_store = agent_panel.thread_store().clone();

        let handle_assist =
            |window: &mut Window, cx: &mut Context<Workspace>| match inline_assist_target {
                InlineAssistTarget::Editor(active_editor) => {
                    InlineAssistant::update_global(cx, |assistant, cx| {
                        assistant.assist(
                            &active_editor,
                            cx.entity().downgrade(),
                            workspace.project().downgrade(),
                            thread_store,
                            action.prompt.clone(),
                            window,
                            cx,
                        );
                    })
                }
                InlineAssistTarget::Terminal(active_terminal) => {
                    TerminalInlineAssistant::update_global(cx, |assistant, cx| {
                        assistant.assist(
                            &active_terminal,
                            cx.entity().downgrade(),
                            workspace.project().downgrade(),
                            thread_store,
                            action.prompt.clone(),
                            window,
                            cx,
                        );
                    });
                }
            };

        if let Some(error) = configuration_error(cx) {
            if let ConfigurationError::ProviderNotAuthenticated(provider) = error {
                cx.spawn(async move |_, cx| {
                    cx.update(|cx| provider.authenticate(cx)).await?;
                    anyhow::Ok(())
                })
                .detach_and_log_err(cx);

                if configuration_error(cx).is_none() {
                    handle_assist(window, cx);
                }
            } else {
                cx.spawn_in(window, async move |_, cx| {
                    let answer = cx
                        .prompt(
                            gpui::PromptLevel::Warning,
                            &error.to_string(),
                            None,
                            &["Configure", "Cancel"],
                        )
                        .await
                        .ok();
                    if let Some(answer) = answer
                        && answer == 0
                    {
                        cx.update(|window, cx| window.dispatch_action(Box::new(OpenSettings), cx))
                            .ok();
                    }
                    anyhow::Ok(())
                })
                .detach_and_log_err(cx);
            }
        } else {
            handle_assist(window, cx);
        }
    }
}
