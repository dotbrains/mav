#![allow(rustdoc::private_intra_doc_links)]
//! This is the place where everything editor-related is stored (data-wise) and displayed (ui-wise).
//! The main point of interest in this crate is [`Editor`] type, which is used in every other Mav part as a user input element.
//! It comes in different flavors: single line, multiline and a fixed height one.
//!
//! Editor contains of multiple large submodules:
//! * [`element`] — the place where all rendering happens
//! * [`display_map`] - chunks up text in the editor into the logical blocks, establishes coordinates and mapping between each of them.
//!   Contains all metadata related to text transformations (folds, fake inlay text insertions, soft wraps, tab markup, etc.).
//!
//! All other submodules and structs are mostly concerned with holding editor data about the way it displays current buffer region(s).
//!
//! If you're looking to improve Vim mode, you should check out Vim crate that wraps Editor and overrides its behavior.
pub mod actions;
pub mod blink_manager;
mod bracket_colorization;
mod clangd_ext;
pub mod code_context_menus;
mod code_lens;
pub mod display_map;
mod document_colors;
mod document_links;
mod document_symbols;
mod editor_settings;
mod element;
mod fold;
#[path = "editor/fold_persistence.rs"]
mod fold_persistence;
mod folding_ranges;
mod git;
mod highlight_matching_bracket;
#[path = "editor/highlights.rs"]
mod highlights;
pub mod hover_links;
pub mod hover_popover;
mod indent_guides;
mod inlays;
pub mod items;
mod jsx_tag_auto_close;
mod linked_editing_ranges;
mod lsp_ext;
mod mouse_context_menu;
#[path = "editor/mouse_modifiers.rs"]
mod mouse_modifiers;
pub mod movement;
mod persistence;
mod runnables;
mod rust_analyzer_ext;
pub mod scroll;
mod selections_collection;
pub mod semantic_tokens;
mod split;
pub mod split_editor_view;

#[path = "editor/addon.rs"]
mod addon;
mod bookmarks;
#[cfg(test)]
mod code_completion_tests;
#[cfg(test)]
mod edit_prediction_tests;
#[cfg(test)]
mod editor_block_comment_tests;
#[cfg(test)]
mod editor_tests;
mod signature_help;
#[cfg(any(test, feature = "test-support"))]
pub mod test;

#[path = "editor/bookmark_gutter.rs"]
mod bookmark_gutter;
#[path = "editor/breakpoint_actions.rs"]
mod breakpoint_actions;
#[path = "editor/breakpoint_gutter.rs"]
mod breakpoint_gutter;
#[path = "editor/buffer_events.rs"]
mod buffer_events;
#[path = "editor/change_list.rs"]
mod change_list;
mod clipboard;
mod code_actions;
#[path = "editor/code_label_styles.rs"]
mod code_label_styles;
mod completions;
mod config;
#[path = "editor/constants.rs"]
mod constants;
#[path = "editor/context_menu.rs"]
mod context_menu;
#[path = "editor/core_types.rs"]
mod core_types;
mod diagnostics;
#[path = "editor/display_lifecycle.rs"]
mod display_lifecycle;
#[path = "editor/duplicate_actions.rs"]
mod duplicate_actions;
#[path = "editor/edit_actions.rs"]
mod edit_actions;
mod edit_prediction;
#[path = "editor/erased_editor.rs"]
mod erased_editor;
#[path = "editor/events.rs"]
mod events;
#[path = "editor/excerpts.rs"]
mod excerpts;
#[path = "editor/file_actions.rs"]
mod file_actions;
#[path = "editor/focus_actions.rs"]
mod focus_actions;
#[path = "editor/format_actions.rs"]
mod format_actions;
#[path = "editor/gutter_context_menu.rs"]
mod gutter_context_menu;
#[path = "editor/gutter_hover.rs"]
mod gutter_hover;
#[path = "editor/helpers.rs"]
mod helpers;
#[path = "editor/history_actions.rs"]
mod history_actions;
#[path = "editor/indentation_actions.rs"]
mod indentation_actions;
#[path = "editor/inline_values.rs"]
mod inline_values;
mod input;
#[path = "editor/line_edit_actions.rs"]
mod line_edit_actions;
#[path = "editor/line_manipulation.rs"]
mod line_manipulation;
#[path = "editor/line_ordering_actions.rs"]
mod line_ordering_actions;
#[path = "editor/lsp_lifecycle.rs"]
mod lsp_lifecycle;
mod markdown_actions;
#[path = "editor/metadata_restore.rs"]
mod metadata_restore;
#[path = "editor/minimap.rs"]
mod minimap;
#[path = "editor/misc_actions.rs"]
mod misc_actions;
#[path = "editor/move_line_actions.rs"]
mod move_line_actions;
mod navigation;
#[path = "editor/navigation_overlay.rs"]
mod navigation_overlay;
#[path = "editor/navigation_types.rs"]
mod navigation_types;
#[path = "editor/popup_navigation.rs"]
mod popup_navigation;
#[path = "editor/prompt_editor.rs"]
mod prompt_editor;
#[path = "editor/providers.rs"]
mod providers;
#[path = "editor/remote_selection.rs"]
mod remote_selection;
#[path = "editor/rename_actions.rs"]
mod rename_actions;
mod rewrap;
#[path = "editor/rotate_selection_actions.rs"]
mod rotate_selection_actions;
#[path = "editor/row_ext.rs"]
mod row_ext;
#[path = "editor/row_highlights.rs"]
mod row_highlights;
#[path = "editor/run_indicators.rs"]
mod run_indicators;
#[path = "editor/scrollbar_marker_state.rs"]
mod scrollbar_marker_state;
mod selection;
#[path = "editor/selection_alignment.rs"]
mod selection_alignment;
#[path = "editor/selection_drag_actions.rs"]
mod selection_drag_actions;
#[path = "editor/selection_ext.rs"]
mod selection_ext;
#[path = "editor/selection_history.rs"]
mod selection_history;
#[path = "editor/selection_state.rs"]
mod selection_state;
#[path = "editor/settings_refresh.rs"]
mod settings_refresh;
#[path = "editor/snapshot.rs"]
mod snapshot;
#[path = "editor/snippet_actions.rs"]
mod snippet_actions;
#[path = "editor/state_types.rs"]
mod state_types;
#[path = "editor/task_context.rs"]
mod task_context;
#[path = "editor/text_conversion.rs"]
mod text_conversion;
#[path = "editor/transpose_action.rs"]
mod transpose_action;
#[path = "editor/utilities.rs"]
mod utilities;
#[path = "editor/workspace_context.rs"]
mod workspace_context;
#[path = "editor/wrap_tag_actions.rs"]
mod wrap_tag_actions;

pub(crate) use actions::*;
pub use addon::Addon;
pub use change_list::ChangeList;
pub use clipboard::ClipboardSelection;
pub use code_actions::CodeActionProvider;
pub(crate) use code_actions::CodeActionsForSelection;
pub use code_label_styles::styled_runs_for_code_label;
use collections::TypeIdHashMap;
pub use completions::CompletionProvider;
#[cfg(test)]
pub(crate) use completions::snippet_candidate_suffixes;
pub(crate) use completions::split_words;
pub use constants::{
    BUFFER_HEADER_PADDING, CODE_ACTIONS_DEBOUNCE_TIMEOUT, FILE_HEADER_HEIGHT,
    LSP_REQUEST_DEBOUNCE_TIMEOUT, MULTI_BUFFER_EXCERPT_HEADER_HEIGHT,
    SELECTION_HIGHLIGHT_DEBOUNCE_TIMEOUT,
};
pub(crate) use constants::{
    CODE_ACTION_TIMEOUT, CURSORS_VISIBLE_FOR, EDIT_PREDICTION_KEY_CONTEXT, FORMAT_TIMEOUT,
    MAX_LINE_LEN, MINIMAP_FONT_SIZE, SCROLL_CENTER_TOP_BOTTOM_DEBOUNCE_TIMEOUT,
};
pub(crate) use core_types::{BreadcrumbsVisibility, CompletionId, EditorActionId};
pub use core_types::{
    BufferSerialization, ContextMenuOptions, ContextMenuPlacement, EditorMode, EditorStyle,
    MinimapVisibility, Navigated, SizingBehavior, SoftWrap, make_inlay_hints_style,
};
use diagnostics::{ActiveDiagnostic, GlobalDiagnosticRenderer, InlineDiagnostic};
pub use diagnostics::{DiagnosticRenderer, set_diagnostic_renderer};
pub use display_map::{
    ChunkRenderer, ChunkRendererContext, DisplayPoint, FoldPlaceholder, HighlightKey,
    NavigationOverlayKey, SemanticTokenHighlight,
};
pub use edit_prediction::make_suggestion_styles;
pub(crate) use edit_prediction::{
    EditDisplayMode, EditPrediction, EditPredictionPreview, EditPredictionSettings,
    EditPredictionState, MenuEditPredictionsPolicy, RegisteredEditPredictionDelegate,
};
#[cfg(test)]
pub(crate) use edit_prediction::{
    EditPredictionKeybindAction, EditPredictionKeybindSurface, edit_prediction_edit_text,
};
pub use edit_prediction_types::Direction;
pub use edit_prediction_types::EditPredictionRequestTrigger;
pub use editor_settings::{
    CompletionDetailAlignment, CompletionMenuItemKind, CurrentLineHighlight, DiffViewStyle,
    DocumentColorsRenderMode, EditorSettings, EditorSettingsScrollbarProxy, ScrollBeyondLastLine,
    ScrollbarAxes, SearchSettings, ShowMinimap, ui_scrollbar_settings_from_raw,
};
pub use element::{
    CursorLayout, EditorElement, HighlightedRange, HighlightedRangeLine, PointForPosition,
    render_breadcrumb_text,
};
use erased_editor::ErasedEditorImpl;
pub use events::EditorEvent;
pub(crate) use events::ReportEditorEvent;
pub use git::blame::BlameRenderer;
pub(crate) use git::{DiffHunkKey, StoredReviewComment};
use git::{
    DiffReviewDragState, DiffReviewOverlay, InlineBlamePopover, render_diff_hunk_controls,
    update_uncommitted_diff_for_buffer,
};
pub(crate) use git::{DisplayDiffHunk, PhantomDiffReviewIndicator};
pub use git::{RenderDiffHunkControlsFn, set_blame_renderer};
pub use hover_popover::hover_markdown_style;
pub use inlays::Inlay;
pub use items::MAX_TAB_TITLE_LEN;
pub use linked_editing_ranges::LinkedEdits;
pub use lsp::CompletionContext;
pub use lsp_ext::lsp_tasks;
pub use multi_buffer::{
    Anchor, AnchorRangeExt, BufferOffset, ExcerptRange, MBTextSummary, MultiBuffer,
    MultiBufferOffset, MultiBufferOffsetUtf16, MultiBufferSnapshot, PathKey, RowInfo, ToOffset,
    ToPoint,
};
pub use navigation_overlay::{NavigationOverlayLabel, NavigationTargetOverlay};
pub use navigation_types::{
    FormatTarget, GotoDefinitionKind, JumpData, MultibufferSelectionMode, RewrapOptions,
    collapse_multiline_range,
};
use prompt_editor::{BreakpointPromptEditAction, PromptEditor, PromptEditorCallback};
pub use providers::{CollaborationHub, SemanticsProvider};
pub(crate) use remote_selection::HoveredCursor;
pub use remote_selection::RemoteSelection;
pub(crate) use row_ext::RowRangeExt;
pub use row_ext::{RangeToAnchorExt, RowExt};
pub(crate) use scrollbar_marker_state::ScrollbarMarkerState;
pub(crate) use selection_ext::SelectionExt;
pub(crate) use selection_history::{
    DeferredSelectionEffectsState, SelectionHistory, SelectionHistoryEntry, SelectionHistoryMode,
};
pub use selection_state::RowHighlightOptions;
pub(crate) use selection_state::{
    AddSelectionsGroup, AddSelectionsState, AutocloseRegion, ColumnarSelectionState,
    GutterHoverButton, InvalidationStack, LineManipulationResult, RowHighlight, SelectNextState,
    SelectSyntaxNodeHistory, SelectSyntaxNodeScrollBehavior, SelectionDragState, SnippetState,
    consume_contiguous_rows,
};
pub use selection_state::{ColumnarMode, SelectMode, SelectPhase, SelectionEffects};
pub use snapshot::{EditorSnapshot, GutterDimensions, column_pixels};
pub use split::{SplittableEditor, ToggleSplitDiff};
pub use split_editor_view::SplitEditorView;
pub(crate) use state_types::{
    AccentData, CharacterDimensions, FocusedBlock, LineHighlight, NavigationData,
    NextScrollCursorCenterTopBottom, debounce_value,
};
pub use state_types::{RenameState, multibuffer_context_lines};
pub use text::Bias;

use ::git::{Blame, status::FileStatus};
use aho_corasick::{AhoCorasick, AhoCorasickBuilder, BuildError};
use anyhow::{Context as _, Result, anyhow, bail};
use blink_manager::BlinkManager;
use client::{Collaborator, ParticipantIndex, parse_mav_link};
use clock::ReplicaId;
use code_context_menus::{
    AvailableCodeAction, CodeActionContents, CodeActionsItem, CodeActionsMenu, CodeContextMenu,
    CompletionsMenu, ContextMenuOrigin,
};
use code_lens::CodeLensState;
use collections::{BTreeMap, HashMap, HashSet, VecDeque};
use dap::TelemetrySpawnLocation;
use display_map::*;
use document_colors::LspColorData;
use document_links::LspDocumentLinks;
use edit_prediction_types::{
    EditPredictionDelegate, EditPredictionDelegateHandle, EditPredictionDiscardReason,
    EditPredictionGranularity, SuggestionDisplayType,
};
use editor_settings::{GoToDefinitionFallback, Minimap as MinimapSettings};
use element::{LineWithInvisibles, PositionMap, layout_line};
use futures::{
    FutureExt,
    future::{self, Shared},
};
use fuzzy::{StringMatch, StringMatchCandidate};
use git::blame::{GitBlame, GlobalBlameRenderer};
use gpui::{
    Action, Animation, AnimationExt, AnyElement, App, AppContext, AsyncWindowContext,
    AvailableSpace, Background, Bounds, ClickEvent, ClipboardEntry, ClipboardItem, Context,
    DispatchPhase, Edges, Entity, EntityId, EntityInputHandler, EventEmitter, FocusHandle,
    FocusOutEvent, Focusable, FontId, FontStyle, FontWeight, Global, HighlightStyle, Hsla,
    KeyContext, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, PaintQuad, ParentElement,
    Pixels, PressureStage, Render, ScrollHandle, SharedString, SharedUri, Size, Stateful, Styled,
    Subscription, Task, TextRun, TextStyle, TextStyleRefinement, UTF16Selection, UnderlineStyle,
    UniformListScrollHandle, WeakEntity, WeakFocusHandle, Window, div, point, prelude::*,
    pulsating_between, px, relative, size,
};
use hover_links::{HoverLink, HoveredLinkState, find_file};
use hover_popover::{HoverState, hide_hover};
use indent_guides::ActiveIndentGuidesState;
use inlays::{InlaySplice, inlay_hints::InlayHintRefreshReason};
use itertools::{Either, Itertools};
use language::{
    AutoindentMode, BlockCommentConfig, BracketMatch, BracketPair, Buffer, BufferRow,
    BufferSnapshot, Capability, CharClassifier, CharKind, CharScopeContext, CodeLabel, CursorShape,
    DiagnosticEntryRef, DiffOptions, EditPredictionsMode, EditPreview, HighlightedText, IndentKind,
    IndentSize, Language, LanguageAwareStyling, LanguageName, LanguageRegistry, LanguageScope,
    LocalFile, OffsetRangeExt, OutlineItem, Point, Selection, SelectionGoal, TextObject,
    TransactionId, TreeSitterOptions, WordsQuery,
    language_settings::{
        self, LanguageSettings, LspInsertMode, RewrapBehavior, WordsCompletionMode,
        all_language_settings,
    },
    point_from_lsp, point_to_lsp, text_diff_with_options,
};
use linked_editing_ranges::refresh_linked_ranges;
use lsp::{
    CodeActionKind, CompletionItemKind, CompletionTriggerKind, InsertTextFormat, InsertTextMode,
    LanguageServerId,
};
use markdown::Markdown;
pub use mav_actions::editor::RevealInFileManager;
use mav_actions::editor::{MoveDown, MoveUp};
use mouse_context_menu::MouseContextMenu;
use movement::TextLayoutDetails;
use multi_buffer::{
    ExcerptBoundaryInfo, ExpandExcerptDirection, MultiBufferDiffHunk, MultiBufferPoint,
    MultiBufferRow,
};
use parking_lot::Mutex;
use persistence::EditorDb;
use project::{
    BreakpointWithPosition, CodeAction, Completion, CompletionDisplayOptions, CompletionIntent,
    CompletionResponse, CompletionSource, DisableAiSettings, DocumentHighlight, InlayHint, InlayId,
    InvalidationStrategy, Location, LocationLink, LspAction, PrepareRenameResponse, Project,
    ProjectItem, ProjectPath, ProjectTransaction,
    bookmark_store::BookmarkStore,
    debugger::{
        breakpoint_store::{
            Breakpoint, BreakpointEditAction, BreakpointSessionState, BreakpointState,
            BreakpointStore, BreakpointStoreEvent,
        },
        session::{Session, SessionEvent},
    },
    git_store::GitStoreEvent,
    lsp_store::{
        BufferSemanticTokens, CacheInlayHints, CompletionDocumentation, FormatTrigger,
        LspFormatTarget, OpenLspBufferHandle, RefreshForServer,
    },
    project_settings::{DiagnosticSeverity, GoToDiagnosticSeverityFilter, ProjectSettings},
};
use rand::seq::SliceRandom;
use regex::Regex;
use rpc::{ErrorCode, ErrorExt, proto::PeerId};
use scroll::{Autoscroll, OngoingScroll, ScrollAnchor, ScrollManager, SharedScrollAnchor};
use selections_collection::{MutableSelectionsCollection, SelectionsCollection};
use serde::{Deserialize, Serialize};
use settings::{
    GitGutterSetting, RelativeLineNumbers, Settings, SettingsLocation, SettingsStore,
    update_settings_file,
};
use smallvec::{SmallVec, smallvec};
use snippet::Snippet;
use std::{
    any::{Any, TypeId},
    borrow::Cow,
    cell::{OnceCell, RefCell},
    cmp::{self, Ordering, Reverse},
    collections::hash_map,
    iter::{self, Peekable},
    mem,
    num::NonZeroU32,
    ops::{ControlFlow, Deref, DerefMut, Not, Range, RangeInclusive},
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
use task::TaskVariables;
use text::{BufferId, FromAnchor, OffsetUtf16, Rope, ToOffset as _, ToPoint as _};
use theme::{AccentColors, ActiveTheme, GlobalTheme, PlayerColor, Theme};
use theme_settings::{ThemeSettings, observe_buffer_font_size_adjustment};
use ui::{
    Avatar, ButtonSize, ButtonStyle, ContextMenu, Disclosure, IconButton, IconButtonShape,
    IconName, IconSize, Indicator, Key, Tooltip, h_flex, prelude::*, scrollbars::ScrollbarAutoHide,
    utils::WithRemSize,
};
use ui_input::ErasedEditor;
use util::{RangeExt, ResultExt, TryFutureExt, maybe, post_inc};
use workspace::{
    CollaboratorId, Item as WorkspaceItem, ItemId, ItemNavHistory, NavigationEntry, OpenInTerminal,
    OpenTerminal, Pane, RestoreOnStartupBehavior, SERIALIZATION_THROTTLE_TIME, SplitDirection,
    TabBarSettings, Toast, ViewId, Workspace, WorkspaceId, WorkspaceSettings,
    item::{ItemBufferKind, ItemHandle, PreviewTabsSettings, SaveOptions},
    notifications::{DetachAndPromptErr, NotificationId, NotifyResultExt, NotifyTaskExt},
    searchable::SearchEvent,
};

use crate::{
    code_context_menus::CompletionsMenuSource,
    editor_settings::MultiCursorModifier,
    hover_links::{find_url, find_url_from_range},
    inlays::{
        InlineValueCache,
        inlay_hints::{LspInlayHintData, inlay_hint_settings},
    },
    runnables::{ResolvedTasks, RunnableData, RunnableTasks},
    scroll::{ScrollOffset, ScrollPixelOffset},
    selections_collection::resolve_selections_wrapping_blocks,
    semantic_tokens::SemanticTokenState,
    signature_help::{SignatureHelpHiddenBy, SignatureHelpState},
};

const CURSOR_BLINK_INTERVAL: Duration = Duration::from_millis(500);
const MIN_NAVIGATION_HISTORY_ROW_DELTA: i64 = 10;
const MAX_SELECTION_HISTORY_LEN: usize = 1024;

pub enum ActiveDebugLine {}
pub enum DebugStackFrameLine {}

pub enum ConflictsOuter {}
pub enum ConflictsOurs {}
pub enum ConflictsTheirs {}
pub enum ConflictsOursMarker {}
pub enum ConflictsTheirsMarker {}

pub struct HunkAddedColor;
pub struct HunkRemovedColor;

pub fn init(cx: &mut App) {
    cx.set_global(GlobalBlameRenderer(Arc::new(())));
    cx.set_global(breadcrumbs::RenderBreadcrumbText(render_breadcrumb_text));

    workspace::register_project_item::<Editor>(cx);
    workspace::FollowableViewRegistry::register::<Editor>(cx);
    workspace::register_serializable_item::<Editor>(cx);

    cx.observe_new(
        |workspace: &mut Workspace, _: Option<&mut Window>, _cx: &mut Context<Workspace>| {
            workspace.register_action(Editor::new_file);
            workspace.register_action(Editor::new_file_split);
            workspace.register_action(Editor::new_file_vertical);
            workspace.register_action(Editor::new_file_horizontal);
            workspace.register_action(Editor::cancel_language_server_work);
            workspace.register_action(Editor::toggle_focus);
            workspace.register_action(Editor::view_bookmarks);
        },
    )
    .detach();

    cx.on_action(move |_: &workspace::NewFile, cx| {
        let app_state = workspace::AppState::global(cx);
        workspace::open_new(
            Default::default(),
            app_state,
            cx,
            |workspace, window, cx| Editor::new_file(workspace, &Default::default(), window, cx),
        )
        .detach_and_log_err(cx);
    })
    .on_action(move |_: &workspace::NewWindow, cx| {
        let app_state = workspace::AppState::global(cx);
        workspace::open_new(
            Default::default(),
            app_state,
            cx,
            |workspace, window, cx| {
                cx.activate(true);
                Editor::new_file(workspace, &Default::default(), window, cx)
            },
        )
        .detach_and_log_err(cx);
    });
    _ = ui_input::ERASED_EDITOR_FACTORY.set(|window, cx| {
        Arc::new(ErasedEditorImpl(
            cx.new(|cx| Editor::single_line(window, cx)),
        )) as Arc<dyn ErasedEditor>
    });
    _ = multi_buffer::EXCERPT_CONTEXT_LINES.set(multibuffer_context_lines);
}

pub struct SearchWithinRange;

type BackgroundHighlight = (
    Arc<dyn Fn(&usize, &Theme) -> Hsla + Send + Sync>,
    Arc<[Range<Anchor>]>,
);
type GutterHighlight = (fn(&App) -> Hsla, Vec<Range<Anchor>>);

/// Mav's primary implementation of text input, allowing users to edit a [`MultiBuffer`].
///
/// See the [module level documentation](self) for more information.
pub struct Editor {
    focus_handle: FocusHandle,
    last_focused_descendant: Option<WeakFocusHandle>,
    /// The text buffer being edited
    buffer: Entity<MultiBuffer>,
    /// Map of how text in the buffer should be displayed.
    /// Handles soft wraps, folds, fake inlay text insertions, etc.
    pub display_map: Entity<DisplayMap>,
    placeholder_display_map: Option<Entity<DisplayMap>>,
    pub selections: SelectionsCollection,
    /// Manages the scroll position for the given editor.
    ///
    /// Whenever you want to modify the scroll position of the editor, you should
    /// usually use the existing available APIs as opposed to directly interacting
    /// with the scroll manager.
    pub scroll_manager: ScrollManager,
    /// When inline assist editors are linked, they all render cursors because
    /// typing enters text into each of them, even the ones that aren't focused.
    pub(crate) show_cursor_when_unfocused: bool,
    columnar_selection_state: Option<ColumnarSelectionState>,
    add_selections_state: Option<AddSelectionsState>,
    select_next_state: Option<SelectNextState>,
    select_prev_state: Option<SelectNextState>,
    selection_history: SelectionHistory,
    defer_selection_effects: bool,
    deferred_selection_effects_state: Option<DeferredSelectionEffectsState>,
    autoclose_regions: Vec<AutocloseRegion>,
    snippet_stack: InvalidationStack<SnippetState>,
    select_syntax_node_history: SelectSyntaxNodeHistory,
    ime_transaction: Option<TransactionId>,
    pub diagnostics_max_severity: DiagnosticSeverity,
    active_diagnostics: ActiveDiagnostic,
    show_inline_diagnostics: bool,
    inline_diagnostics_update: Task<()>,
    inline_diagnostics_enabled: bool,
    diagnostics_enabled: bool,
    word_completions_enabled: bool,
    inline_diagnostics: Vec<(Anchor, InlineDiagnostic)>,
    soft_wrap_mode_override: Option<language_settings::SoftWrap>,
    hard_wrap: Option<usize>,
    project: Option<Entity<Project>>,
    semantics_provider: Option<Rc<dyn SemanticsProvider>>,
    completion_provider: Option<Rc<dyn CompletionProvider>>,
    collaboration_hub: Option<Box<dyn CollaborationHub>>,
    blink_manager: Entity<BlinkManager>,
    show_cursor_names: bool,
    hovered_cursors: HashMap<HoveredCursor, Task<()>>,
    pub show_local_selections: bool,
    mode: EditorMode,
    breadcrumbs_visibility: BreadcrumbsVisibility,
    show_gutter: bool,
    show_scrollbars: ScrollbarAxes,
    minimap_visibility: MinimapVisibility,
    offset_content: bool,
    disable_expand_excerpt_buttons: bool,
    delegate_expand_excerpts: bool,
    delegate_stage_and_restore: bool,
    delegate_open_excerpts: bool,
    enable_lsp_data: bool,
    needs_initial_data_update: bool,
    enable_runnables: bool,
    enable_code_lens: bool,
    enable_mouse_wheel_zoom: bool,
    show_line_numbers: Option<bool>,
    use_relative_line_numbers: Option<bool>,
    show_git_diff_gutter: Option<bool>,
    show_code_actions: Option<bool>,
    show_runnables: Option<bool>,
    show_bookmarks: Option<bool>,
    show_breakpoints: Option<bool>,
    show_diff_review_button: bool,
    show_wrap_guides: Option<bool>,
    show_indent_guides: Option<bool>,
    buffers_with_disabled_indent_guides: HashSet<BufferId>,
    highlight_order: usize,
    highlighted_rows: TypeIdHashMap<Vec<RowHighlight>>,
    background_highlights: HashMap<HighlightKey, BackgroundHighlight>,
    navigation_overlays: HashMap<NavigationOverlayKey, Arc<[NavigationTargetOverlay]>>,
    gutter_highlights: TypeIdHashMap<GutterHighlight>,
    scrollbar_marker_state: ScrollbarMarkerState,
    active_indent_guides_state: ActiveIndentGuidesState,
    nav_history: Option<ItemNavHistory>,
    context_menu: RefCell<Option<CodeContextMenu>>,
    context_menu_options: Option<ContextMenuOptions>,
    mouse_context_menu: Option<MouseContextMenu>,
    completion_tasks: Vec<(CompletionId, Task<()>)>,
    inline_blame_popover: Option<InlineBlamePopover>,
    inline_blame_popover_show_task: Option<Task<()>>,
    signature_help_state: SignatureHelpState,
    auto_signature_help: Option<bool>,
    find_all_references_task_sources: Vec<Anchor>,
    next_completion_id: CompletionId,
    code_actions_for_selection: CodeActionsForSelection,
    runnables_for_selection_toggle: Task<()>,
    quick_selection_highlight_task: Option<(Range<Anchor>, Task<()>)>,
    debounced_selection_highlight_task: Option<(Range<Anchor>, Task<()>)>,
    debounced_selection_highlight_complete: bool,
    last_selection_from_search: bool,
    document_highlights_task: Option<Task<()>>,
    linked_editing_range_task: Option<Task<Option<()>>>,
    linked_edit_ranges: linked_editing_ranges::LinkedEditingRanges,
    pending_rename: Option<RenameState>,
    searchable: bool,
    cursor_shape: CursorShape,
    /// Whether the cursor is offset one character to the left when something is
    /// selected (needed for vim visual mode)
    cursor_offset_on_selection: bool,
    current_line_highlight: Option<CurrentLineHighlight>,
    /// Whether to collapse search match ranges to just their start position.
    /// When true, navigating to a match positions the cursor at the match
    /// without selecting the matched text.
    collapse_matches: bool,
    autoindent_mode: Option<AutoindentMode>,
    workspace: Option<(WeakEntity<Workspace>, Option<WorkspaceId>)>,
    input_enabled: bool,
    expects_character_input: bool,
    use_modal_editing: bool,
    read_only: bool,
    leader_id: Option<CollaboratorId>,
    remote_id: Option<ViewId>,
    pub hover_state: HoverState,
    pending_mouse_down: Option<Rc<RefCell<Option<MouseDownEvent>>>>,
    prev_pressure_stage: Option<PressureStage>,
    gutter_hovered: bool,
    hovered_link_state: Option<HoveredLinkState>,
    edit_prediction_provider: Option<RegisteredEditPredictionDelegate>,
    code_action_providers: Vec<Rc<dyn CodeActionProvider>>,
    active_edit_prediction: Option<EditPredictionState>,
    /// Used to prevent flickering as the user types while the menu is open
    stale_edit_prediction_in_menu: Option<EditPredictionState>,
    edit_prediction_settings: EditPredictionSettings,
    edit_predictions_hidden_for_vim_mode: bool,
    show_edit_predictions_override: Option<bool>,
    show_completions_on_input_override: Option<bool>,
    menu_edit_predictions_policy: MenuEditPredictionsPolicy,
    edit_prediction_preview: EditPredictionPreview,
    in_leading_whitespace: bool,
    next_inlay_id: usize,
    next_color_inlay_id: usize,
    _subscriptions: Vec<Subscription>,
    pixel_position_of_newest_cursor: Option<gpui::Point<Pixels>>,
    gutter_dimensions: GutterDimensions,
    style: Option<EditorStyle>,
    text_style_refinement: Option<TextStyleRefinement>,
    next_editor_action_id: EditorActionId,
    editor_actions: Rc<
        RefCell<BTreeMap<EditorActionId, Box<dyn Fn(&Editor, &mut Window, &mut Context<Self>)>>>,
    >,
    use_autoclose: bool,
    use_auto_surround: bool,
    use_selection_highlight: bool,
    auto_replace_emoji_shortcode: bool,
    jsx_tag_auto_close_enabled_in_any_buffer: bool,
    show_git_blame_gutter: bool,
    show_git_blame_inline: bool,
    show_git_blame_inline_delay_task: Option<Task<()>>,
    git_blame_inline_enabled: bool,
    render_diff_hunk_controls: RenderDiffHunkControlsFn,
    buffer_serialization: Option<BufferSerialization>,
    show_selection_menu: Option<bool>,
    blame: Option<Entity<GitBlame>>,
    blame_subscription: Option<Subscription>,
    custom_context_menu: Option<
        Box<
            dyn 'static
                + Fn(
                    &mut Self,
                    DisplayPoint,
                    &mut Window,
                    &mut Context<Self>,
                ) -> Option<Entity<ui::ContextMenu>>,
        >,
    >,
    last_bounds: Option<Bounds<Pixels>>,
    last_position_map: Option<Rc<PositionMap>>,
    /// The right margin (vertical scrollbar + minimap width) the editor was
    /// last laid out with, updated on every prepaint.
    /// Used later in the frame by `SplitBufferHeadersElement` to shrink the
    /// width available to buffer headers.
    last_right_margin: Pixels,
    /// Whether the horizontal scrollbar was laid out as visible during the last
    /// prepaint.
    /// Used by `SplitBufferHeadersElement` to clip buffer headers so they don't
    /// paint over the scrollbar.
    last_horizontal_scrollbar_visible: bool,
    expect_bounds_change: Option<Bounds<Pixels>>,
    runnables: RunnableData,
    bookmark_store: Option<Entity<BookmarkStore>>,
    breakpoint_store: Option<Entity<BreakpointStore>>,
    gutter_hover_button: (Option<GutterHoverButton>, Option<Task<()>>),
    pub(crate) gutter_diff_review_indicator: (Option<PhantomDiffReviewIndicator>, Option<Task<()>>),
    pub(crate) diff_review_drag_state: Option<DiffReviewDragState>,
    /// Active diff review overlays. Multiple overlays can be open simultaneously
    /// when hunks have comments stored.
    pub(crate) diff_review_overlays: Vec<DiffReviewOverlay>,
    /// Stored review comments grouped by hunk.
    /// Uses a Vec instead of HashMap because DiffHunkKey contains an Anchor
    /// which doesn't implement Hash/Eq in a way suitable for HashMap keys.
    stored_review_comments: Vec<(DiffHunkKey, Vec<StoredReviewComment>)>,
    /// Counter for generating unique comment IDs.
    next_review_comment_id: usize,
    hovered_diff_hunk_row: Option<DisplayRow>,
    pull_diagnostics_task: Task<()>,
    in_project_search: bool,
    previous_search_ranges: Option<Arc<[Range<Anchor>]>>,
    breadcrumb_header: Option<String>,
    focused_block: Option<FocusedBlock>,
    next_scroll_position: NextScrollCursorCenterTopBottom,
    addons: TypeIdHashMap<Box<dyn Addon>>,
    registered_buffers: HashMap<BufferId, OpenLspBufferHandle>,
    load_diff_task: Option<Shared<Task<()>>>,
    /// Whether we are temporarily displaying a diff other than git's
    temporary_diff_override: bool,
    /// Whether to render all diff hunks with the "unstaged" appearance,
    /// regardless of whether they have a secondary hunk. Used by views whose
    /// diffs aren't related to the git index (e.g. agent diffs).
    render_diff_hunks_as_unstaged: bool,
    selection_mark_mode: bool,
    toggle_fold_multiple_buffers: Task<()>,
    _scroll_cursor_center_top_bottom_task: Task<()>,
    serialize_selections: Task<()>,
    serialize_folds: Task<()>,
    minimap: Option<Entity<Self>>,
    pub change_list: ChangeList,
    inline_value_cache: InlineValueCache,
    number_deleted_lines: bool,

    selection_drag_state: SelectionDragState,
    colors: Option<LspColorData>,
    code_lens: Option<CodeLensState>,
    post_scroll_update: Task<()>,
    refresh_colors_task: Task<()>,
    refresh_code_lens_task: Task<()>,
    use_document_folding_ranges: bool,
    refresh_folding_ranges_task: Task<()>,
    inlay_hints: Option<LspInlayHintData>,
    folding_newlines: Task<()>,
    select_next_is_case_sensitive: Option<bool>,
    pub lookup_key: Option<Box<dyn Any + Send + Sync>>,
    on_local_selections_changed:
        Option<Box<dyn Fn(Point, &mut Window, &mut Context<Self>) + 'static>>,
    suppress_selection_callback: bool,
    applicable_language_settings: HashMap<Option<LanguageName>, LanguageSettings>,
    accent_data: Option<AccentData>,
    bracket_fetched_tree_sitter_chunks: HashMap<Range<text::Anchor>, HashSet<Range<BufferRow>>>,
    semantic_token_state: SemanticTokenState,
    pub(crate) refresh_matching_bracket_highlights_task: Task<()>,
    refresh_document_symbols_task: Shared<Task<()>>,
    lsp_document_links: LspDocumentLinks,
    lsp_document_symbols: HashMap<BufferId, Vec<OutlineItem<text::Anchor>>>,
    refresh_outline_symbols_at_cursor_at_cursor_task: Task<()>,
    outline_symbols_at_cursor: Option<(BufferId, Vec<OutlineItem<Anchor>>)>,
    sticky_headers_task: Task<()>,
    sticky_headers: Option<Vec<OutlineItem<Anchor>>>,
    pub(crate) colorize_brackets_task: Task<()>,
}

impl Editor {
    pub fn single_line(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let buffer = cx.new(|cx| Buffer::local("", cx));
        let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
        Self::new(EditorMode::SingleLine, buffer, None, window, cx)
    }

    pub fn multi_line(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let buffer = cx.new(|cx| Buffer::local("", cx));
        let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
        Self::new(EditorMode::full(), buffer, None, window, cx)
    }

    pub fn auto_height(
        min_lines: usize,
        max_lines: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let buffer = cx.new(|cx| Buffer::local("", cx));
        let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
        Self::new(
            EditorMode::AutoHeight {
                min_lines,
                max_lines: Some(max_lines),
            },
            buffer,
            None,
            window,
            cx,
        )
    }

    /// Creates a new auto-height editor with a minimum number of lines but no maximum.
    /// The editor grows as tall as needed to fit its content.
    pub fn auto_height_unbounded(
        min_lines: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let buffer = cx.new(|cx| Buffer::local("", cx));
        let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
        Self::new(
            EditorMode::AutoHeight {
                min_lines,
                max_lines: None,
            },
            buffer,
            None,
            window,
            cx,
        )
    }

    pub fn for_buffer(
        buffer: Entity<Buffer>,
        project: Option<Entity<Project>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
        Self::new(EditorMode::full(), buffer, project, window, cx)
    }

    pub fn for_multibuffer(
        buffer: Entity<MultiBuffer>,
        project: Option<Entity<Project>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new(EditorMode::full(), buffer, project, window, cx)
    }

    pub fn clone(&self, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut clone = Self::new(
            self.mode.clone(),
            self.buffer.clone(),
            self.project.clone(),
            window,
            cx,
        );
        let my_snapshot = self.display_map.update(cx, |display_map, cx| {
            let snapshot = display_map.snapshot(cx);
            clone.display_map.update(cx, |display_map, cx| {
                display_map.set_state(&snapshot, cx);
            });
            snapshot
        });
        let clone_snapshot = clone.display_map.update(cx, |map, cx| map.snapshot(cx));
        clone.folds_did_change(cx);
        clone.selections.clone_state(&self.selections);
        clone
            .scroll_manager
            .clone_state(&self.scroll_manager, &my_snapshot, &clone_snapshot, cx);
        clone.searchable = self.searchable;
        clone.read_only = self.read_only;
        clone.buffers_with_disabled_indent_guides =
            self.buffers_with_disabled_indent_guides.clone();
        clone.enable_mouse_wheel_zoom = self.enable_mouse_wheel_zoom;
        clone.enable_lsp_data = self.enable_lsp_data;
        clone.needs_initial_data_update = self.enable_lsp_data;
        clone.enable_runnables = self.enable_runnables;
        clone.enable_code_lens = self.enable_code_lens;
        clone
    }

    pub fn new(
        mode: EditorMode,
        buffer: Entity<MultiBuffer>,
        project: Option<Entity<Project>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Editor::new_internal(mode, buffer, project, None, window, cx)
    }

    pub fn refresh_sticky_headers(
        &mut self,
        display_snapshot: &DisplaySnapshot,
        cx: &mut Context<Editor>,
    ) {
        if !self.mode.is_full() {
            return;
        }
        let multi_buffer = display_snapshot.buffer_snapshot().clone();
        let scroll_anchor = self
            .scroll_manager
            .native_anchor(display_snapshot, cx)
            .anchor;
        let Some(buffer_snapshot) = multi_buffer.as_singleton() else {
            return;
        };

        let buffer = buffer_snapshot.clone();
        let Some((buffer_visible_start, _)) = multi_buffer.anchor_to_buffer_anchor(scroll_anchor)
        else {
            return;
        };
        let buffer_visible_start = buffer_visible_start.to_point(&buffer);
        let max_row = buffer.max_point().row;
        let start_row = buffer_visible_start.row.min(max_row);
        let end_row = (buffer_visible_start.row + 10).min(max_row);

        let syntax = self.style(cx).syntax.clone();
        let background_task = cx.background_spawn(async move {
            buffer
                .outline_items_containing(
                    Point::new(start_row, 0)..Point::new(end_row, 0),
                    true,
                    Some(syntax.as_ref()),
                )
                .into_iter()
                .filter_map(|outline_item| {
                    Some(OutlineItem {
                        depth: outline_item.depth,
                        range: multi_buffer
                            .buffer_anchor_range_to_anchor_range(outline_item.range)?,
                        selection_range: multi_buffer
                            .buffer_anchor_range_to_anchor_range(outline_item.selection_range)?,
                        source_range_for_text: multi_buffer.buffer_anchor_range_to_anchor_range(
                            outline_item.source_range_for_text,
                        )?,
                        text: outline_item.text,
                        highlight_ranges: outline_item.highlight_ranges,
                        name_ranges: outline_item.name_ranges,
                        body_range: outline_item.body_range.and_then(|range| {
                            multi_buffer.buffer_anchor_range_to_anchor_range(range)
                        }),
                        annotation_range: outline_item.annotation_range.and_then(|range| {
                            multi_buffer.buffer_anchor_range_to_anchor_range(range)
                        }),
                    })
                })
                .collect()
        });
        self.sticky_headers_task = cx.spawn(async move |this, cx| {
            let sticky_headers = background_task.await;
            this.update(cx, |this, cx| {
                if this.sticky_headers.as_ref() != Some(&sticky_headers) {
                    this.sticky_headers = Some(sticky_headers);
                    cx.notify();
                }
            })
            .ok();
        });
    }

    fn new_internal(
        mode: EditorMode,
        multi_buffer: Entity<MultiBuffer>,
        project: Option<Entity<Project>>,
        display_map: Option<Entity<DisplayMap>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        debug_assert!(
            display_map.is_none() || mode.is_minimap(),
            "Providing a display map for a new editor is only intended for the minimap and might have unintended side effects otherwise!"
        );

        let full_mode = mode.is_full();
        let is_minimap = mode.is_minimap();
        let diagnostics_max_severity = if full_mode {
            EditorSettings::get_global(cx)
                .diagnostics_max_severity
                .unwrap_or(DiagnosticSeverity::Hint)
        } else {
            DiagnosticSeverity::Off
        };
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let editor = cx.entity().downgrade();
        let fold_placeholder = FoldPlaceholder {
            constrain_width: false,
            render: Arc::new(move |fold_id, fold_range, cx| {
                let editor = editor.clone();
                FoldPlaceholder::fold_element(fold_id, cx)
                    .cursor_pointer()
                    .child("⋯")
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(move |_, _window, cx| {
                        editor
                            .update(cx, |editor, cx| {
                                editor.unfold_ranges(
                                    &[fold_range.start..fold_range.end],
                                    true,
                                    false,
                                    cx,
                                );
                                cx.stop_propagation();
                            })
                            .ok();
                    })
                    .into_any()
            }),
            merge_adjacent: true,
            ..FoldPlaceholder::default()
        };
        let display_map = display_map.unwrap_or_else(|| {
            cx.new(|cx| {
                DisplayMap::new(
                    multi_buffer.clone(),
                    style.font(),
                    font_size,
                    None,
                    FILE_HEADER_HEIGHT,
                    MULTI_BUFFER_EXCERPT_HEADER_HEIGHT,
                    fold_placeholder,
                    diagnostics_max_severity,
                    cx,
                )
            })
        });

        let selections = SelectionsCollection::new();

        let blink_manager = cx.new(|cx| {
            let mut blink_manager = BlinkManager::new(
                CURSOR_BLINK_INTERVAL,
                |cx| EditorSettings::get_global(cx).cursor_blink,
                cx,
            );
            if is_minimap {
                blink_manager.disable(cx);
            }
            blink_manager
        });

        let soft_wrap_mode_override =
            matches!(mode, EditorMode::SingleLine).then(|| language_settings::SoftWrap::None);

        let mut project_subscriptions = Vec::new();
        if full_mode && let Some(project) = project.as_ref() {
            project_subscriptions.push(cx.subscribe_in(
                project,
                window,
                |editor, _, event, window, cx| match event {
                    project::Event::RefreshCodeLens => {
                        editor.refresh_code_lenses(None, window, cx);
                    }
                    project::Event::RefreshInlayHints {
                        server_id,
                        request_id,
                    } => {
                        editor.refresh_inlay_hints(
                            InlayHintRefreshReason::RefreshRequested {
                                server_id: *server_id,
                                request_id: *request_id,
                            },
                            cx,
                        );
                    }
                    project::Event::RefreshSemanticTokens {
                        server_id,
                        request_id,
                    } => {
                        editor.refresh_semantic_tokens(
                            None,
                            Some(RefreshForServer {
                                server_id: *server_id,
                                request_id: *request_id,
                            }),
                            cx,
                        );
                    }
                    project::Event::LanguageServerRemoved(_) => {
                        editor.registered_buffers.clear();
                        editor.register_visible_buffers(cx);
                        editor.invalidate_semantic_tokens(None);
                        editor.refresh_runnables(None, window, cx);
                        editor.update_lsp_data(None, window, cx);
                        editor.refresh_inlay_hints(InlayHintRefreshReason::ServerRemoved, cx);
                    }
                    project::Event::SnippetEdit(id, snippet_edits) => {
                        // todo(lw): Non singletons
                        if let Some(buffer) = editor.buffer.read(cx).as_singleton() {
                            let snapshot = buffer.read(cx).snapshot();
                            let focus_handle = editor.focus_handle(cx);
                            if snapshot.remote_id() == *id && focus_handle.is_focused(window) {
                                for (range, snippet) in snippet_edits {
                                    let buffer_range =
                                        language::range_from_lsp(*range).to_offset(&snapshot);
                                    editor
                                        .insert_snippet(
                                            &[MultiBufferOffset(buffer_range.start)
                                                ..MultiBufferOffset(buffer_range.end)],
                                            snippet.clone(),
                                            window,
                                            cx,
                                        )
                                        .ok();
                                }
                            }
                        }
                    }
                    project::Event::LanguageServerBufferRegistered { buffer_id, .. } => {
                        let buffer_id = *buffer_id;
                        if editor.buffer().read(cx).buffer(buffer_id).is_some() {
                            editor.register_buffer(buffer_id, cx);
                            editor.refresh_runnables(Some(buffer_id), window, cx);
                            editor.update_lsp_data(Some(buffer_id), window, cx);
                            editor.refresh_inlay_hints(InlayHintRefreshReason::NewLinesShown, cx);
                            refresh_linked_ranges(editor, window, cx);
                            editor.refresh_code_actions_for_selection(window, cx);
                            editor.refresh_document_highlights(cx);
                        }
                    }

                    project::Event::EntryRenamed(transaction, project_path, abs_path) => {
                        let Some(workspace) = editor.workspace() else {
                            return;
                        };
                        let Some(active_editor) = workspace.read(cx).active_item_as::<Self>(cx)
                        else {
                            return;
                        };

                        if active_editor.entity_id() == cx.entity_id() {
                            let entity_id = cx.entity_id();
                            workspace.update(cx, |this, cx| {
                                this.panes_mut()
                                    .iter_mut()
                                    .filter(|pane| pane.entity_id() != entity_id)
                                    .for_each(|p| {
                                        p.update(cx, |pane, _| {
                                            pane.nav_history_mut().rename_item(
                                                entity_id,
                                                project_path.clone(),
                                                abs_path.clone().into(),
                                            );
                                        })
                                    });
                            });

                            Self::open_transaction_for_hidden_buffers(
                                workspace,
                                transaction.clone(),
                                "Rename".to_string(),
                                window,
                                cx,
                            );
                        }
                    }

                    project::Event::WorkspaceEditApplied(transaction) => {
                        let Some(workspace) = editor.workspace() else {
                            return;
                        };
                        let Some(active_editor) = workspace.read(cx).active_item_as::<Self>(cx)
                        else {
                            return;
                        };

                        if active_editor.entity_id() == cx.entity_id() {
                            Self::open_transaction_for_hidden_buffers(
                                workspace,
                                transaction.clone(),
                                "LSP Edit".to_string(),
                                window,
                                cx,
                            );
                        }
                    }

                    _ => {}
                },
            ));
            if let Some(task_inventory) = project
                .read(cx)
                .task_store()
                .read(cx)
                .task_inventory()
                .cloned()
            {
                project_subscriptions.push(cx.observe_in(
                    &task_inventory,
                    window,
                    |editor, _, window, cx| {
                        editor.refresh_runnables(None, window, cx);
                    },
                ));
            };

            project_subscriptions.push(cx.subscribe_in(
                &project.read(cx).breakpoint_store(),
                window,
                |editor, _, event, window, cx| match event {
                    BreakpointStoreEvent::ClearDebugLines => {
                        editor.clear_row_highlights::<ActiveDebugLine>();
                        editor.refresh_inline_values(cx);
                    }
                    BreakpointStoreEvent::SetDebugLine => {
                        if editor.go_to_active_debug_line(window, cx) {
                            cx.stop_propagation();
                        }

                        editor.refresh_inline_values(cx);
                    }
                    _ => {}
                },
            ));
            let git_store = project.read(cx).git_store().clone();
            let project = project.clone();
            project_subscriptions.push(cx.subscribe(&git_store, move |this, _, event, cx| {
                if let GitStoreEvent::RepositoryAdded = event {
                    this.load_diff_task = Some(
                        update_uncommitted_diff_for_buffer(
                            cx.entity(),
                            &project,
                            this.buffer.read(cx).all_buffers(),
                            this.buffer.clone(),
                            cx,
                        )
                        .shared(),
                    );
                }
            }));
        }

        let buffer_snapshot = multi_buffer.read(cx).snapshot(cx);

        let inlay_hint_settings =
            inlay_hint_settings(selections.newest_anchor().head(), &buffer_snapshot, cx);
        let focus_handle = cx.focus_handle();
        if !is_minimap {
            cx.on_focus(&focus_handle, window, Self::handle_focus)
                .detach();
            cx.on_focus_in(&focus_handle, window, Self::handle_focus_in)
                .detach();
            cx.on_focus_out(&focus_handle, window, Self::handle_focus_out)
                .detach();
            cx.on_blur(&focus_handle, window, Self::handle_blur)
                .detach();
            cx.observe_pending_input(window, Self::observe_pending_input)
                .detach();
        }

        let show_indent_guides =
            if matches!(mode, EditorMode::SingleLine | EditorMode::Minimap { .. }) {
                Some(false)
            } else {
                None
            };

        let bookmark_store = match (&mode, project.as_ref()) {
            (EditorMode::Full { .. }, Some(project)) => Some(project.read(cx).bookmark_store()),
            _ => None,
        };

        let breakpoint_store = match (&mode, project.as_ref()) {
            (EditorMode::Full { .. }, Some(project)) => Some(project.read(cx).breakpoint_store()),
            _ => None,
        };

        let mut code_action_providers = Vec::new();
        let mut load_uncommitted_diff = None;
        if let Some(project) = project.clone() {
            load_uncommitted_diff = Some(
                update_uncommitted_diff_for_buffer(
                    cx.entity(),
                    &project,
                    multi_buffer.read(cx).all_buffers(),
                    multi_buffer.clone(),
                    cx,
                )
                .shared(),
            );
            code_action_providers.push(Rc::new(project) as Rc<_>);
        }

        let mut editor = Self {
            focus_handle,
            show_cursor_when_unfocused: false,
            last_focused_descendant: None,
            buffer: multi_buffer.clone(),
            display_map: display_map.clone(),
            placeholder_display_map: None,
            selections,
            scroll_manager: ScrollManager::new(cx),
            columnar_selection_state: None,
            add_selections_state: None,
            select_next_state: None,
            select_prev_state: None,
            selection_history: SelectionHistory::default(),
            defer_selection_effects: false,
            deferred_selection_effects_state: None,
            autoclose_regions: Vec::new(),
            snippet_stack: InvalidationStack::default(),
            select_syntax_node_history: SelectSyntaxNodeHistory::default(),
            ime_transaction: None,
            active_diagnostics: ActiveDiagnostic::None,
            show_inline_diagnostics: ProjectSettings::get_global(cx).diagnostics.inline.enabled,
            inline_diagnostics_update: Task::ready(()),
            inline_diagnostics: Vec::new(),
            soft_wrap_mode_override,
            diagnostics_max_severity,
            hard_wrap: None,
            completion_provider: project.clone().map(|project| Rc::new(project) as _),
            semantics_provider: project
                .as_ref()
                .map(|project| Rc::new(project.downgrade()) as _),
            collaboration_hub: project.clone().map(|project| Box::new(project) as _),
            project,
            blink_manager: blink_manager.clone(),
            show_local_selections: true,
            show_scrollbars: ScrollbarAxes {
                horizontal: full_mode,
                vertical: full_mode,
            },
            minimap_visibility: MinimapVisibility::for_mode(&mode, cx),
            offset_content: !matches!(mode, EditorMode::SingleLine),
            breadcrumbs_visibility: BreadcrumbsVisibility::from_settings(cx),
            show_gutter: full_mode,
            show_line_numbers: (!full_mode).then_some(false),
            use_relative_line_numbers: None,
            disable_expand_excerpt_buttons: !full_mode,
            delegate_expand_excerpts: false,
            delegate_stage_and_restore: false,
            delegate_open_excerpts: false,
            enable_lsp_data: full_mode,
            needs_initial_data_update: full_mode,
            enable_runnables: full_mode,
            enable_code_lens: full_mode,
            enable_mouse_wheel_zoom: full_mode,
            show_git_diff_gutter: None,
            show_code_actions: None,
            show_runnables: None,
            show_bookmarks: None,
            show_breakpoints: None,
            show_diff_review_button: false,
            show_wrap_guides: None,
            show_indent_guides,
            buffers_with_disabled_indent_guides: HashSet::default(),
            highlight_order: 0,
            highlighted_rows: Default::default(),
            background_highlights: HashMap::default(),
            navigation_overlays: HashMap::default(),
            gutter_highlights: Default::default(),
            scrollbar_marker_state: ScrollbarMarkerState::default(),
            active_indent_guides_state: ActiveIndentGuidesState::default(),
            nav_history: None,
            context_menu: RefCell::new(None),
            context_menu_options: None,
            mouse_context_menu: None,
            completion_tasks: Vec::new(),
            inline_blame_popover: None,
            inline_blame_popover_show_task: None,
            signature_help_state: SignatureHelpState::default(),
            auto_signature_help: None,
            find_all_references_task_sources: Vec::new(),
            next_completion_id: 0,
            next_inlay_id: 0,
            code_action_providers,
            code_actions_for_selection: CodeActionsForSelection::None,
            runnables_for_selection_toggle: Task::ready(()),
            quick_selection_highlight_task: None,
            debounced_selection_highlight_task: None,
            debounced_selection_highlight_complete: false,
            last_selection_from_search: false,
            document_highlights_task: None,
            linked_editing_range_task: None,
            pending_rename: None,
            searchable: !is_minimap,
            cursor_shape: EditorSettings::get_global(cx)
                .cursor_shape
                .unwrap_or_default(),
            cursor_offset_on_selection: false,
            current_line_highlight: None,
            autoindent_mode: Some(AutoindentMode::EachLine),
            collapse_matches: false,
            workspace: None,
            input_enabled: !is_minimap,
            expects_character_input: !is_minimap,
            use_modal_editing: full_mode,
            read_only: is_minimap,
            use_autoclose: true,
            use_auto_surround: true,
            use_selection_highlight: true,
            auto_replace_emoji_shortcode: false,
            jsx_tag_auto_close_enabled_in_any_buffer: false,
            leader_id: None,
            remote_id: None,
            hover_state: HoverState::default(),
            pending_mouse_down: None,
            prev_pressure_stage: None,
            hovered_link_state: None,
            edit_prediction_provider: None,
            active_edit_prediction: None,
            stale_edit_prediction_in_menu: None,
            edit_prediction_preview: EditPredictionPreview::Inactive {
                released_too_fast: false,
            },
            inline_diagnostics_enabled: full_mode,
            diagnostics_enabled: full_mode,
            word_completions_enabled: full_mode,
            inline_value_cache: InlineValueCache::new(inlay_hint_settings.show_value_hints),
            gutter_hovered: false,
            pixel_position_of_newest_cursor: None,
            last_bounds: None,
            last_position_map: None,
            last_right_margin: Pixels::ZERO,
            last_horizontal_scrollbar_visible: false,
            expect_bounds_change: None,
            gutter_dimensions: GutterDimensions::default(),
            style: None,
            show_cursor_names: false,
            hovered_cursors: HashMap::default(),
            next_editor_action_id: EditorActionId::default(),
            editor_actions: Rc::default(),
            edit_predictions_hidden_for_vim_mode: false,
            show_edit_predictions_override: None,
            show_completions_on_input_override: None,
            menu_edit_predictions_policy: MenuEditPredictionsPolicy::ByProvider,
            edit_prediction_settings: EditPredictionSettings::Disabled,
            in_leading_whitespace: false,
            custom_context_menu: None,
            show_git_blame_gutter: false,
            show_git_blame_inline: false,
            show_selection_menu: None,
            show_git_blame_inline_delay_task: None,
            git_blame_inline_enabled: full_mode
                && ProjectSettings::get_global(cx).git.inline_blame.enabled,
            render_diff_hunk_controls: Arc::new(render_diff_hunk_controls),
            buffer_serialization: is_minimap.not().then(|| {
                BufferSerialization::new(
                    ProjectSettings::get_global(cx)
                        .session
                        .restore_unsaved_buffers,
                )
            }),
            blame: None,
            blame_subscription: None,

            bookmark_store,
            breakpoint_store,
            gutter_hover_button: (None, None),
            gutter_diff_review_indicator: (None, None),
            diff_review_drag_state: None,
            diff_review_overlays: Vec::new(),
            stored_review_comments: Vec::new(),
            next_review_comment_id: 0,
            hovered_diff_hunk_row: None,
            _subscriptions: (!is_minimap)
                .then(|| {
                    vec![
                        cx.observe(&multi_buffer, Self::on_buffer_changed),
                        cx.subscribe_in(&multi_buffer, window, Self::on_buffer_event),
                        cx.observe_in(&display_map, window, Self::on_display_map_changed),
                        cx.observe(&blink_manager, |_, _, cx| cx.notify()),
                        cx.observe_global_in::<SettingsStore>(window, Self::settings_changed),
                        cx.observe_global_in::<GlobalTheme>(window, Self::theme_changed),
                        observe_buffer_font_size_adjustment(cx, |_, cx| cx.notify()),
                        cx.observe_window_activation(window, |editor, window, cx| {
                            let active = window.is_window_active();
                            editor.blink_manager.update(cx, |blink_manager, cx| {
                                if active {
                                    blink_manager.enable(cx);
                                } else {
                                    blink_manager.disable(cx);
                                }
                            });
                        }),
                    ]
                })
                .unwrap_or_default(),
            runnables: RunnableData::new(),
            pull_diagnostics_task: Task::ready(()),
            colors: None,
            code_lens: None,
            refresh_colors_task: Task::ready(()),
            refresh_code_lens_task: Task::ready(()),
            use_document_folding_ranges: false,
            refresh_folding_ranges_task: Task::ready(()),
            inlay_hints: None,
            next_color_inlay_id: 0,
            post_scroll_update: Task::ready(()),
            linked_edit_ranges: Default::default(),
            in_project_search: false,
            previous_search_ranges: None,
            breadcrumb_header: None,
            focused_block: None,
            next_scroll_position: NextScrollCursorCenterTopBottom::default(),
            addons: Default::default(),
            registered_buffers: HashMap::default(),
            _scroll_cursor_center_top_bottom_task: Task::ready(()),
            selection_mark_mode: false,
            toggle_fold_multiple_buffers: Task::ready(()),
            serialize_selections: Task::ready(()),
            serialize_folds: Task::ready(()),
            text_style_refinement: None,
            load_diff_task: load_uncommitted_diff,
            temporary_diff_override: false,
            render_diff_hunks_as_unstaged: false,
            minimap: None,
            change_list: ChangeList::new(),
            mode,
            selection_drag_state: SelectionDragState::None,
            folding_newlines: Task::ready(()),
            lookup_key: None,
            select_next_is_case_sensitive: None,
            on_local_selections_changed: None,
            suppress_selection_callback: false,
            applicable_language_settings: HashMap::default(),
            semantic_token_state: SemanticTokenState::new(cx, full_mode),
            accent_data: None,
            bracket_fetched_tree_sitter_chunks: HashMap::default(),
            number_deleted_lines: false,
            refresh_matching_bracket_highlights_task: Task::ready(()),
            refresh_document_symbols_task: Task::ready(()).shared(),
            lsp_document_links: LspDocumentLinks::new(cx),
            lsp_document_symbols: HashMap::default(),
            refresh_outline_symbols_at_cursor_at_cursor_task: Task::ready(()),
            outline_symbols_at_cursor: None,
            sticky_headers_task: Task::ready(()),
            sticky_headers: None,
            colorize_brackets_task: Task::ready(()),
        };

        if is_minimap {
            return editor;
        }

        editor.applicable_language_settings = editor.fetch_applicable_language_settings(cx);
        editor.accent_data = editor.fetch_accent_data(cx);

        if let Some(breakpoints) = editor.breakpoint_store.as_ref() {
            editor
                ._subscriptions
                .push(cx.observe(breakpoints, |_, _, cx| {
                    cx.notify();
                }));
        }
        editor._subscriptions.extend(project_subscriptions);

        editor._subscriptions.push(cx.subscribe_in(
            &cx.entity(),
            window,
            |editor, _, e: &EditorEvent, window, cx| match e {
                EditorEvent::ScrollPositionChanged { local, .. } => {
                    if *local {
                        editor.hide_signature_help(cx, SignatureHelpHiddenBy::Escape);
                        editor.hide_blame_popover(true, cx);
                        let snapshot = editor.snapshot(window, cx);
                        let new_anchor = editor
                            .scroll_manager
                            .native_anchor(&snapshot.display_snapshot, cx);
                        editor.update_restoration_data(cx, move |data| {
                            data.scroll_position = (
                                new_anchor.top_row(snapshot.buffer_snapshot()),
                                new_anchor.offset,
                            );
                        });

                        editor.update_data_on_scroll(true, window, cx);
                    }
                    editor.refresh_sticky_headers(&editor.snapshot(window, cx), cx);
                }
                EditorEvent::Edited { .. } => {
                    let vim_mode = vim_mode_setting::VimModeSetting::try_get(cx)
                        .map(|vim_mode| vim_mode.0)
                        .unwrap_or(false);
                    if !vim_mode {
                        let display_map = editor.display_snapshot(cx);
                        let selections = editor.selections.all_adjusted_display(&display_map);
                        let pop_state = editor
                            .change_list
                            .last()
                            .map(|previous| {
                                previous.len() == selections.len()
                                    && previous.iter().enumerate().all(|(ix, p)| {
                                        p.to_display_point(&display_map).row()
                                            == selections[ix].head().row()
                                    })
                            })
                            .unwrap_or(false);
                        let new_positions = selections
                            .into_iter()
                            .map(|s| display_map.display_point_to_anchor(s.head(), Bias::Left))
                            .collect();
                        editor
                            .change_list
                            .push_to_change_list(pop_state, new_positions);
                    }
                }
                _ => (),
            },
        ));

        if let Some(dap_store) = editor
            .project
            .as_ref()
            .map(|project| project.read(cx).dap_store())
        {
            let weak_editor = cx.weak_entity();

            editor
                ._subscriptions
                .push(
                    cx.observe_new::<project::debugger::session::Session>(move |_, _, cx| {
                        let session_entity = cx.entity();
                        weak_editor
                            .update(cx, |editor, cx| {
                                editor._subscriptions.push(
                                    cx.subscribe(&session_entity, Self::on_debug_session_event),
                                );
                            })
                            .ok();
                    }),
                );

            for session in dap_store.read(cx).sessions().cloned().collect::<Vec<_>>() {
                editor
                    ._subscriptions
                    .push(cx.subscribe(&session, Self::on_debug_session_event));
            }
        }

        // skip adding the initial selection to selection history
        editor.selection_history.mode = SelectionHistoryMode::Skipping;
        editor.end_selection(window, cx);
        editor.selection_history.mode = SelectionHistoryMode::Normal;

        editor.scroll_manager.show_scrollbars(window, cx);
        jsx_tag_auto_close::refresh_enabled_in_any_buffer(&mut editor, &multi_buffer, cx);

        if full_mode {
            let should_auto_hide_scrollbars = cx.should_auto_hide_scrollbars();
            cx.set_global(ScrollbarAutoHide(should_auto_hide_scrollbars));

            if editor.git_blame_inline_enabled {
                editor.start_git_blame_inline(false, window, cx);
            }

            editor.go_to_active_debug_line(window, cx);

            editor.minimap =
                editor.create_minimap(EditorSettings::get_global(cx).minimap, window, cx);
            editor.colors = Some(LspColorData::new(cx));
            editor.use_document_folding_ranges = true;
            editor.inlay_hints = Some(LspInlayHintData::new(inlay_hint_settings));
            if editor.enable_code_lens && EditorSettings::get_global(cx).code_lens.inline() {
                editor.code_lens = Some(CodeLensState::default());
            }

            if let Some(buffer) = multi_buffer.read(cx).as_singleton() {
                editor.register_buffer(buffer.read(cx).remote_id(), cx);
            }
            editor.report_editor_event(ReportEditorEvent::EditorOpened, None, cx);
        }

        editor
    }

    pub fn display_snapshot(&self, cx: &mut App) -> DisplaySnapshot {
        self.display_map.update(cx, |map, cx| map.snapshot(cx))
    }

    pub fn deploy_mouse_context_menu(
        &mut self,
        position: gpui::Point<Pixels>,
        context_menu: Entity<ContextMenu>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.mouse_context_menu = Some(MouseContextMenu::new(
            self,
            crate::mouse_context_menu::MenuPosition::PinnedToScreen(position),
            context_menu,
            window,
            cx,
        ));
    }

    pub fn mouse_menu_is_focused(&self, window: &Window, cx: &App) -> bool {
        self.mouse_context_menu
            .as_ref()
            .is_some_and(|menu| menu.context_menu.focus_handle(cx).is_focused(window))
    }

    pub fn last_bounds(&self) -> Option<&Bounds<Pixels>> {
        self.last_bounds.as_ref()
    }

    pub(crate) fn last_right_margin(&self) -> Pixels {
        self.last_right_margin
    }

    pub(crate) fn last_horizontal_scrollbar_visible(&self) -> bool {
        self.last_horizontal_scrollbar_visible
    }

    pub fn leader_id(&self) -> Option<CollaboratorId> {
        self.leader_id
    }

    pub fn buffer(&self) -> &Entity<MultiBuffer> {
        &self.buffer
    }

    pub fn project(&self) -> Option<&Entity<Project>> {
        self.project.as_ref()
    }

    pub fn workspace(&self) -> Option<Entity<Workspace>> {
        self.workspace.as_ref()?.0.upgrade()
    }

    /// Detaches a task and shows an error notification in the workspace if available,
    /// otherwise just logs the error.
    pub fn detach_and_notify_err<R, E>(
        &self,
        task: Task<Result<R, E>>,
        window: &mut Window,
        cx: &mut App,
    ) where
        E: std::fmt::Debug + std::fmt::Display + 'static,
        R: 'static,
    {
        if let Some(workspace) = self.workspace() {
            task.detach_and_notify_err(workspace.downgrade(), window, cx);
        } else {
            task.detach_and_log_err(cx);
        }
    }

    /// Returns the workspace serialization ID if this editor should be serialized.
    fn workspace_serialization_id(&self, _cx: &App) -> Option<WorkspaceId> {
        self.workspace
            .as_ref()
            .filter(|_| self.should_serialize_buffer())
            .and_then(|workspace| workspace.1)
    }

    pub fn title<'a>(&self, cx: &'a App) -> Cow<'a, str> {
        self.buffer().read(cx).title(cx)
    }

    pub fn snapshot(&self, window: &Window, cx: &mut App) -> EditorSnapshot {
        let git_blame_gutter_max_author_length = self
            .render_git_blame_gutter(cx)
            .then(|| {
                if let Some(blame) = self.blame.as_ref() {
                    let max_author_length =
                        blame.update(cx, |blame, cx| blame.max_author_length(cx));
                    Some(max_author_length)
                } else {
                    None
                }
            })
            .flatten();

        let display_snapshot = self.display_map.update(cx, |map, cx| map.snapshot(cx));

        EditorSnapshot {
            mode: self.mode.clone(),
            show_gutter: self.show_gutter,
            offset_content: self.offset_content,
            show_line_numbers: self.show_line_numbers,
            number_deleted_lines: self.number_deleted_lines,
            show_git_diff_gutter: self.show_git_diff_gutter,
            semantic_tokens_enabled: self.semantic_token_state.enabled(),
            show_code_actions: self.show_code_actions,
            show_runnables: self.show_runnables,
            show_bookmarks: self.show_bookmarks,
            show_breakpoints: self.show_breakpoints,
            git_blame_gutter_max_author_length,
            scroll_anchor: self.scroll_manager.shared_scroll_anchor(cx),
            display_snapshot,
            placeholder_display_snapshot: self
                .placeholder_display_map
                .as_ref()
                .map(|display_map| display_map.update(cx, |map, cx| map.snapshot(cx))),
            ongoing_scroll: self.scroll_manager.ongoing_scroll(),
            is_focused: self.focus_handle.is_focused(window),
            current_line_highlight: self
                .current_line_highlight
                .unwrap_or_else(|| EditorSettings::get_global(cx).current_line_highlight),
            gutter_hovered: self.gutter_hovered,
        }
    }

    pub fn language_at<T: ToOffset>(&self, point: T, cx: &App) -> Option<Arc<Language>> {
        self.buffer.read(cx).language_at(point, cx)
    }

    pub fn file_at<T: ToOffset>(&self, point: T, cx: &App) -> Option<Arc<dyn language::File>> {
        self.buffer.read(cx).read(cx).file_at(point).cloned()
    }

    pub fn active_buffer(&self, cx: &App) -> Option<Entity<Buffer>> {
        let multibuffer = self.buffer.read(cx);
        let snapshot = multibuffer.snapshot(cx);
        let (anchor, _) =
            snapshot.anchor_to_buffer_anchor(self.selections.newest_anchor().head())?;
        multibuffer.buffer(anchor.buffer_id)
    }

    pub fn mode(&self) -> &EditorMode {
        &self.mode
    }

    pub fn set_mode(&mut self, mode: EditorMode) {
        self.mode = mode;
    }

    pub fn collaboration_hub(&self) -> Option<&dyn CollaborationHub> {
        self.collaboration_hub.as_deref()
    }

    pub fn set_collaboration_hub(&mut self, hub: Box<dyn CollaborationHub>) {
        self.collaboration_hub = Some(hub);
    }

    pub fn set_in_project_search(&mut self, in_project_search: bool) {
        self.in_project_search = in_project_search;
    }

    pub fn set_custom_context_menu(
        &mut self,
        f: impl 'static
        + Fn(
            &mut Self,
            DisplayPoint,
            &mut Window,
            &mut Context<Self>,
        ) -> Option<Entity<ui::ContextMenu>>,
    ) {
        self.custom_context_menu = Some(Box::new(f))
    }

    pub fn semantics_provider(&self) -> Option<Rc<dyn SemanticsProvider>> {
        self.semantics_provider.clone()
    }

    pub fn set_semantics_provider(&mut self, provider: Option<Rc<dyn SemanticsProvider>>) {
        self.semantics_provider = provider;
    }

    pub fn placeholder_text(&self, cx: &mut App) -> Option<String> {
        self.placeholder_display_map
            .as_ref()
            .map(|display_map| display_map.update(cx, |map, cx| map.snapshot(cx)).text())
    }

    pub fn set_placeholder_text(
        &mut self,
        placeholder_text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let multibuffer = cx
            .new(|cx| MultiBuffer::singleton(cx.new(|cx| Buffer::local(placeholder_text, cx)), cx));

        let style = window.text_style();

        self.placeholder_display_map = Some(cx.new(|cx| {
            DisplayMap::new(
                multibuffer,
                style.font(),
                style.font_size.to_pixels(window.rem_size()),
                None,
                FILE_HEADER_HEIGHT,
                MULTI_BUFFER_EXCERPT_HEADER_HEIGHT,
                Default::default(),
                DiagnosticSeverity::Off,
                cx,
            )
        }));
        cx.notify();
    }

    pub fn set_cursor_shape(&mut self, cursor_shape: CursorShape, cx: &mut Context<Self>) {
        self.cursor_shape = cursor_shape;

        // Disrupt blink for immediate user feedback that the cursor shape has changed
        self.blink_manager.update(cx, BlinkManager::show_cursor);

        cx.notify();
    }

    pub fn show_cursor(&mut self, cx: &mut Context<Self>) {
        self.blink_manager.update(cx, BlinkManager::show_cursor);
    }

    pub fn cursor_shape(&self) -> CursorShape {
        self.cursor_shape
    }

    pub fn set_cursor_offset_on_selection(&mut self, set_cursor_offset_on_selection: bool) {
        self.cursor_offset_on_selection = set_cursor_offset_on_selection;
    }

    pub fn set_current_line_highlight(
        &mut self,
        current_line_highlight: Option<CurrentLineHighlight>,
    ) {
        self.current_line_highlight = current_line_highlight;
    }

    pub fn set_collapse_matches(&mut self, collapse_matches: bool) {
        self.collapse_matches = collapse_matches;
    }

    pub fn range_for_match<T: std::marker::Copy>(&self, range: &Range<T>) -> Range<T> {
        if self.collapse_matches {
            return range.start..range.start;
        }
        range.clone()
    }

    pub fn clip_at_line_ends(&mut self, cx: &mut Context<Self>) -> bool {
        self.display_map.read(cx).clip_at_line_ends
    }

    pub fn set_clip_at_line_ends(&mut self, clip: bool, cx: &mut Context<Self>) {
        if self.display_map.read(cx).clip_at_line_ends != clip {
            self.display_map
                .update(cx, |map, _| map.clip_at_line_ends = clip);
        }
    }

    pub fn capability(&self, cx: &App) -> Capability {
        if self.read_only {
            Capability::ReadOnly
        } else {
            self.buffer.read(cx).capability()
        }
    }

    pub fn read_only(&self, cx: &App) -> bool {
        self.read_only || self.buffer.read(cx).read_only()
    }

    pub fn set_read_only(&mut self, read_only: bool) {
        self.read_only = read_only;
    }

    pub fn set_use_selection_highlight(&mut self, highlight: bool) {
        self.use_selection_highlight = highlight;
    }

    pub fn set_should_serialize(&mut self, should_serialize: bool, cx: &App) {
        self.buffer_serialization = should_serialize.then(|| {
            BufferSerialization::new(
                ProjectSettings::get_global(cx)
                    .session
                    .restore_unsaved_buffers,
            )
        })
    }

    fn should_serialize_buffer(&self) -> bool {
        self.buffer_serialization.is_some()
    }

    pub fn set_use_modal_editing(&mut self, to: bool) {
        self.use_modal_editing = to;
    }

    pub fn use_modal_editing(&self) -> bool {
        self.use_modal_editing
    }

    /// Inserted text is normalized to LF line endings before being applied.
    /// Normalize before measuring inserted text for post-edit offsets.
    pub fn edit<I, S, T>(&mut self, edits: I, cx: &mut Context<Self>)
    where
        I: IntoIterator<Item = (Range<S>, T)>,
        S: ToOffset,
        T: Into<Arc<str>>,
    {
        if self.read_only(cx) {
            return;
        }

        self.buffer
            .update(cx, |buffer, cx| buffer.edit(edits, None, cx));
    }

    pub fn edit_with_autoindent<I, S, T>(&mut self, edits: I, cx: &mut Context<Self>)
    where
        I: IntoIterator<Item = (Range<S>, T)>,
        S: ToOffset,
        T: Into<Arc<str>>,
    {
        if self.read_only(cx) {
            return;
        }

        self.buffer.update(cx, |buffer, cx| {
            buffer.edit(edits, self.autoindent_mode.clone(), cx)
        });
    }

    pub fn edit_with_block_indent<I, S, T>(
        &mut self,
        edits: I,
        original_indent_columns: Vec<Option<u32>>,
        cx: &mut Context<Self>,
    ) where
        I: IntoIterator<Item = (Range<S>, T)>,
        S: ToOffset,
        T: Into<Arc<str>>,
    {
        if self.read_only(cx) {
            return;
        }

        self.buffer.update(cx, |buffer, cx| {
            buffer.edit(
                edits,
                Some(AutoindentMode::Block {
                    original_indent_columns,
                }),
                cx,
            )
        });
    }

    pub fn cancel(&mut self, _: &Cancel, window: &mut Window, cx: &mut Context<Self>) {
        self.selection_mark_mode = false;
        self.selection_drag_state = SelectionDragState::None;

        if self.dismiss_menus_and_popups(true, window, cx) {
            cx.notify();
            return;
        }
        if self.clear_expanded_diff_hunks(cx) {
            cx.notify();
            return;
        }
        if self.show_git_blame_gutter {
            self.show_git_blame_gutter = false;
            cx.notify();
            return;
        }

        if self.mode.is_full()
            && self.change_selections(Default::default(), window, cx, |s| s.try_cancel())
        {
            cx.notify();
            return;
        }

        cx.propagate();
    }

    pub fn dismiss_menus_and_popups(
        &mut self,
        is_user_requested: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut dismissed = false;

        dismissed |= self.take_rename(false, window, cx).is_some();
        dismissed |= self.hide_blame_popover(true, cx);
        dismissed |= hide_hover(self, cx);
        dismissed |= self.hide_signature_help(cx, SignatureHelpHiddenBy::Escape);
        dismissed |= self.hide_context_menu(window, cx).is_some();
        dismissed |= self.mouse_context_menu.take().is_some();
        dismissed |= is_user_requested
            && self.discard_edit_prediction(EditPredictionDiscardReason::Rejected, cx);
        dismissed |= self.snippet_stack.pop().is_some();
        if self.diff_review_drag_state.is_some() {
            self.cancel_diff_review_drag(cx);
            dismissed = true;
        }
        if !self.diff_review_overlays.is_empty() {
            self.dismiss_all_diff_review_overlays(cx);
            dismissed = true;
        }

        if self.mode.is_full() && self.has_active_diagnostic_group() {
            self.dismiss_diagnostics(cx);
            dismissed = true;
        }

        dismissed
    }

    fn open_transaction_for_hidden_buffers(
        workspace: Entity<Workspace>,
        transaction: ProjectTransaction,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if transaction.0.is_empty() {
            return;
        }

        let edited_buffers_already_open = {
            let other_editors: Vec<Entity<Editor>> = workspace
                .read(cx)
                .panes()
                .iter()
                .flat_map(|pane| pane.read(cx).items_of_type::<Editor>())
                .filter(|editor| editor.entity_id() != cx.entity_id())
                .collect();

            transaction.0.keys().all(|buffer| {
                other_editors.iter().any(|editor| {
                    let multi_buffer = editor.read(cx).buffer();
                    multi_buffer.read(cx).is_singleton()
                        && multi_buffer
                            .read(cx)
                            .as_singleton()
                            .map_or(false, |singleton| {
                                singleton.entity_id() == buffer.entity_id()
                            })
                })
            })
        };
        if !edited_buffers_already_open {
            let workspace = workspace.downgrade();
            cx.defer_in(window, move |_, window, cx| {
                cx.spawn_in(window, async move |editor, cx| {
                    Self::open_project_transaction(&editor, workspace, transaction, title, cx)
                        .await
                        .ok()
                })
                .detach();
            });
        }
    }

    pub async fn open_project_transaction(
        editor: &WeakEntity<Editor>,
        workspace: WeakEntity<Workspace>,
        transaction: ProjectTransaction,
        title: String,
        cx: &mut AsyncWindowContext,
    ) -> Result<()> {
        let mut entries = transaction.0.into_iter().collect::<Vec<_>>();
        cx.update(|_, cx| {
            entries.sort_unstable_by_key(|(buffer, _)| {
                buffer.read(cx).file().map(|f| f.path().clone())
            });
        })?;
        if entries.is_empty() {
            return Ok(());
        }

        // If the project transaction's edits are all contained within this editor, then
        // avoid opening a new editor to display them.

        if let [(buffer, transaction)] = &*entries {
            let cursor_excerpt = editor.update(cx, |editor, cx| {
                let snapshot = editor.buffer().read(cx).snapshot(cx);
                let head = editor.selections.newest_anchor().head();
                let (buffer_snapshot, excerpt_range) = snapshot.excerpt_containing(head..head)?;
                if buffer_snapshot.remote_id() != buffer.read(cx).remote_id() {
                    return None;
                }
                Some(excerpt_range)
            })?;

            if let Some(excerpt_range) = cursor_excerpt {
                let all_edits_within_excerpt = buffer.read_with(cx, |buffer, _| {
                    let excerpt_range = excerpt_range.context.to_offset(buffer);
                    buffer
                        .edited_ranges_for_transaction::<usize>(transaction)
                        .all(|range| {
                            excerpt_range.start <= range.start && excerpt_range.end >= range.end
                        })
                });

                if all_edits_within_excerpt {
                    return Ok(());
                }
            }
        }

        let mut ranges_to_highlight = Vec::new();
        let excerpt_buffer = cx.new(|cx| {
            let mut multibuffer = MultiBuffer::new(Capability::ReadWrite).with_title(title);
            for (buffer_handle, transaction) in &entries {
                let edited_ranges = buffer_handle
                    .read(cx)
                    .edited_ranges_for_transaction::<Point>(transaction)
                    .collect::<Vec<_>>();
                multibuffer.set_excerpts_for_path(
                    PathKey::for_buffer(buffer_handle, cx),
                    buffer_handle.clone(),
                    edited_ranges.clone(),
                    multibuffer_context_lines(cx),
                    cx,
                );
                let snapshot = multibuffer.snapshot(cx);
                let buffer_snapshot = buffer_handle.read(cx).snapshot();
                ranges_to_highlight.extend(edited_ranges.into_iter().filter_map(|range| {
                    let text_range = buffer_snapshot.anchor_range_inside(range);
                    let start = snapshot.anchor_in_buffer(text_range.start)?;
                    let end = snapshot.anchor_in_buffer(text_range.end)?;
                    Some(start..end)
                }));
            }
            multibuffer.push_transaction(entries.iter().map(|(b, t)| (b, t)), cx);
            multibuffer
        });

        workspace.update_in(cx, |workspace, window, cx| {
            let project = workspace.project().clone();
            let editor =
                cx.new(|cx| Editor::for_multibuffer(excerpt_buffer, Some(project), window, cx));
            workspace.add_item_to_active_pane(Box::new(editor.clone()), None, true, window, cx);
            editor.update(cx, |editor, cx| {
                editor.highlight_background(
                    HighlightKey::Editor,
                    &ranges_to_highlight,
                    |_, theme| theme.colors().editor_highlighted_line_background,
                    cx,
                );
            });
        })?;

        Ok(())
    }

    pub fn has_mouse_context_menu(&self) -> bool {
        self.mouse_context_menu.is_some()
    }

    fn refresh_document_highlights(&mut self, cx: &mut Context<Self>) -> Option<()> {
        if self.pending_rename.is_some() {
            return None;
        }

        let provider = self.semantics_provider.clone()?;
        let buffer = self.buffer.read(cx);
        let newest_selection = self.selections.newest_anchor().clone();
        let cursor_position = newest_selection.head();
        let (cursor_buffer, cursor_buffer_position) =
            buffer.text_anchor_for_position(cursor_position, cx)?;
        let (tail_buffer, tail_buffer_position) =
            buffer.text_anchor_for_position(newest_selection.tail(), cx)?;
        if cursor_buffer != tail_buffer {
            return None;
        }

        let snapshot = cursor_buffer.read(cx).snapshot();
        let word_ranges = cx.background_spawn(async move {
            // this might look odd to put on the background thread, but
            // `surrounding_word` can be quite expensive as it calls into
            // tree-sitter language scopes
            let (start_word_range, _) = snapshot.surrounding_word(cursor_buffer_position, None);
            let (end_word_range, _) = snapshot.surrounding_word(tail_buffer_position, None);
            (start_word_range, end_word_range)
        });

        let debounce = EditorSettings::get_global(cx).lsp_highlight_debounce.0;
        self.document_highlights_task = Some(cx.spawn(async move |this, cx| {
            let (start_word_range, end_word_range) = word_ranges.await;
            if start_word_range != end_word_range {
                this.update(cx, |this, cx| {
                    this.document_highlights_task.take();
                    this.clear_background_highlights(HighlightKey::DocumentHighlightRead, cx);
                    this.clear_background_highlights(HighlightKey::DocumentHighlightWrite, cx);
                })
                .ok();
                return;
            }
            cx.background_executor()
                .timer(Duration::from_millis(debounce))
                .await;

            let highlights = if let Some(highlights) = cx.update(|cx| {
                provider.document_highlights(&cursor_buffer, cursor_buffer_position, cx)
            }) {
                highlights.await.log_err()
            } else {
                None
            };

            if let Some(highlights) = highlights {
                this.update(cx, |this, cx| {
                    if this.pending_rename.is_some() {
                        return;
                    }

                    let buffer = this.buffer.read(cx);
                    if buffer
                        .text_anchor_for_position(cursor_position, cx)
                        .is_none_or(|(buffer, _)| buffer != cursor_buffer)
                    {
                        return;
                    }

                    let mut write_ranges = Vec::new();
                    let mut read_ranges = Vec::new();
                    let multibuffer_snapshot = buffer.snapshot(cx);
                    for highlight in highlights {
                        for range in
                            multibuffer_snapshot.buffer_range_to_excerpt_ranges(highlight.range)
                        {
                            if highlight.kind == lsp::DocumentHighlightKind::WRITE {
                                write_ranges.push(range);
                            } else {
                                read_ranges.push(range);
                            }
                        }
                    }

                    this.highlight_background(
                        HighlightKey::DocumentHighlightRead,
                        &read_ranges,
                        |_, theme| theme.colors().editor_document_highlight_read_background,
                        cx,
                    );
                    this.highlight_background(
                        HighlightKey::DocumentHighlightWrite,
                        &write_ranges,
                        |_, theme| theme.colors().editor_document_highlight_write_background,
                        cx,
                    );
                    cx.notify();
                })
                .log_err();
            }
        }));
        None
    }

    fn prepare_highlight_query_from_selection(
        &mut self,
        snapshot: &DisplaySnapshot,
        cx: &mut Context<Editor>,
    ) -> Option<(String, Range<Anchor>)> {
        if matches!(self.mode, EditorMode::SingleLine) {
            return None;
        }
        if !self.use_selection_highlight || !EditorSettings::get_global(cx).selection_highlight {
            return None;
        }
        // When the current selection was set by search navigation, suppress selection
        // occurrence highlights to avoid confusing non-matching occurrences with actual
        // search results (e.g. `^something` matches 3 line-start occurrences, but a
        // literal highlight would also mark a mid-line "something" that never matched
        // the regex). A manual selection made by the user clears this flag, restoring
        // the normal occurrence-highlight behavior.
        if self.last_selection_from_search
            && self.has_background_highlights(HighlightKey::BufferSearchHighlights)
        {
            return None;
        }
        if self.selections.count() != 1 || self.selections.line_mode() {
            return None;
        }
        let selection = self.selections.newest::<Point>(&snapshot);
        // If the selection spans multiple rows OR it is empty
        if selection.start.row != selection.end.row
            || selection.start.column == selection.end.column
        {
            return None;
        }
        let selection_anchor_range = selection.range().to_anchors(snapshot.buffer_snapshot());
        let query = snapshot
            .buffer_snapshot()
            .text_for_range(selection_anchor_range.clone())
            .collect::<String>();
        if query.trim().is_empty() {
            return None;
        }
        Some((query, selection_anchor_range))
    }

    #[ztracing::instrument(skip_all)]
    fn update_selection_occurrence_highlights(
        &mut self,
        multi_buffer_snapshot: MultiBufferSnapshot,
        query_text: String,
        query_range: Range<Anchor>,
        multi_buffer_range_to_query: Range<Point>,
        use_debounce: bool,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) -> Task<()> {
        cx.spawn_in(window, async move |editor, cx| {
            if use_debounce {
                cx.background_executor()
                    .timer(SELECTION_HIGHLIGHT_DEBOUNCE_TIMEOUT)
                    .await;
            }
            let match_task = cx.background_spawn(async move {
                let buffer_ranges = multi_buffer_snapshot
                    .range_to_buffer_ranges(
                        multi_buffer_range_to_query.start..multi_buffer_range_to_query.end,
                    )
                    .into_iter()
                    .filter(|(_, excerpt_visible_range, _)| !excerpt_visible_range.is_empty());
                let mut match_ranges = Vec::new();
                let Ok(regex) = project::search::SearchQuery::text(
                    query_text,
                    false,
                    false,
                    false,
                    Default::default(),
                    Default::default(),
                    false,
                    None,
                ) else {
                    return Vec::default();
                };
                let query_range = query_range.to_anchors(&multi_buffer_snapshot);
                for (buffer_snapshot, search_range, _) in buffer_ranges {
                    match_ranges.extend(
                        regex
                            .search(
                                &buffer_snapshot,
                                Some(search_range.start.0..search_range.end.0),
                            )
                            .await
                            .into_iter()
                            .filter_map(|match_range| {
                                let match_start = buffer_snapshot
                                    .anchor_after(search_range.start + match_range.start);
                                let match_end = buffer_snapshot
                                    .anchor_before(search_range.start + match_range.end);
                                {
                                    let range = multi_buffer_snapshot
                                        .anchor_in_buffer(match_start)?
                                        ..multi_buffer_snapshot.anchor_in_buffer(match_end)?;
                                    Some(range).filter(|match_anchor_range| {
                                        match_anchor_range != &query_range
                                    })
                                }
                            }),
                    );
                }
                match_ranges
            });
            let match_ranges = match_task.await;
            editor
                .update_in(cx, |editor, _, cx| {
                    if use_debounce {
                        editor.clear_background_highlights(HighlightKey::SelectedTextHighlight, cx);
                        editor.debounced_selection_highlight_complete = true;
                    } else if editor.debounced_selection_highlight_complete {
                        return;
                    }
                    if !match_ranges.is_empty() {
                        editor.highlight_background(
                            HighlightKey::SelectedTextHighlight,
                            &match_ranges,
                            |_, theme| theme.colors().editor_document_highlight_bracket_background,
                            cx,
                        )
                    }
                })
                .log_err();
        })
    }

    #[ztracing::instrument(skip_all)]
    fn refresh_outline_symbols_at_cursor(&mut self, cx: &mut Context<Editor>) {
        if !self.lsp_data_enabled() {
            return;
        }
        let cursor = self.selections.newest_anchor().head();
        let multi_buffer_snapshot = self.buffer().read(cx).snapshot(cx);

        if self.uses_lsp_document_symbols(cursor, &multi_buffer_snapshot, cx) {
            self.outline_symbols_at_cursor =
                self.lsp_symbols_at_cursor(cursor, &multi_buffer_snapshot, cx);
            cx.emit(EditorEvent::OutlineSymbolsChanged);
            cx.notify();
        } else {
            let syntax = cx.theme().syntax().clone();
            let background_task = cx.background_spawn(async move {
                multi_buffer_snapshot.symbols_containing(cursor, Some(&syntax))
            });
            self.refresh_outline_symbols_at_cursor_at_cursor_task =
                cx.spawn(async move |this, cx| {
                    let symbols = background_task.await;
                    this.update(cx, |this, cx| {
                        this.outline_symbols_at_cursor = symbols;
                        cx.emit(EditorEvent::OutlineSymbolsChanged);
                        cx.notify();
                    })
                    .ok();
                });
        }
    }

    #[ztracing::instrument(skip_all)]
    fn refresh_selected_text_highlights(
        &mut self,
        snapshot: &DisplaySnapshot,
        on_buffer_edit: bool,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) {
        let Some((query_text, query_range)) =
            self.prepare_highlight_query_from_selection(snapshot, cx)
        else {
            self.clear_background_highlights(HighlightKey::SelectedTextHighlight, cx);
            self.quick_selection_highlight_task.take();
            self.debounced_selection_highlight_task.take();
            self.debounced_selection_highlight_complete = false;
            return;
        };
        let display_snapshot = self.display_map.update(cx, |map, cx| map.snapshot(cx));
        let multi_buffer_snapshot = self.buffer().read(cx).snapshot(cx);
        let query_changed = self
            .quick_selection_highlight_task
            .as_ref()
            .is_none_or(|(prev_anchor_range, _)| prev_anchor_range != &query_range);
        if query_changed {
            self.debounced_selection_highlight_complete = false;
        }
        if on_buffer_edit || query_changed {
            self.quick_selection_highlight_task = Some((
                query_range.clone(),
                self.update_selection_occurrence_highlights(
                    snapshot.buffer.clone(),
                    query_text.clone(),
                    query_range.clone(),
                    self.multi_buffer_visible_range(&display_snapshot, cx),
                    false,
                    window,
                    cx,
                ),
            ));
        }
        if on_buffer_edit
            || self
                .debounced_selection_highlight_task
                .as_ref()
                .is_none_or(|(prev_anchor_range, _)| prev_anchor_range != &query_range)
        {
            let multi_buffer_start = multi_buffer_snapshot
                .anchor_before(MultiBufferOffset(0))
                .to_point(&multi_buffer_snapshot);
            let multi_buffer_end = multi_buffer_snapshot
                .anchor_after(multi_buffer_snapshot.len())
                .to_point(&multi_buffer_snapshot);
            let multi_buffer_full_range = multi_buffer_start..multi_buffer_end;
            self.debounced_selection_highlight_task = Some((
                query_range.clone(),
                self.update_selection_occurrence_highlights(
                    snapshot.buffer.clone(),
                    query_text,
                    query_range,
                    multi_buffer_full_range,
                    true,
                    window,
                    cx,
                ),
            ));
        }
    }

    pub fn multi_buffer_visible_range(
        &self,
        display_snapshot: &DisplaySnapshot,
        cx: &App,
    ) -> Range<Point> {
        let visible_start = self
            .scroll_manager
            .native_anchor(display_snapshot, cx)
            .anchor
            .to_point(display_snapshot.buffer_snapshot())
            .to_display_point(display_snapshot);

        let mut target_end = visible_start;
        *target_end.row_mut() += self.visible_line_count().unwrap_or(0.).ceil() as u32;

        visible_start.to_point(display_snapshot)
            ..display_snapshot
                .clip_point(target_end, Bias::Right)
                .to_point(display_snapshot)
    }
}

impl Focusable for Editor {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Editor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        EditorElement::new(&cx.entity(), self.create_style(cx))
    }
}

const UPDATE_DEBOUNCE: Duration = Duration::from_millis(50);
