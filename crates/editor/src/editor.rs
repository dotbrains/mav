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
#[path = "editor/api_state.rs"]
mod api_state;
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
#[path = "editor/edit_api.rs"]
mod edit_api;
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
#[path = "editor/highlight_refresh.rs"]
mod highlight_refresh;
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
#[path = "editor/outline_refresh.rs"]
mod outline_refresh;
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
#[path = "editor/sticky_headers.rs"]
mod sticky_headers;
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

    pub fn has_mouse_context_menu(&self) -> bool {
        self.mouse_context_menu.is_some()
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
