#[path = "markdown/any_div.rs"]
mod any_div;
#[path = "markdown/builder.rs"]
mod builder;
#[path = "markdown/builder_output.rs"]
mod builder_output;
#[path = "markdown/clipboard.rs"]
mod clipboard;
#[path = "markdown/code_blocks.rs"]
mod code_blocks;
#[path = "markdown/element.rs"]
mod element;
mod escaping;
pub mod html;
mod mermaid;
#[path = "markdown/parse.rs"]
mod parse;
pub mod parser;
mod path_range;
#[path = "markdown/rendered_line.rs"]
mod rendered_line;
#[path = "markdown/rendered_text.rs"]
mod rendered_text;
#[path = "markdown/selection.rs"]
mod selection;
mod style;

use any_div::AnyDiv;
use builder::{MarkdownElementBuilder, MetadataCellStyle};
pub use element::{AutoscrollBehavior, MarkdownElement};
use escaping::MarkdownEscaper;
use rendered_line::{RenderedLine, SourceMapping};
use rendered_text::{RenderedFootnoteRef, RenderedLink, RenderedMarkdown, RenderedText};
use selection::{SelectMode, Selection};

use base64::Engine as _;
use futures::FutureExt as _;
use language::LanguageName;

use log::Level;
use mermaid::{
    MermaidState, ParsedMarkdownMermaidDiagram, extract_mermaid_diagrams, render_mermaid_diagram,
};
pub use path_range::{LineCol, PathWithRange};
use settings::Settings as _;
pub use style::{BlockQuoteKindColors, HeadingLevelStyles, MarkdownFont, MarkdownStyle};
use theme_settings::ThemeSettings;
use util::maybe;

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::ops::Range;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use collections::{HashMap, HashSet};
use gpui::{
    AnyElement, App, Bounds, ClipboardItem, Entity, FocusHandle, Focusable, Hsla, Image,
    ImageFormat, ImageSource, ScrollHandle, Stateful, StyleRefinement, StyledImage, Subscription,
    Task, TextAlign, TextStyle, TextStyleRefinement, actions,
};
use language::{Language, LanguageRegistry};
use parser::CodeBlockMetadata;
use parser::{
    MarkdownEvent, MarkdownTag, MarkdownTagEnd, ParsedMetadataBlock, parse_links_only,
    parse_markdown_with_options,
};
use pulldown_cmark::BlockQuoteKind;
use sum_tree::TreeMap;
use ui::{Tooltip, prelude::*};
use util::ResultExt;

use crate::parser::CodeBlockKind;

/// A callback function that can be used to customize the style of links based on the destination URL.
/// If the callback returns `None`, the default link style will be used.
pub type CodeSpanLinkCallback = Arc<dyn Fn(&str, &App) -> Option<SharedString> + 'static>;
type SourceClickCallback = Box<dyn Fn(usize, usize, &mut Window, &mut App) -> bool>;
type CheckboxToggleCallback = Rc<dyn Fn(Range<usize>, bool, &mut Window, &mut App)>;

pub struct Markdown {
    source: SharedString,
    selection: Selection,
    pressed_link: Option<RenderedLink>,
    pressed_footnote_ref: Option<RenderedFootnoteRef>,
    autoscroll_request: Option<usize>,
    active_root_block: Option<usize>,
    parsed_markdown: ParsedMarkdown,
    images_by_source_offset: HashMap<usize, Arc<Image>>,
    should_reparse: bool,
    pending_parse: Option<Task<()>>,
    focus_handle: FocusHandle,
    language_registry: Option<Arc<LanguageRegistry>>,
    fallback_code_block_language: Option<LanguageName>,
    options: MarkdownOptions,
    mermaid_state: MermaidState,
    _mermaid_theme_subscription: Option<Subscription>,
    mermaid_showing_code: HashSet<usize>,
    copied_code_blocks: HashSet<ElementId>,
    wrapped_code_blocks: HashSet<usize>,
    code_block_scroll_handles: BTreeMap<usize, ScrollHandle>,
    context_menu_link: Option<SharedString>,
    context_menu_selected_text: Option<SharedString>,
    context_menu_selected_markdown: Option<SharedString>,
    search_highlights: Vec<Range<usize>>,
    active_search_highlight: Option<usize>,
}

#[derive(Clone, Copy, Default)]
pub struct MarkdownOptions {
    pub parse_links_only: bool,
    pub parse_html: bool,
    pub render_mermaid_diagrams: bool,
    pub parse_heading_slugs: bool,
    pub render_metadata_blocks: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CopyButtonVisibility {
    Hidden,
    AlwaysVisible,
    VisibleOnHover,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WrapButtonVisibility {
    Hidden,
    AlwaysVisible,
    VisibleOnHover,
}

pub enum CodeBlockRenderer {
    Default {
        copy_button_visibility: CopyButtonVisibility,
        wrap_button_visibility: WrapButtonVisibility,
        border: bool,
    },
    Custom {
        render: CodeBlockRenderFn,
        /// A function that can modify the parent container after the code block
        /// content has been appended as a child element.
        transform: Option<CodeBlockTransformFn>,
    },
}

pub type CodeBlockRenderFn = Arc<
    dyn Fn(
        &CodeBlockKind,
        &ParsedMarkdown,
        Range<usize>,
        CodeBlockMetadata,
        &mut Window,
        &App,
    ) -> Div,
>;

pub type CodeBlockTransformFn =
    Arc<dyn Fn(AnyDiv, Range<usize>, CodeBlockMetadata, &mut Window, &App) -> AnyDiv>;

actions!(
    markdown,
    [
        /// Copies the selected text to the clipboard.
        Copy,
        /// Copies the selected text as markdown to the clipboard.
        CopyAsMarkdown
    ]
);

impl Markdown {
    pub fn new(
        source: SharedString,
        language_registry: Option<Arc<LanguageRegistry>>,
        fallback_code_block_language: Option<LanguageName>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new_with_options(
            source,
            language_registry,
            fallback_code_block_language,
            MarkdownOptions::default(),
            cx,
        )
    }

    pub fn new_with_options(
        source: SharedString,
        language_registry: Option<Arc<LanguageRegistry>>,
        fallback_code_block_language: Option<LanguageName>,
        options: MarkdownOptions,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();

        let theme_subscription = if options.render_mermaid_diagrams {
            Some(
                cx.observe_global::<theme::GlobalTheme>(|this: &mut Self, cx| {
                    this.invalidate_mermaid_cache(cx);
                }),
            )
        } else {
            None
        };
        let mut this = Self {
            source,
            selection: Selection::default(),
            pressed_link: None,
            pressed_footnote_ref: None,
            autoscroll_request: None,
            active_root_block: None,
            should_reparse: false,
            images_by_source_offset: Default::default(),
            parsed_markdown: ParsedMarkdown::default(),
            pending_parse: None,
            focus_handle,
            language_registry,
            fallback_code_block_language,
            options,
            mermaid_state: MermaidState::default(),
            _mermaid_theme_subscription: theme_subscription,
            mermaid_showing_code: HashSet::default(),
            copied_code_blocks: HashSet::default(),
            wrapped_code_blocks: HashSet::default(),
            code_block_scroll_handles: BTreeMap::default(),
            context_menu_link: None,
            context_menu_selected_text: None,
            context_menu_selected_markdown: None,
            search_highlights: Vec::new(),
            active_search_highlight: None,
        };
        this.parse(cx);
        this
    }

    pub fn new_text(source: SharedString, cx: &mut Context<Self>) -> Self {
        Self::new_with_options(
            source,
            None,
            None,
            MarkdownOptions {
                parse_links_only: true,
                ..Default::default()
            },
            cx,
        )
    }

    pub fn set_active_root_for_source_index(
        &mut self,
        source_index: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        let active_root_block =
            source_index.and_then(|index| self.parsed_markdown.root_block_for_source_index(index));
        if self.active_root_block == active_root_block {
            return;
        }

        self.active_root_block = active_root_block;
        cx.notify();
    }

    pub fn reset(&mut self, source: SharedString, cx: &mut Context<Self>) {
        if &source == self.source() {
            return;
        }
        self.source = source;
        self.selection = Selection::default();
        self.autoscroll_request = None;
        self.pending_parse = None;
        self.should_reparse = false;
        self.search_highlights.clear();
        self.active_search_highlight = None;
        // Don't clear parsed_markdown here - keep existing content visible until new parse completes
        self.parse(cx);
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn parsed_markdown(&self) -> &ParsedMarkdown {
        &self.parsed_markdown
    }

    pub fn escape(s: &str) -> Cow<'_, str> {
        let output_len: usize = {
            let mut escaper = MarkdownEscaper::new();
            s.chars().map(|c| escaper.next(c).output_len(c)).sum()
        };

        if output_len == s.len() {
            return s.into();
        }

        let mut escaper = MarkdownEscaper::new();
        let mut output = String::with_capacity(output_len);
        for c in s.chars() {
            escaper.next(c).write_to(c, &mut output);
        }
        output.into()
    }

    pub fn selected_text(&self) -> Option<String> {
        if self.selection.end <= self.selection.start {
            None
        } else {
            Some(self.source[self.selection.start..self.selection.end].to_string())
        }
    }

    pub fn set_search_highlights(
        &mut self,
        highlights: Vec<Range<usize>>,
        active: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        debug_assert!(
            highlights
                .windows(2)
                .all(|ranges| (ranges[0].start, ranges[0].end) <= (ranges[1].start, ranges[1].end))
        );
        self.search_highlights = highlights;
        self.active_search_highlight =
            active.filter(|active| *active < self.search_highlights.len());
        cx.notify();
    }

    pub fn clear_search_highlights(&mut self, cx: &mut Context<Self>) {
        if !self.search_highlights.is_empty() || self.active_search_highlight.is_some() {
            self.search_highlights.clear();
            self.active_search_highlight = None;
            cx.notify();
        }
    }

    pub fn set_active_search_highlight(&mut self, active: Option<usize>, cx: &mut Context<Self>) {
        let active = active.filter(|active| *active < self.search_highlights.len());
        if self.active_search_highlight != active {
            self.active_search_highlight = active;
            cx.notify();
        }
    }

    pub fn search_highlights(&self) -> &[Range<usize>] {
        &self.search_highlights
    }

    pub fn active_search_highlight(&self) -> Option<usize> {
        self.active_search_highlight
    }

    /// Returns the URL of the link that was most recently right-clicked, if any.
    /// This is set during a right-click mouse-down event and can be read by parent
    /// views to include a "Copy Link" item in their context menus.
    pub fn context_menu_link(&self) -> Option<&SharedString> {
        self.context_menu_link.as_ref()
    }

    /// Returns the rendered (plain) text that was selected when the most recent
    /// context menu invocation happened.
    pub fn context_menu_selected_text(&self) -> Option<&SharedString> {
        self.context_menu_selected_text.as_ref()
    }

    /// Returns the raw markdown source that was selected when the most recent
    /// context menu invocation happened.
    pub fn context_menu_selected_markdown(&self) -> Option<&SharedString> {
        self.context_menu_selected_markdown.as_ref()
    }
}

impl Focusable for Markdown {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

#[derive(Debug, Clone, Default)]
pub struct ParsedMarkdown {
    pub source: SharedString,
    pub events: Arc<[(Range<usize>, MarkdownEvent)]>,
    pub languages_by_name: TreeMap<SharedString, Arc<Language>>,
    pub languages_by_path: TreeMap<Arc<str>, Arc<Language>>,
    pub root_block_starts: Arc<[usize]>,
    pub(crate) html_blocks: BTreeMap<usize, html::html_parser::ParsedHtmlBlock>,
    pub(crate) metadata_blocks: BTreeMap<usize, ParsedMetadataBlock>,
    pub(crate) mermaid_diagrams: BTreeMap<usize, ParsedMarkdownMermaidDiagram>,
    pub heading_slugs: HashMap<SharedString, usize>,
    pub footnote_definitions: HashMap<SharedString, usize>,
}

impl ParsedMarkdown {
    pub fn source(&self) -> &SharedString {
        &self.source
    }

    pub fn events(&self) -> &Arc<[(Range<usize>, MarkdownEvent)]> {
        &self.events
    }

    pub fn root_block_starts(&self) -> &Arc<[usize]> {
        &self.root_block_starts
    }

    pub fn root_block_for_source_index(&self, source_index: usize) -> Option<usize> {
        if self.root_block_starts.is_empty() {
            return None;
        }

        let partition = self
            .root_block_starts
            .partition_point(|block_start| *block_start <= source_index);

        Some(partition.saturating_sub(1))
    }
}

#[cfg(test)]
#[path = "markdown/tests.rs"]
mod tests;
