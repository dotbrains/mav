use super::*;

#[derive(Debug, Default)]
pub enum PrepareRenameResponse {
    Success(Range<Anchor>),
    OnlyUnpreparedRenameSupported,
    #[default]
    InvalidPosition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InlayId {
    EditPrediction(usize),
    DebuggerValue(usize),
    // LSP
    Hint(usize),
    Color(usize),
    ReplResult(usize),
}

impl InlayId {
    pub fn id(&self) -> usize {
        match self {
            Self::EditPrediction(id) => *id,
            Self::DebuggerValue(id) => *id,
            Self::Hint(id) => *id,
            Self::Color(id) => *id,
            Self::ReplResult(id) => *id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlayHint {
    pub position: language::Anchor,
    pub label: InlayHintLabel,
    pub kind: Option<InlayHintKind>,
    pub padding_left: bool,
    pub padding_right: bool,
    pub tooltip: Option<InlayHintTooltip>,
    pub resolve_state: ResolveState,
}

/// The user's intent behind a given completion confirmation.
#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy)]
pub enum CompletionIntent {
    /// The user intends to 'commit' this result, if possible.
    /// Completion confirmations should run side effects.
    ///
    /// For LSP completions, will respect the setting `completions.lsp_insert_mode`.
    Complete,
    /// Similar to [Self::Complete], but behaves like `lsp_insert_mode` is set to `insert`.
    CompleteWithInsert,
    /// Similar to [Self::Complete], but behaves like `lsp_insert_mode` is set to `replace`.
    CompleteWithReplace,
    /// The user intends to continue 'composing' this completion.
    /// Completion confirmations should not run side effects and
    /// let the user continue composing their action.
    Compose,
}

impl CompletionIntent {
    pub fn is_complete(&self) -> bool {
        self == &Self::Complete
    }

    pub fn is_compose(&self) -> bool {
        self == &Self::Compose
    }
}

/// Describes a visual group for a completion item in the menu.
/// When the group changes between consecutive completions, the menu inserts a divider.
/// If a label is provided, a non-selectable header row is also rendered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletionGroup {
    /// Identity of this group, used to detect transitions between consecutive items.
    pub key: SharedString,
    /// When set, a non-selectable header with this text is rendered below the divider.
    pub label: Option<SharedString>,
}

/// Similar to `CoreCompletion`, but with extra metadata attached.
#[derive(Clone)]
pub struct Completion {
    /// The range of text that will be replaced by this completion.
    pub replace_range: Range<Anchor>,
    /// The new text that will be inserted.
    pub new_text: String,
    /// A label for this completion that is shown in the menu.
    pub label: CodeLabel,
    /// The documentation for this completion.
    pub documentation: Option<CompletionDocumentation>,
    /// Completion data source which it was constructed from.
    pub source: CompletionSource,
    /// A path to an icon for this completion that is shown in the menu.
    pub icon_path: Option<SharedString>,
    /// An optional color to tint this completion's icon with in the menu.
    /// When `None`, the menu's default muted color is used.
    pub icon_color: Option<Hsla>,
    /// Text starting here and ending at the cursor will be used as the query for filtering this completion.
    ///
    /// If None, the start of the surrounding word is used.
    pub match_start: Option<text::Anchor>,
    /// Key used for de-duplicating snippets. If None, always considered unique.
    pub snippet_deduplication_key: Option<(usize, usize)>,
    /// Whether to adjust indentation (the default) or not.
    pub insert_text_mode: Option<InsertTextMode>,
    /// An optional callback to invoke when this completion is confirmed.
    /// Returns whether new completions should be retriggered after the current one.
    /// If `true` is returned, the editor will show a new completion menu after this completion is confirmed.
    /// if no confirmation is provided or `false` is returned, the completion will be committed.
    pub confirm: Option<Arc<dyn Send + Sync + Fn(CompletionIntent, &mut Window, &mut App) -> bool>>,
    /// An optional group for this completion. When the group changes between consecutive
    /// items, the completion menu inserts a divider. If the group also carries a label,
    /// a non-selectable header row is rendered below the divider.
    pub group: Option<CompletionGroup>,
}

#[derive(Debug, Clone)]
pub enum CompletionSource {
    Lsp {
        /// The alternate `insert` range, if provided by the LSP server.
        insert_range: Option<Range<Anchor>>,
        /// The id of the language server that produced this completion.
        server_id: LanguageServerId,
        /// The raw completion provided by the language server.
        lsp_completion: Box<lsp::CompletionItem>,
        /// A set of defaults for this completion item.
        lsp_defaults: Option<Arc<lsp::CompletionListItemDefaults>>,
        /// Whether this completion has been resolved, to ensure it happens once per completion.
        resolved: bool,
    },
    Dap {
        /// The sort text for this completion.
        sort_text: String,
    },
    Custom,
    BufferWord {
        word_range: Range<Anchor>,
        resolved: bool,
    },
}

impl CompletionSource {
    pub fn server_id(&self) -> Option<LanguageServerId> {
        if let CompletionSource::Lsp { server_id, .. } = self {
            Some(*server_id)
        } else {
            None
        }
    }

    pub fn lsp_completion(&self, apply_defaults: bool) -> Option<Cow<'_, lsp::CompletionItem>> {
        if let Self::Lsp {
            lsp_completion,
            lsp_defaults,
            ..
        } = self
        {
            if apply_defaults && let Some(lsp_defaults) = lsp_defaults {
                let mut completion_with_defaults = *lsp_completion.clone();
                let default_commit_characters = lsp_defaults.commit_characters.as_ref();
                let default_edit_range = lsp_defaults.edit_range.as_ref();
                let default_insert_text_format = lsp_defaults.insert_text_format.as_ref();
                let default_insert_text_mode = lsp_defaults.insert_text_mode.as_ref();

                if default_commit_characters.is_some()
                    || default_edit_range.is_some()
                    || default_insert_text_format.is_some()
                    || default_insert_text_mode.is_some()
                {
                    if completion_with_defaults.commit_characters.is_none()
                        && default_commit_characters.is_some()
                    {
                        completion_with_defaults.commit_characters =
                            default_commit_characters.cloned()
                    }
                    if completion_with_defaults.text_edit.is_none() {
                        match default_edit_range {
                            Some(lsp::CompletionListItemDefaultsEditRange::Range(range)) => {
                                completion_with_defaults.text_edit =
                                    Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
                                        range: *range,
                                        new_text: completion_with_defaults.label.clone(),
                                    }))
                            }
                            Some(lsp::CompletionListItemDefaultsEditRange::InsertAndReplace {
                                insert,
                                replace,
                            }) => {
                                completion_with_defaults.text_edit =
                                    Some(lsp::CompletionTextEdit::InsertAndReplace(
                                        lsp::InsertReplaceEdit {
                                            new_text: completion_with_defaults.label.clone(),
                                            insert: *insert,
                                            replace: *replace,
                                        },
                                    ))
                            }
                            None => {}
                        }
                    }
                    if completion_with_defaults.insert_text_format.is_none()
                        && default_insert_text_format.is_some()
                    {
                        completion_with_defaults.insert_text_format =
                            default_insert_text_format.cloned()
                    }
                    if completion_with_defaults.insert_text_mode.is_none()
                        && default_insert_text_mode.is_some()
                    {
                        completion_with_defaults.insert_text_mode =
                            default_insert_text_mode.cloned()
                    }
                }
                return Some(Cow::Owned(completion_with_defaults));
            }
            Some(Cow::Borrowed(lsp_completion))
        } else {
            None
        }
    }
}

impl std::fmt::Debug for Completion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Completion")
            .field("replace_range", &self.replace_range)
            .field("new_text", &self.new_text)
            .field("label", &self.label)
            .field("documentation", &self.documentation)
            .field("source", &self.source)
            .finish()
    }
}

/// Response from a source of completions.
pub struct CompletionResponse {
    pub completions: Vec<Completion>,
    pub display_options: CompletionDisplayOptions,
    /// When false, indicates that the list is complete and does not need to be re-queried if it
    /// can be filtered instead.
    pub is_incomplete: bool,
}

#[derive(Default)]
pub struct CompletionDisplayOptions {
    pub dynamic_width: bool,
}

impl CompletionDisplayOptions {
    pub fn merge(&mut self, other: &CompletionDisplayOptions) {
        self.dynamic_width = self.dynamic_width && other.dynamic_width;
    }
}

/// Response from language server completion request.
#[derive(Clone, Debug, Default)]
pub(crate) struct CoreCompletionResponse {
    pub completions: Vec<CoreCompletion>,
    /// When false, indicates that the list is complete and does not need to be re-queried if it
    /// can be filtered instead.
    pub is_incomplete: bool,
}

/// A generic completion that can come from different sources.
#[derive(Clone, Debug)]
pub(crate) struct CoreCompletion {
    pub(crate) replace_range: Range<Anchor>,
    pub(crate) new_text: String,
    pub(crate) source: CompletionSource,
}

/// A code action provided by a language server.
#[derive(Clone, Debug, PartialEq)]
pub struct CodeAction {
    /// The id of the language server that produced this code action.
    pub server_id: LanguageServerId,
    /// The range of the buffer where this code action is applicable.
    pub range: Range<Anchor>,
    /// The raw code action provided by the language server.
    /// Can be either an action or a command.
    pub lsp_action: LspAction,
    /// Whether the action needs to be resolved using the language server.
    pub resolved: bool,
}

/// An action sent back by a language server.
#[derive(Clone, Debug, PartialEq)]
pub enum LspAction {
    /// An action with the full data, may have a command or may not.
    /// May require resolving.
    Action(Box<lsp::CodeAction>),
    /// A command data to run as an action.
    Command(lsp::Command),
    /// A code lens data to run as an action.
    CodeLens(lsp::CodeLens),
}

impl LspAction {
    pub fn title(&self) -> &str {
        match self {
            Self::Action(action) => &action.title,
            Self::Command(command) => &command.title,
            Self::CodeLens(lens) => lens
                .command
                .as_ref()
                .map(|command| command.title.as_str())
                .unwrap_or("Unknown command"),
        }
    }

    pub fn action_kind(&self) -> Option<lsp::CodeActionKind> {
        match self {
            Self::Action(action) => action.kind.clone(),
            Self::Command(_) => Some(lsp::CodeActionKind::new("command")),
            Self::CodeLens(_) => Some(lsp::CodeActionKind::new("code lens")),
        }
    }

    pub fn edit(&self) -> Option<&lsp::WorkspaceEdit> {
        match self {
            Self::Action(action) => action.edit.as_ref(),
            Self::Command(_) => None,
            Self::CodeLens(_) => None,
        }
    }

    pub fn command(&self) -> Option<&lsp::Command> {
        match self {
            Self::Action(action) => action.command.as_ref(),
            Self::Command(command) => Some(command),
            Self::CodeLens(lens) => lens.command.as_ref(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveState {
    Resolved,
    CanResolve(LanguageServerId, Option<lsp::LSPAny>),
    Resolving,
}
impl InlayHint {
    pub fn text(&self) -> Rope {
        match &self.label {
            InlayHintLabel::String(s) => Rope::from(s),
            InlayHintLabel::LabelParts(parts) => parts.iter().map(|part| &*part.value).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlayHintLabel {
    String(String),
    LabelParts(Vec<InlayHintLabelPart>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlayHintLabelPart {
    pub value: String,
    pub tooltip: Option<InlayHintLabelPartTooltip>,
    pub location: Option<(LanguageServerId, lsp::Location)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlayHintTooltip {
    String(String),
    MarkupContent(MarkupContent),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlayHintLabelPartTooltip {
    String(String),
    MarkupContent(MarkupContent),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkupContent {
    pub kind: HoverBlockKind,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LocationLink {
    pub origin: Option<Location>,
    pub target: Location,
}

#[derive(Debug)]
pub struct DocumentHighlight {
    pub range: Range<language::Anchor>,
    pub kind: DocumentHighlightKind,
}

#[derive(Clone, Debug)]
pub struct Symbol {
    pub language_server_name: LanguageServerName,
    pub source_worktree_id: WorktreeId,
    pub source_language_server_id: LanguageServerId,
    pub path: SymbolLocation,
    pub label: CodeLabel,
    pub name: String,
    pub kind: lsp::SymbolKind,
    pub range: Range<Unclipped<PointUtf16>>,
    pub container_name: Option<String>,
}

#[derive(Clone, Debug)]
pub struct DocumentSymbol {
    pub name: String,
    pub kind: lsp::SymbolKind,
    pub range: Range<Unclipped<PointUtf16>>,
    pub selection_range: Range<Unclipped<PointUtf16>>,
    pub children: Vec<DocumentSymbol>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HoverBlock {
    pub text: String,
    pub kind: HoverBlockKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HoverBlockKind {
    PlainText,
    Markdown,
    Code { language: String },
}

#[derive(Debug, Clone)]
pub struct Hover {
    pub contents: Vec<HoverBlock>,
    pub range: Option<Range<language::Anchor>>,
    pub language: Option<Arc<Language>>,
}

impl Hover {
    pub fn is_empty(&self) -> bool {
        self.contents.iter().all(|block| block.text.is_empty())
    }
}
