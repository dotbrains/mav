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
#[path = "editor/constructors.rs"]
mod constructors;
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
#[path = "editor/initializer_base_subscriptions.rs"]
mod initializer_base_subscriptions;
#[path = "editor/initializer_display.rs"]
mod initializer_display;
#[path = "editor/initializer_finish.rs"]
mod initializer_finish;
#[path = "editor/initializer_focus.rs"]
mod initializer_focus;
#[path = "editor/initializer_internal.rs"]
mod initializer_internal;
#[path = "editor/initializer_project.rs"]
mod initializer_project;
#[path = "editor/initializer_subscriptions.rs"]
mod initializer_subscriptions;
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
