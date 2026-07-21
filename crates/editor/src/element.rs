mod action_registration;
mod auto_height;
mod autoscroll_layout;
mod blame_entries;
mod blame_layouts;
mod block_layout;
mod block_render;
mod breadcrumbs;
mod context_menu_layout;
mod cursor_layout;
mod cursor_popovers;
mod cursor_scrollbar_paint;
mod cursor_selections;
mod cursor_surface;
mod diff_hunk_controls;
mod document_colors;
mod editor_layout;
mod element_helpers;
mod element_impl;
mod final_surface_layout;
mod guides_layout;
mod gutter;
mod gutter_controls;
mod gutter_indicators;
mod gutter_paint;
mod header;
mod highlight_inputs;
mod highlighted_range;
mod hover_popovers;
mod initial_prepaint_layout;
mod inline_decorations;
mod layout_data;
mod layout_primitives;
mod lifecycle;
mod line_builder;
mod line_layout_model;
mod line_metrics;
mod line_numbers;
mod line_paint;
mod line_setup;
mod metrics_layout;
mod minimap;
mod mouse;
mod navigation_overlay;
mod paint;
mod paint_background;
mod paint_helpers;
mod position_map;
mod post_scroll_prepaint;
mod pre_scroll_layout;
mod prepaint_helpers;
mod register_actions;
mod request_layout;
mod row_activity;
mod row_highlights;
mod scroll_position_layout;
mod scrollbar_information;
mod scrollbar_layouts;
mod scrollbar_markers;
mod selection_inputs;
mod signature_help_layout;
mod snapshot_layout;
mod sticky_header_layout;
mod surface_layout;
mod visible_rows;
mod word_diff_layout;

pub use action_registration::register_action;
use auto_height::{calculate_wrap_width, compute_auto_height_layout};
use blame_entries::render_inline_blame_entry;
pub use breadcrumbs::render_breadcrumb_text;
pub use cursor_layout::{CursorLayout, CursorName};
pub use editor_layout::layout_line;
use editor_layout::{CursorPopoverType, EditorLayout, IndentGuideLayout};
use gutter::{Gutter, gutter_bounds};
#[cfg(test)]
pub(crate) use header::StickyHeader;
pub(crate) use header::{header_jump_data, render_buffer_header};
pub use highlighted_range::{HighlightedRange, HighlightedRangeLine};
pub(crate) use layout_data::BlockLayout;
use layout_data::{
    ColoredRange, ContextMenuLayout, CreaseTrailerLayout, RenderBlocksOutput,
    ScrollbarLayoutInformation,
};
use layout_primitives::{InlineBlameLayout, LineHighlightSpec, LineNumberStyle, SelectionLayout};
pub use lifecycle::{EditorElement, SplitSide};
pub(crate) use line_layout_model::{Invisible, LineFragment, LineWithInvisibles};
pub(super) use line_numbers::{LineNumberLayout, LineNumberSegment};
use navigation_overlay::NavigationOverlayPaintCommand;
pub use position_map::PointForPosition;
pub(crate) use position_map::PositionMap;
use request_layout::EditorRequestLayoutState;
use scrollbar_layouts::{EditorScrollbars, MinimapLayout, ScrollbarLayout};

use crate::{
    BUFFER_HEADER_PADDING, BlockId, ChunkRendererContext, ChunkReplacement, CodeActionSource,
    ConflictsOurs, ConflictsOursMarker, ConflictsOuter, ConflictsTheirs, ConflictsTheirsMarker,
    ContextMenuPlacement, CursorShape, CustomBlockId, DisplayDiffHunk, DisplayPoint, DisplayRow,
    EditDisplayMode, EditPrediction, Editor, EditorMode, EditorSettings, EditorSnapshot,
    EditorStyle, FILE_HEADER_HEIGHT, FocusedBlock, GutterDimensions, HalfPageDown, HalfPageUp,
    HandleInput, HoveredCursor, InlayHintRefreshReason, LineDown, LineHighlight, LineUp,
    MAX_LINE_LEN, MINIMAP_FONT_SIZE, PageDown, PageUp, Point, RowExt, RowRangeExt, Selection,
    SelectionDragState, SizingBehavior, SoftWrap, ToPoint,
    code_context_menus::{CodeActionsMenu, MENU_ASIDE_MAX_WIDTH, MENU_ASIDE_MIN_WIDTH, MENU_GAP},
    column_pixels,
    display_map::{
        Block, BlockContext, BlockStyle, ChunkRendererId, DisplaySnapshot, EditorMargins,
        HighlightKey, HighlightedChunk, ToDisplayPoint,
    },
    editor_settings::{
        CurrentLineHighlight, DocumentColorsRenderMode, Minimap, MinimapThumb, MinimapThumbBorder,
        ScrollBeyondLastLine, ScrollbarAxes, ScrollbarDiagnostics, ShowMinimap,
    },
    hover_popover::{
        self, HOVER_POPOVER_GAP, MIN_POPOVER_CHARACTER_WIDTH, MIN_POPOVER_LINE_HEIGHT,
        POPOVER_RIGHT_OFFSET,
    },
    inlay_hint_settings,
    scroll::{
        ActiveScrollbarState, Autoscroll, ScrollOffset, ScrollPixelOffset, ScrollbarThumbState,
        autoscroll::NeedsHorizontalAutoscroll, scroll_amount::ScrollAmount,
    },
};
use buffer_diff::{DiffHunkStatus, DiffHunkStatusKind};
use collections::{BTreeMap, HashMap, HashSet};
use feature_flags::{DiffReviewFeatureFlag, FeatureFlagAppExt as _};
use git::blame::BlameEntry;
use gpui::{
    Action, Along, AnyElement, App, AppContext, AvailableSpace, Axis as ScrollbarAxis, BorderStyle,
    Bounds, ClipboardItem, ContentMask, Context, Corners, CursorStyle, DispatchPhase, Edges,
    Element, ElementInputHandler, Entity, Focusable as _, Font, FontId, FontWeight,
    GlobalElementId, Hitbox, HitboxBehavior, Hsla, InteractiveElement, IntoElement, IsZero,
    ModifiersChangedEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad,
    ParentElement, Pixels, ScrollHandle, ShapedLine, SharedString, Size,
    StatefulInteractiveElement, Style, Styled, StyledText, TaskExt, TextAlign, TextRun,
    TextStyleRefinement, WeakEntity, Window, div, fill, outline, pattern_slash, point, px, quad,
    relative, size, solid_background, transparent_black,
};
use itertools::Itertools;
use language::{
    HighlightedText, IndentGuideSettings, LanguageAwareStyling,
    language_settings::ShowWhitespaceSetting,
};
use markdown::Markdown;
use multi_buffer::{
    Anchor, ExpandExcerptDirection, ExpandInfo, MultiBufferOffset, MultiBufferPoint,
    MultiBufferRow, RowInfo,
};

use project::{
    debugger::breakpoint_store::{Breakpoint, BreakpointSessionState},
    project_settings::ProjectSettings,
};
use settings::{
    GitGutterSetting, GitHunkStyleSetting, IndentGuideBackgroundColoring, IndentGuideColoring,
    Settings,
};
use smallvec::{SmallVec, smallvec};
use std::{
    any::TypeId,
    borrow::Cow,
    cmp::{self, Ordering},
    fmt::{self, Write},
    iter, mem,
    ops::{Deref, Range},
    rc::Rc,
    sync::Arc,
    time::Duration,
};
use sum_tree::Bias;
use text::BufferId;
use theme::{ActiveTheme, Appearance, PlayerColor};
use theme_settings::BufferLineHeight;
use ui::utils::ensure_minimum_contrast;
use ui::{ButtonLike, POPOVER_Y_PADDING, Tooltip, prelude::*, scrollbars::ShowScrollbar};
use unicode_segmentation::UnicodeSegmentation;
use util::{ResultExt, debug_panic};
use workspace::{
    CollaboratorId, ItemHandle, Workspace,
    item::{Item, ItemBufferKind},
};

#[cfg(test)]
#[path = "element/tests/mod.rs"]
mod tests;
