use agent::ThreadStore;
use agent_settings::AgentSettings;
use collections::{HashMap, VecDeque};
use editor::actions::Paste;
use editor::code_context_menus::CodeContextMenu;
use editor::display_map::{CreaseId, EditorMargins};
use editor::{AnchorRangeExt as _, MultiBufferOffset, ToOffset as _};
use editor::{
    ContextMenuOptions, Editor, EditorElement, EditorEvent, EditorMode, EditorStyle, MultiBuffer,
};
use fs::Fs;
use gpui::{
    AnyElement, App, ClipboardItem, Context, Entity, EventEmitter, FocusHandle, Focusable,
    Subscription, TextStyle, TextStyleRefinement, WeakEntity, Window, actions,
};
use language_model::{LanguageModel, LanguageModelRegistry};
use markdown::{HeadingLevelStyles, Markdown, MarkdownElement, MarkdownStyle};
use mav_actions::{
    agent::ToggleModelSelector,
    editor::{MoveDown, MoveUp},
};
use parking_lot::Mutex;
use project::Project;
use settings::Settings;
use std::cmp;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;
use theme_settings::ThemeSettings;
use ui::utils::WithRemSize;
use ui::{IconButtonShape, KeyBinding, PopoverMenuHandle, Tooltip, prelude::*};
use uuid::Uuid;
use workspace::notifications::NotificationId;
use workspace::{Toast, Workspace};

use crate::agent_model_selector::AgentModelSelector;
use crate::buffer_codegen::{BufferCodegen, CodegenAlternative};
use crate::completion_provider::{
    PromptCompletionProvider, PromptCompletionProviderDelegate, PromptContextType,
};
use crate::mention_set::paste_images_as_context;
use crate::mention_set::{MentionSet, crease_for_mention};
use crate::terminal_codegen::TerminalCodegen;
use crate::{
    CycleFavoriteModels, CycleNextInlineAssist, CyclePreviousInlineAssist, ModelUsageContext,
};

actions!(inline_assistant, [ThumbsUpResult, ThumbsDownResult]);

enum CompletionState {
    Pending,
    Generated { completion_text: Option<String> },
    Rated,
}

struct SessionState {
    session_id: Uuid,
    completion: CompletionState,
}

pub struct PromptEditor<T> {
    pub editor: Entity<Editor>,
    mode: PromptEditorMode,
    mention_set: Entity<MentionSet>,
    workspace: WeakEntity<Workspace>,
    model_selector: Entity<AgentModelSelector>,
    edited_since_done: bool,
    prompt_history: VecDeque<String>,
    prompt_history_ix: Option<usize>,
    pending_prompt: String,
    _codegen_subscription: Subscription,
    editor_subscriptions: Vec<Subscription>,
    show_rate_limit_notice: bool,
    session_state: SessionState,
    _phantom: std::marker::PhantomData<T>,
}

impl<T: 'static> EventEmitter<PromptEditorEvent> for PromptEditor<T> {}

mod buffer;
mod completion;
mod core;
mod feedback;
mod render;
mod render_controls;
mod terminal;
#[cfg(test)]
mod tests;

use completion::{PromptEditorCompletionProviderDelegate, inline_assistant_model_supports_images};
pub use terminal::TerminalInlineAssistId;

pub enum PromptEditorMode {
    Buffer {
        id: InlineAssistId,
        codegen: Entity<BufferCodegen>,
        editor_margins: Arc<Mutex<EditorMargins>>,
    },
    Terminal {
        id: TerminalInlineAssistId,
        codegen: Entity<TerminalCodegen>,
        height_in_lines: u8,
    },
}

pub enum PromptEditorEvent {
    StartRequested,
    StopRequested,
    ConfirmRequested { execute: bool },
    CancelRequested,
    Resized { height_in_lines: u8 },
}

#[derive(Copy, Clone, Default, Debug, PartialEq, Eq, Hash)]
pub struct InlineAssistId(pub usize);

impl InlineAssistId {
    pub fn post_inc(&mut self) -> InlineAssistId {
        let id = *self;
        self.0 += 1;
        id
    }
}

pub enum CodegenStatus {
    Idle,
    Pending,
    Done,
    Error(anyhow::Error),
}

#[derive(Copy, Clone)]
pub enum GenerationMode {
    Generate,
    Transform,
}

impl GenerationMode {
    fn start_label(self) -> &'static str {
        match self {
            GenerationMode::Generate => "Generate",
            GenerationMode::Transform => "Transform",
        }
    }
    fn tooltip_interrupt(self) -> &'static str {
        match self {
            GenerationMode::Generate => "Interrupt Generation",
            GenerationMode::Transform => "Interrupt Transform",
        }
    }

    fn tooltip_restart(self) -> &'static str {
        match self {
            GenerationMode::Generate => "Restart Generation",
            GenerationMode::Transform => "Restart Transform",
        }
    }

    fn tooltip_accept(self) -> &'static str {
        match self {
            GenerationMode::Generate => "Accept Generation",
            GenerationMode::Transform => "Accept Transform",
        }
    }
}

/// Stored information that can be used to resurrect a context crease when creating an editor for a past message.
#[derive(Clone, Debug)]
struct MessageCrease {
    range: Range<MultiBufferOffset>,
    icon_path: SharedString,
    label: SharedString,
}

fn extract_message_creases(
    editor: &mut Editor,
    mention_set: &Entity<MentionSet>,
    window: &mut Window,
    cx: &mut Context<'_, Editor>,
) -> Vec<MessageCrease> {
    let creases = mention_set.read(cx).creases();
    let snapshot = editor.snapshot(window, cx);
    snapshot
        .crease_snapshot
        .creases()
        .filter(|(id, _)| creases.contains(id))
        .filter_map(|(_, crease)| {
            let metadata = crease.metadata()?.clone();
            Some(MessageCrease {
                range: crease.range().to_offset(snapshot.buffer()),
                label: metadata.label,
                icon_path: metadata.icon_path,
            })
        })
        .collect()
}

fn insert_message_creases(
    editor: &mut Editor,
    message_creases: &[MessageCrease],
    window: &mut Window,
    cx: &mut Context<'_, Editor>,
) -> Vec<CreaseId> {
    let buffer_snapshot = editor.buffer().read(cx).snapshot(cx);
    let creases = message_creases
        .iter()
        .map(|crease| {
            let start = buffer_snapshot.anchor_after(crease.range.start);
            let end = buffer_snapshot.anchor_before(crease.range.end);
            crease_for_mention(
                crease.label.clone(),
                crease.icon_path.clone(),
                None,
                start..end,
                cx.weak_entity(),
            )
        })
        .collect::<Vec<_>>();
    let ids = editor.insert_creases(creases.clone(), cx);
    editor.fold_creases(creases, false, window, cx);
    ids
}
