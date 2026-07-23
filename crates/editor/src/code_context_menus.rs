use crate::scroll::ScrollAmount;
use fuzzy::{StringMatch, StringMatchCandidate};
use gpui::{
    AnyElement, Entity, Focusable, FontWeight, ListSizingBehavior, ScrollHandle, ScrollStrategy,
    SharedString, Size, StrikethroughStyle, StyledText, Task, TaskExt, UniformListScrollHandle,
    div, px, uniform_list,
};
use itertools::Itertools;
use language::CodeLabel;
use language::{Buffer, LanguageName, LanguageRegistry};
use lsp::{CompletionItemKind, CompletionItemTag};
use markdown::{CopyButtonVisibility, Markdown, MarkdownElement};
use multi_buffer::Anchor;
use ordered_float::OrderedFloat;
use project::lsp_store::CompletionDocumentation;
use project::{CodeAction, Completion, CompletionGroup, TaskSourceKind};
use project::{CompletionDisplayOptions, CompletionSource};
use task::DebugScenario;
use task::TaskContext;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::{
    cell::RefCell,
    cmp::{Reverse, min},
    iter,
    ops::Range,
    rc::Rc,
};
use task::ResolvedTask;
use ui::{
    Divider, ListItem, ListSubHeader, Popover, ScrollAxes, Scrollbars, Tooltip, WithScrollbar,
    prelude::*,
};
use util::ResultExt;

use crate::{
    CodeActionProvider, CompletionId, CompletionProvider, DisplayRow, Editor, EditorStyle,
    ResolvedTasks,
    actions::{ConfirmCodeAction, ConfirmCompletion},
    split_words, styled_runs_for_code_label,
};
use crate::{CodeActionSource, EditorSettings};
use collections::{HashSet, VecDeque};
use settings::{CompletionDetailAlignment, CompletionMenuItemKind, Settings, SnippetSortOrder};

pub const MENU_GAP: Pixels = px(4.);
pub const MENU_ASIDE_X_PADDING: Pixels = px(16.);
pub const MENU_ASIDE_MIN_WIDTH: Pixels = px(260.);
pub const MENU_ASIDE_MAX_WIDTH: Pixels = px(500.);
pub const COMPLETION_MENU_MIN_WIDTH: Pixels = px(280.);
pub const COMPLETION_MENU_MAX_WIDTH: Pixels = px(540.);
pub const CODE_ACTION_MENU_MIN_WIDTH: Pixels = px(220.);
pub const CODE_ACTION_MENU_MAX_WIDTH: Pixels = px(540.);

// Constants for the markdown cache. The purpose of this cache is to reduce flickering due to
// documentation not yet being parsed.
//
// The size of the cache is set to 16, which is roughly 3 times more than the number of items
// fetched around the current selection. This way documentation is more often ready for render when
// revisiting previous entries, such as when pressing backspace.
const MARKDOWN_CACHE_MAX_SIZE: usize = 16;
const MARKDOWN_CACHE_BEFORE_ITEMS: usize = 2;
const MARKDOWN_CACHE_AFTER_ITEMS: usize = 2;

// Number of items beyond the visible items to resolve documentation.
const RESOLVE_BEFORE_ITEMS: usize = 4;
const RESOLVE_AFTER_ITEMS: usize = 4;

#[derive(Clone, Debug)]
pub enum CompletionMenuEntry {
    Match(StringMatch),
    Divider,
    GroupHeader(SharedString),
}

impl CompletionMenuEntry {
    pub fn as_match(&self) -> Option<&StringMatch> {
        match self {
            CompletionMenuEntry::Match(m) => Some(m),
            CompletionMenuEntry::Divider | CompletionMenuEntry::GroupHeader(_) => None,
        }
    }

    pub fn is_selectable(&self) -> bool {
        matches!(self, CompletionMenuEntry::Match(_))
    }
}

pub enum CodeContextMenu {
    Completions(CompletionsMenu),
    CodeActions(CodeActionsMenu),
}

impl CodeContextMenu {
    pub fn select_first(
        &mut self,
        provider: Option<&dyn CompletionProvider>,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) -> bool {
        if self.visible() {
            match self {
                CodeContextMenu::Completions(menu) => menu.select_first(provider, window, cx),
                CodeContextMenu::CodeActions(menu) => menu.select_first(cx),
            }
            true
        } else {
            false
        }
    }

    pub fn select_prev(
        &mut self,
        provider: Option<&dyn CompletionProvider>,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) -> bool {
        if self.visible() {
            match self {
                CodeContextMenu::Completions(menu) => menu.select_prev(provider, window, cx),
                CodeContextMenu::CodeActions(menu) => menu.select_prev(cx),
            }
            true
        } else {
            false
        }
    }

    pub fn select_next(
        &mut self,
        provider: Option<&dyn CompletionProvider>,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) -> bool {
        if self.visible() {
            match self {
                CodeContextMenu::Completions(menu) => menu.select_next(provider, window, cx),
                CodeContextMenu::CodeActions(menu) => menu.select_next(cx),
            }
            true
        } else {
            false
        }
    }

    pub fn select_last(
        &mut self,
        provider: Option<&dyn CompletionProvider>,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) -> bool {
        if self.visible() {
            match self {
                CodeContextMenu::Completions(menu) => menu.select_last(provider, window, cx),
                CodeContextMenu::CodeActions(menu) => menu.select_last(cx),
            }
            true
        } else {
            false
        }
    }

    pub fn visible(&self) -> bool {
        match self {
            CodeContextMenu::Completions(menu) => menu.visible(),
            CodeContextMenu::CodeActions(menu) => menu.visible(),
        }
    }

    pub fn origin(&self) -> ContextMenuOrigin {
        match self {
            CodeContextMenu::Completions(menu) => menu.origin(),
            CodeContextMenu::CodeActions(menu) => menu.origin(),
        }
    }

    pub fn render(
        &self,
        style: &EditorStyle,
        max_height_in_lines: u32,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) -> AnyElement {
        match self {
            CodeContextMenu::Completions(menu) => {
                menu.render(style, max_height_in_lines, window, cx)
            }
            CodeContextMenu::CodeActions(menu) => {
                menu.render(style, max_height_in_lines, window, cx)
            }
        }
    }

    pub fn render_aside(
        &mut self,
        max_size: Size<Pixels>,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) -> Option<AnyElement> {
        match self {
            CodeContextMenu::Completions(menu) => menu.render_aside(max_size, window, cx),
            CodeContextMenu::CodeActions(menu) => menu.render_aside(max_size, window, cx),
        }
    }

    pub fn focused(&self, window: &mut Window, cx: &mut Context<Editor>) -> bool {
        match self {
            CodeContextMenu::Completions(completions_menu) => completions_menu
                .get_or_create_entry_markdown(completions_menu.selected_item, cx)
                .as_ref()
                .is_some_and(|markdown| markdown.focus_handle(cx).contains_focused(window, cx)),
            CodeContextMenu::CodeActions(_) => false,
        }
    }

    pub fn scroll_aside(
        &mut self,
        scroll_amount: ScrollAmount,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) {
        match self {
            CodeContextMenu::Completions(completions_menu) => {
                completions_menu.scroll_aside(scroll_amount, window, cx)
            }
            CodeContextMenu::CodeActions(_) => (),
        }
    }

    pub fn primary_scroll_handle(&self) -> UniformListScrollHandle {
        match self {
            CodeContextMenu::Completions(menu) => menu.scroll_handle.clone(),
            CodeContextMenu::CodeActions(menu) => menu.scroll_handle.clone(),
        }
    }
}

pub enum ContextMenuOrigin {
    Cursor,
    GutterIndicator(DisplayRow),
    QuickActionBar,
}

pub struct CompletionsMenu {
    pub id: CompletionId,
    pub source: CompletionsMenuSource,
    sort_completions: bool,
    pub initial_position: Anchor,
    pub initial_query: Option<Arc<String>>,
    pub is_incomplete: bool,
    pub buffer: Entity<Buffer>,
    pub completions: Rc<RefCell<Box<[Completion]>>>,
    /// String match candidate for each completion, grouped by `match_start`.
    match_candidates: Arc<[(Option<text::Anchor>, Vec<StringMatchCandidate>)]>,
    /// Entries displayed in the menu, which is a filtered and sorted subset of `match_candidates`.
    pub entries: Rc<RefCell<Box<[CompletionMenuEntry]>>>,
    pub selected_item: usize,
    filter_task: Task<()>,
    cancel_filter: Arc<AtomicBool>,
    scroll_handle: UniformListScrollHandle,
    // The `ScrollHandle` used on the Markdown documentation rendered on the
    // side of the completions menu.
    pub scroll_handle_aside: ScrollHandle,
    resolve_completions: bool,
    show_completion_documentation: bool,
    last_rendered_range: Rc<RefCell<Option<Range<usize>>>>,
    markdown_cache: Rc<RefCell<VecDeque<(MarkdownCacheKey, Entity<Markdown>)>>>,
    language_registry: Option<Arc<LanguageRegistry>>,
    language: Option<LanguageName>,
    display_options: CompletionDisplayOptions,
    snippet_sort_order: SnippetSortOrder,
}

impl CompletionsMenu {
    pub fn visible(&self) -> bool {
        !self.entries.borrow().is_empty()
    }

    pub fn origin(&self) -> ContextMenuOrigin {
        ContextMenuOrigin::Cursor
    }
}

#[derive(Clone, Debug, PartialEq)]
enum MarkdownCacheKey {
    ForCandidate {
        candidate_id: usize,
    },
    ForCompletionMatch {
        new_text: String,
        markdown_source: SharedString,
    },
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CompletionsMenuSource {
    /// Show all completions (words, snippets, LSP)
    Normal,
    /// Show only snippets (not words or LSP)
    ///
    /// Used after typing a non-word character
    SnippetsOnly,
    /// Tab stops within a snippet that have a predefined finite set of choices
    SnippetChoices,
    /// Show only words (not snippets or LSP)
    ///
    /// Used when word completions are explicitly triggered
    Words { ignore_threshold: bool },
}

// TODO: There should really be a wrapper around fuzzy match tasks that does this.
impl Drop for CompletionsMenu {
    fn drop(&mut self) {
        self.cancel_filter.store(true, Ordering::Relaxed);
    }
}

#[derive(Default)]
struct CompletionMenuScrollBarSetting;

impl ui::scrollbars::ScrollbarVisibility for CompletionMenuScrollBarSetting {
    fn visibility(&self, cx: &App) -> ui::scrollbars::ShowScrollbar {
        EditorSettings::get_global(cx).completion_menu_scrollbar
    }
}

mod code_actions;
mod completion_constructors;
mod completion_filtering;
mod completion_kinds;
mod completion_navigation;
mod completion_rendering;
mod completion_resolving;

pub use code_actions::{AvailableCodeAction, CodeActionContents, CodeActionsItem, CodeActionsMenu};
use completion_kinds::render_completion_kind_letter;
