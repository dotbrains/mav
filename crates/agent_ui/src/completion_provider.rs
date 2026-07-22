use std::cmp::Reverse;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::DEFAULT_THREAD_TITLE;
use crate::thread_metadata_store::{ThreadMetadata, ThreadMetadataStore};
use acp_thread::MentionUri;
use agent_client_protocol::schema::v1 as acp;
use anyhow::Result;
use editor::{CompletionProvider, Editor, code_context_menus::COMPLETION_MENU_MAX_WIDTH};
use futures::FutureExt as _;
use fuzzy::{PathMatch, StringMatch, StringMatchCandidate};
use gpui::{
    App, BackgroundExecutor, Entity, Focusable, Hsla, SharedString, Task, WeakEntity, Window,
};
use language::{Buffer, CodeLabel, CodeLabelBuilder, HighlightId};
use lsp::CompletionContext;
use multi_buffer::ToOffset as _;
use ordered_float::OrderedFloat;
use project::lsp_store::{CompletionDocumentation, SymbolLocation};
use project::{
    Completion, CompletionDisplayOptions, CompletionGroup, CompletionIntent, CompletionResponse,
    DiagnosticSummary, PathMatchCandidateSet, Project, ProjectPath, Symbol, WorktreeId,
};

use rope::Point;
use terminal_view::{TerminalView, terminal_panel::TerminalPanel};
use text::{Anchor, ToOffset as _, ToPoint as _};
use ui::IconName;
use ui::prelude::*;
use util::ResultExt as _;
use util::paths::PathStyle;
use util::rel_path::RelPath;
use util::truncate_and_remove_front;
use workspace::Workspace;

use crate::AgentPanel;
use crate::mention_set::MentionSet;

#[path = "completion_provider/completion_models.rs"]
mod completion_models;
use completion_models::*;
#[path = "completion_provider/context.rs"]
mod context;
use context::*;
#[path = "completion_provider/confirm.rs"]
mod confirm;
use confirm::*;
#[path = "completion_provider/parsing.rs"]
mod parsing;
use parsing::*;
#[path = "completion_provider/diagnostics.rs"]
mod diagnostics;
use diagnostics::*;
#[path = "completion_provider/search.rs"]
mod search;
use search::*;
#[path = "completion_provider/labels.rs"]
mod labels;
use labels::*;
#[path = "completion_provider/completion_items.rs"]
mod completion_items;
#[path = "completion_provider/diagnostic_completions.rs"]
mod diagnostic_completions;
#[path = "completion_provider/provider_impl.rs"]
mod provider_impl;
#[path = "completion_provider/provider_queries.rs"]
mod provider_queries;
#[path = "completion_provider/selections.rs"]
mod selections;
use selections::*;

#[path = "completion_provider/integration_tests.rs"]
#[cfg(test)]
mod integration_tests;
#[path = "completion_provider/parsing_tests.rs"]
#[cfg(test)]
mod parsing_tests;

pub trait PromptCompletionProviderDelegate: Send + Sync + 'static {
    fn supports_context(&self, mode: PromptContextType, cx: &App) -> bool {
        self.supported_modes(cx).contains(&mode)
    }
    fn supported_modes(&self, cx: &App) -> Vec<PromptContextType>;
    fn supports_images(&self, cx: &App) -> bool;

    fn available_commands(&self, cx: &App) -> Vec<AvailableCommand>;
    fn available_skills(&self, _cx: &App) -> Vec<AvailableSkill> {
        Vec::new()
    }
    fn confirm_command(&self, cx: &mut App);

    /// Called once each time the user opens slash-command autocomplete
    /// in the editor this delegate serves. Implementations may use it
    /// to lazily kick off work that produces commands (for example,
    /// scanning the global skills directory). The default is a no-op.
    fn slash_autocomplete_invoked(&self, _cx: &mut App) {}
}

pub struct PromptCompletionProvider<T: PromptCompletionProviderDelegate> {
    source: Arc<T>,
    editor: WeakEntity<Editor>,
    mention_set: Entity<MentionSet>,
    workspace: WeakEntity<Workspace>,
}

impl<T: PromptCompletionProviderDelegate> PromptCompletionProvider<T> {
    pub fn new(
        source: T,
        editor: WeakEntity<Editor>,
        mention_set: Entity<MentionSet>,
        workspace: WeakEntity<Workspace>,
    ) -> Self {
        Self {
            source: Arc::new(source),
            editor,
            mention_set,
            workspace,
        }
    }
}
