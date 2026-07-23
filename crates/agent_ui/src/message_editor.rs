use crate::DEFAULT_THREAD_TITLE;
use crate::SendImmediately;
use crate::{
    ChatWithFollow,
    completion_provider::{
        AgentContextSelection, AvailableCommand, AvailableSkill, PromptCompletionProvider,
        PromptCompletionProviderDelegate, PromptContextAction, PromptContextType,
        SlashCommandCompletion,
    },
    mention_set::{Mention, MentionImage, MentionSet, insert_crease_for_mention},
};
use acp_thread::MentionUri;
use agent::ThreadStore;
use agent_client_protocol::schema::v1 as acp;
use anyhow::{Result, anyhow};
use base64::Engine as _;
use editor::{
    Addon, AnchorRangeExt, ContextMenuOptions, Editor, EditorElement, EditorEvent, EditorMode,
    EditorStyle, Inlay, MultiBuffer, MultiBufferOffset, MultiBufferSnapshot, ToOffset,
    actions::{Copy, Cut, Paste},
    code_context_menus::CodeContextMenu,
    display_map::{CreaseId, CreaseSnapshot},
    scroll::Autoscroll,
};
use futures::{FutureExt as _, future::join_all};
use gpui::{
    AppContext, ClipboardEntry, ClipboardItem, Context, Entity, EventEmitter, FocusHandle,
    Focusable, Image, ImageFormat, KeyContext, SharedString, Subscription, Task, TaskExt,
    TextStyle, WeakEntity,
};
use language::{Buffer, language_settings::InlayHintKind};
use mav_actions::agent::{Chat, PasteRaw};
use parking_lot::RwLock;
use project::AgentId;
use project::{
    CompletionIntent, InlayHint, InlayHintLabel, InlayId, Project, ProjectPath, Worktree,
};
use rope::Point;
use settings::Settings;
use std::{cmp::min, fmt::Write, ops::Range, rc::Rc, sync::Arc};
use text::LineEnding;
use theme_settings::ThemeSettings;
use ui::{ContextMenu, prelude::*};
use util::paths::PathStyle;
use util::{ResultExt, debug_panic};
use workspace::{CollaboratorId, Workspace};

mod capabilities;
mod pasted_context;
use capabilities::MessageEditorCompletionDelegate;
pub use capabilities::{SessionCapabilities, SharedSessionCapabilities};
use pasted_context::{
    insert_mention_for_project_path, insert_resolved_pasted_context_items,
    resolve_pasted_context_items,
};

pub struct MessageEditor {
    mention_set: Entity<MentionSet>,
    editor: Entity<Editor>,
    workspace: WeakEntity<Workspace>,
    project: WeakEntity<Project>,
    session_capabilities: SharedSessionCapabilities,
    agent_id: AgentId,
    thread_store: Option<Entity<ThreadStore>>,
    _subscriptions: Vec<Subscription>,
    _parse_slash_command_task: Task<()>,
}

#[derive(Clone, Debug)]
pub enum InputAttempt {
    Text(Arc<str>),
    Paste(ClipboardItem),
}

#[derive(Clone, Debug)]
pub enum MessageEditorEvent {
    Send,
    SendImmediately,
    Cancel,
    Focus,
    LostFocus,
    Edited,
    /// Emitted when the user opens slash-command autocomplete in this
    /// editor. Used by `ThreadView` to fire the global-skills scan
    /// trigger; see `NativeAgent::ensure_skills_scan_started`.
    SlashAutocompleteOpened,
    InputAttempted {
        attempt: InputAttempt,
        cursor_offset: usize,
    },
}

impl EventEmitter<MessageEditorEvent> for MessageEditor {}

const COMMAND_HINT_INLAY_ID: InlayId = InlayId::Hint(0);

mod actions;
mod content;
mod content_helpers;
mod core;
mod editing;
mod insertions;
mod paste;
mod render;

impl MessageEditor {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        project: WeakEntity<Project>,
        thread_store: Option<Entity<ThreadStore>>,
        session_capabilities: SharedSessionCapabilities,
        agent_id: AgentId,
        placeholder: &str,
        mode: EditorMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let language_registry = project
            .upgrade()
            .map(|project| project.read(cx).languages().clone());

        let editor = cx.new(|cx| {
            let buffer = cx.new(|cx| {
                let buffer = Buffer::local("", cx);
                if let Some(language_registry) = language_registry.as_ref() {
                    buffer.set_language_registry(language_registry.clone());
                }
                buffer
            });
            let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));

            let mut editor = Editor::new(mode, buffer, None, window, cx);
            editor.set_placeholder_text(placeholder, window, cx);
            editor.set_show_indent_guides(false, cx);
            editor.set_show_completions_on_input(Some(true));
            editor.set_soft_wrap();
            editor.disable_mouse_wheel_zoom();
            editor.set_use_modal_editing(true);
            editor.set_context_menu_options(ContextMenuOptions {
                min_entries_visible: 12,
                max_entries_visible: 12,
                placement: None,
            });
            editor.register_addon(MessageEditorAddon::new());

            editor.set_custom_context_menu(|editor, _point, window, cx| {
                let has_selection = editor.has_non_empty_selection(&editor.display_snapshot(cx));

                Some(ContextMenu::build(window, cx, |menu, _, _| {
                    menu.action("Cut", Box::new(editor::actions::Cut))
                        .action_disabled_when(
                            !has_selection,
                            "Copy",
                            Box::new(editor::actions::Copy),
                        )
                        .action("Paste", Box::new(editor::actions::Paste))
                        .action("Paste as Plain Text", Box::new(PasteRaw))
                }))
            });

            editor
        });
        let mention_set = cx.new(|_cx| MentionSet::new(project.clone(), thread_store.clone()));
        let completion_provider = Rc::new(PromptCompletionProvider::new(
            MessageEditorCompletionDelegate {
                session_capabilities: session_capabilities.clone(),
                has_thread_store: thread_store.is_some(),
                message_editor: cx.weak_entity(),
            },
            editor.downgrade(),
            mention_set.clone(),
            workspace.clone(),
        ));
        editor.update(cx, |editor, _cx| {
            editor.set_completion_provider(Some(completion_provider.clone()))
        });

        cx.on_focus_in(&editor.focus_handle(cx), window, |_, _, cx| {
            cx.emit(MessageEditorEvent::Focus)
        })
        .detach();
        cx.on_focus_out(&editor.focus_handle(cx), window, |_, _, _, cx| {
            cx.emit(MessageEditorEvent::LostFocus)
        })
        .detach();

        let mut has_hint = false;
        let mut subscriptions = Vec::new();

        subscriptions.push(cx.subscribe_in(&editor, window, {
            move |this, editor, event, window, cx| {
                let input_attempted_text = match event {
                    EditorEvent::InputHandled { text, .. } => Some(text),
                    EditorEvent::InputIgnored { text } => Some(text),
                    _ => None,
                };
                if let Some(text) = input_attempted_text
                    && editor.read(cx).read_only(cx)
                    && !text.is_empty()
                {
                    let editor = editor.read(cx);
                    let cursor_anchor = editor.selections.newest_anchor().head();
                    let cursor_offset = cursor_anchor
                        .to_offset(&editor.buffer().read(cx).snapshot(cx))
                        .0;
                    cx.emit(MessageEditorEvent::InputAttempted {
                        attempt: InputAttempt::Text(text.clone()),
                        cursor_offset,
                    });
                }

                if let EditorEvent::Edited { .. } = event
                    && !editor.read(cx).read_only(cx)
                {
                    cx.emit(MessageEditorEvent::Edited);
                    editor.update(cx, |editor, cx| {
                        let snapshot = editor.snapshot(window, cx);
                        this.mention_set
                            .update(cx, |mention_set, _cx| mention_set.remove_invalid(&snapshot));

                        let new_hints = this
                            .command_hint(snapshot.buffer())
                            .into_iter()
                            .collect::<Vec<_>>();
                        let has_new_hint = !new_hints.is_empty();
                        editor.splice_inlays(
                            if has_hint {
                                &[COMMAND_HINT_INLAY_ID]
                            } else {
                                &[]
                            },
                            new_hints,
                            cx,
                        );
                        has_hint = has_new_hint;
                    });
                    cx.notify();
                }
            }
        }));

        if let Some(language_registry) = language_registry {
            let editor = editor.clone();
            cx.spawn(async move |_, cx| {
                let markdown = language_registry.language_for_name("Markdown").await?;
                editor.update(cx, |editor, cx| {
                    if let Some(buffer) = editor.buffer().read(cx).as_singleton() {
                        buffer.update(cx, |buffer, cx| {
                            buffer.set_language(Some(markdown), cx);
                        });
                    }
                });
                anyhow::Ok(())
            })
            .detach_and_log_err(cx);
        }

        Self {
            editor,
            mention_set,
            workspace,
            project,
            session_capabilities,
            agent_id,
            thread_store,
            _subscriptions: subscriptions,
            _parse_slash_command_task: Task::ready(()),
        }
    }
}

#[cfg(test)]
mod tests;
