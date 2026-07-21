mod action_registration;
mod auto_height;
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
mod diff_hunk_controls;
mod document_colors;
mod editor_layout;
mod element_helpers;
mod guides_layout;
mod gutter;
mod gutter_controls;
mod gutter_paint;
mod header;
mod highlighted_range;
mod hover_popovers;
mod inline_decorations;
mod layout_data;
mod layout_primitives;
mod line_builder;
mod line_layout_model;
mod line_metrics;
mod line_numbers;
mod line_paint;
mod minimap;
mod mouse;
mod navigation_overlay;
mod paint_background;
mod paint_helpers;
mod position_map;
mod prepaint_helpers;
mod register_actions;
mod scrollbar_layouts;
mod scrollbar_markers;
mod signature_help_layout;
mod word_diff_layout;

pub use action_registration::register_action;
use auto_height::{calculate_wrap_width, compute_auto_height_layout};
use blame_entries::render_inline_blame_entry;
pub use breadcrumbs::render_breadcrumb_text;
pub use cursor_layout::{CursorLayout, CursorName};
pub use editor_layout::layout_line;
use editor_layout::{CursorPopoverType, EditorLayout, IndentGuideLayout};
use gutter::Gutter;
#[cfg(test)]
pub(crate) use header::StickyHeader;
pub(crate) use header::{header_jump_data, render_buffer_header};
pub use highlighted_range::{HighlightedRange, HighlightedRangeLine};
pub(crate) use layout_data::BlockLayout;
use layout_data::{
    ColoredRange, ContextMenuLayout, CreaseTrailerLayout, ScrollbarLayoutInformation,
};
use layout_primitives::{InlineBlameLayout, LineHighlightSpec, LineNumberStyle, SelectionLayout};
pub(crate) use line_layout_model::{Invisible, LineFragment, LineWithInvisibles};
pub(super) use line_numbers::{LineNumberLayout, LineNumberSegment};
use navigation_overlay::NavigationOverlayPaintCommand;
pub use position_map::PointForPosition;
pub(crate) use position_map::PositionMap;
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
        ActiveScrollbarState, ScrollOffset, ScrollPixelOffset, ScrollbarThumbState,
        scroll_amount::ScrollAmount,
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
    cell::Cell,
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

#[derive(Default)]
struct RenderBlocksOutput {
    // We store spacer blocks separately because they paint in a different order
    // (spacers -> indent guides -> non-spacers)
    non_spacer_blocks: Vec<BlockLayout>,
    spacer_blocks: Vec<BlockLayout>,
    row_block_types: HashMap<DisplayRow, bool>,
    resized_blocks: Option<HashMap<CustomBlockId, u32>>,
}

pub struct EditorElement {
    editor: Entity<Editor>,
    style: EditorStyle,
    split_side: Option<SplitSide>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitSide {
    Left,
    Right,
}

impl EditorElement {
    pub fn new(editor: &Entity<Editor>, style: EditorStyle) -> Self {
        Self {
            editor: editor.clone(),
            style,
            split_side: None,
        }
    }

    pub fn set_split_side(&mut self, side: SplitSide) {
        self.split_side = Some(side);
    }

    fn register_key_listeners(&self, window: &mut Window, _: &mut App, layout: &EditorLayout) {
        let position_map = layout.position_map.clone();
        window.on_key_event({
            let editor = self.editor.clone();
            move |event: &ModifiersChangedEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble {
                    return;
                }
                editor.update(cx, |editor, cx| {
                    let inlay_hint_settings = inlay_hint_settings(
                        editor.selections.newest_anchor().head(),
                        &editor.buffer.read(cx).snapshot(cx),
                        cx,
                    );

                    if let Some(inlay_modifiers) = inlay_hint_settings
                        .toggle_on_modifiers_press
                        .as_ref()
                        .filter(|modifiers| modifiers.modified())
                    {
                        editor.refresh_inlay_hints(
                            InlayHintRefreshReason::ModifiersChanged(
                                inlay_modifiers == &event.modifiers,
                            ),
                            cx,
                        );
                    }

                    if editor.hover_state.focused(window, cx) {
                        return;
                    }

                    editor.handle_modifiers_changed(event.modifiers, &position_map, window, cx);
                })
            }
        });
    }

    fn editor_with_selections(&self, cx: &App) -> Option<Entity<Editor>> {
        if let EditorMode::Minimap { parent } = self.editor.read(cx).mode() {
            parent.upgrade()
        } else {
            Some(self.editor.clone())
        }
    }
}

#[derive(Default)]
pub struct EditorRequestLayoutState {
    // We use prepaint depth to limit the number of times prepaint is
    // called recursively. We need this so that we can update stale
    // data for e.g. block heights in block map.
    prepaint_depth: Rc<Cell<usize>>,
}

impl EditorRequestLayoutState {
    // In ideal conditions we only need one more subsequent prepaint call for resize to take effect.
    // i.e. MAX_PREPAINT_DEPTH = 2, but placing near blocks can expose more lines from below, and
    // we end up querying blocks for those lines too in subsequent renders.
    // Setting MAX_PREPAINT_DEPTH = 3, passes all tests. Just to be on the safe side we set it to 5, so
    // that subsequent shrinking does not lead to incorrect block placing.
    const MAX_PREPAINT_DEPTH: usize = 5;

    fn increment_prepaint_depth(&self) -> EditorPrepaintGuard {
        let depth = self.prepaint_depth.get();
        self.prepaint_depth.set(depth + 1);
        EditorPrepaintGuard {
            prepaint_depth: self.prepaint_depth.clone(),
        }
    }

    fn has_remaining_prepaint_depth(&self) -> bool {
        self.prepaint_depth.get() < Self::MAX_PREPAINT_DEPTH
    }
}

struct EditorPrepaintGuard {
    prepaint_depth: Rc<Cell<usize>>,
}

impl Drop for EditorPrepaintGuard {
    fn drop(&mut self) {
        let depth = self.prepaint_depth.get();
        self.prepaint_depth.set(depth.saturating_sub(1));
    }
}

impl Element for EditorElement {
    type RequestLayoutState = EditorRequestLayoutState;
    type PrepaintState = EditorLayout;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (gpui::LayoutId, Self::RequestLayoutState) {
        let rem_size = self.rem_size(cx);
        window.with_rem_size(rem_size, |window| {
            self.editor.update(cx, |editor, cx| {
                editor.set_style(self.style.clone(), window, cx);

                let layout_id = match editor.mode {
                    EditorMode::SingleLine => {
                        let rem_size = window.rem_size();
                        let height = self.style.text.line_height_in_pixels(rem_size);
                        let mut style = Style::default();
                        style.size.height = height.into();
                        style.size.width = relative(1.).into();
                        window.request_layout(style, None, cx)
                    }
                    EditorMode::AutoHeight {
                        min_lines,
                        max_lines,
                    } => {
                        let editor_handle = cx.entity();
                        window.request_measured_layout(
                            Style::default(),
                            move |known_dimensions, available_space, window, cx| {
                                editor_handle
                                    .update(cx, |editor, cx| {
                                        compute_auto_height_layout(
                                            editor,
                                            min_lines,
                                            max_lines,
                                            known_dimensions,
                                            available_space.width,
                                            window,
                                            cx,
                                        )
                                    })
                                    .unwrap_or_default()
                            },
                        )
                    }
                    EditorMode::Minimap { .. } => {
                        let mut style = Style::default();
                        style.size.width = relative(1.).into();
                        style.size.height = relative(1.).into();
                        window.request_layout(style, None, cx)
                    }
                    EditorMode::Full {
                        sizing_behavior, ..
                    } => {
                        let mut style = Style::default();
                        style.size.width = relative(1.).into();
                        if sizing_behavior == SizingBehavior::SizeByContent {
                            let snapshot = editor.snapshot(window, cx);
                            let line_height =
                                self.style.text.line_height_in_pixels(window.rem_size());
                            let scroll_height =
                                (snapshot.max_point().row().next_row().0 as f32) * line_height;
                            style.size.height = scroll_height.into();
                        } else {
                            style.size.height = relative(1.).into();
                        }
                        window.request_layout(style, None, cx)
                    }
                };

                (layout_id, EditorRequestLayoutState::default())
            })
        })
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let _prepaint_depth_guard = request_layout.increment_prepaint_depth();
        let text_style = TextStyleRefinement {
            font_size: Some(self.style.text.font_size),
            line_height: Some(self.style.text.line_height),
            ..Default::default()
        };

        let is_minimap = self.editor.read(cx).mode.is_minimap();
        let is_singleton = self.editor.read(cx).buffer_kind(cx) == ItemBufferKind::Singleton;

        if !is_minimap {
            let focus_handle = self.editor.focus_handle(cx);
            window.set_view_id(self.editor.entity_id());
            window.set_focus_handle(&focus_handle, cx);
        }

        let rem_size = self.rem_size(cx);
        window.with_rem_size(rem_size, |window| {
            window.with_text_style(Some(text_style), |window| {
                window.with_content_mask(Some(ContentMask::new(bounds)), |window| {
                    let (mut snapshot, is_read_only) = self.editor.update(cx, |editor, cx| {
                        (editor.snapshot(window, cx), editor.read_only(cx))
                    });
                    let style = &self.style;

                    let rem_size = window.rem_size();
                    let font_id = window.text_system().resolve_font(&style.text.font());
                    let font_size = style.text.font_size.to_pixels(rem_size);
                    let line_height = style.text.line_height_in_pixels(rem_size);
                    let em_width = window.text_system().em_width(font_id, font_size).unwrap();
                    let em_advance = window.text_system().em_advance(font_id, font_size).unwrap();
                    let em_layout_width = window.text_system().em_layout_width(font_id, font_size);
                    let glyph_grid_cell = size(em_advance, line_height);

                    let gutter_dimensions =
                        snapshot.gutter_dimensions(font_id, font_size, style, window, cx);
                    let text_width = bounds.size.width - gutter_dimensions.width;

                    let settings = EditorSettings::get_global(cx);
                    let scrollbars_shown = settings.scrollbar.show != ShowScrollbar::Never;
                    let vertical_scrollbar_width = (scrollbars_shown
                        && settings.scrollbar.axes.vertical
                        && self.editor.read(cx).show_scrollbars.vertical)
                        .then_some(style.scrollbar_width)
                        .unwrap_or_default();
                    let minimap_width = self
                        .get_minimap_width(
                            &settings.minimap,
                            scrollbars_shown,
                            text_width,
                            em_width,
                            font_size,
                            rem_size,
                            cx,
                        )
                        .unwrap_or_default();

                    let right_margin = minimap_width + vertical_scrollbar_width;

                    let extended_right = 2 * em_width + right_margin;
                    let editor_width = text_width - gutter_dimensions.margin - extended_right;
                    let editor_margins = EditorMargins {
                        gutter: gutter_dimensions,
                        right: right_margin,
                        extended_right,
                    };

                    snapshot = self.editor.update(cx, |editor, cx| {
                        editor.last_bounds = Some(bounds);
                        editor.gutter_dimensions = gutter_dimensions;
                        editor.set_visible_line_count(
                            (bounds.size.height / line_height) as f64,
                            window,
                            cx,
                        );
                        editor.set_visible_column_count(f64::from(editor_width / em_advance));

                        if matches!(
                            editor.mode,
                            EditorMode::AutoHeight { .. } | EditorMode::Minimap { .. }
                        ) {
                            snapshot
                        } else {
                            let wrap_width = calculate_wrap_width(
                                editor.soft_wrap_mode(cx),
                                editor_width,
                                em_layout_width,
                            );

                            if editor.set_wrap_width(wrap_width, cx) {
                                editor.snapshot(window, cx)
                            } else {
                                snapshot
                            }
                        }
                    });

                    let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
                    let gutter_hitbox = window.insert_hitbox(
                        gutter_bounds(bounds, gutter_dimensions),
                        HitboxBehavior::Normal,
                    );
                    let text_hitbox = window.insert_hitbox(
                        Bounds {
                            origin: gutter_hitbox.top_right(),
                            size: size(text_width, bounds.size.height),
                        },
                        HitboxBehavior::Normal,
                    );

                    // Offset the content_bounds from the text_bounds by the gutter margin (which
                    // is roughly half a character wide) to make hit testing work more like how we want.
                    let content_offset = point(editor_margins.gutter.margin, Pixels::ZERO);
                    let content_origin = text_hitbox.origin + content_offset;

                    let height_in_lines = f64::from(bounds.size.height / line_height);
                    let max_row = snapshot.max_point().row().as_f64();

                    // Calculate how much of the editor is clipped by parent containers (e.g., List).
                    // This allows us to only render lines that are actually visible, which is
                    // critical for performance when large content-sized editors are inside Lists.
                    let visible_bounds = window.content_mask().bounds;
                    let visible_top = bounds.top().max(visible_bounds.top());
                    let visible_bottom = bounds.bottom().min(visible_bounds.bottom());
                    let clipped_top = (visible_top - bounds.top()).max(px(0.));
                    let visible_height = (visible_bottom - visible_top).max(px(0.));
                    let clipped_top_in_lines = f64::from(clipped_top / line_height);
                    let visible_height_in_lines = f64::from(visible_height / line_height);

                    // The max scroll position for the top of the window
                    let scroll_beyond_last_line = self.editor.read(cx).scroll_beyond_last_line(cx);
                    let max_scroll_top = match scroll_beyond_last_line {
                        ScrollBeyondLastLine::OnePage => max_row,
                        ScrollBeyondLastLine::Off => (max_row - height_in_lines + 1.).max(0.),
                        ScrollBeyondLastLine::VerticalScrollMargin => {
                            let settings = EditorSettings::get_global(cx);
                            (max_row - height_in_lines + 1. + settings.vertical_scroll_margin)
                                .max(0.)
                        }
                    };

                    let (
                        autoscroll_request,
                        autoscroll_containing_element,
                        needs_horizontal_autoscroll,
                    ) = self.editor.update(cx, |editor, cx| {
                        let autoscroll_request = editor.scroll_manager.take_autoscroll_request();

                        let autoscroll_containing_element =
                            autoscroll_request.is_some() || editor.has_pending_selection();

                        let (needs_horizontal_autoscroll, was_scrolled) = editor
                            .autoscroll_vertically(
                                bounds,
                                line_height,
                                max_scroll_top,
                                autoscroll_request,
                                window,
                                cx,
                            );
                        if was_scrolled.0 {
                            snapshot = editor.snapshot(window, cx);
                        }
                        (
                            autoscroll_request,
                            autoscroll_containing_element,
                            needs_horizontal_autoscroll,
                        )
                    });

                    let mut scroll_position = snapshot.scroll_position();
                    if !line_height.is_zero() {
                        scroll_position.y = window
                            .pixel_snap_f64(scroll_position.y * f64::from(line_height))
                            / f64::from(line_height);
                    }
                    // The scroll position is a fractional point, the whole number of which represents
                    // the top of the window in terms of display rows.
                    // We add clipped_top_in_lines to skip rows that are clipped by parent containers,
                    // but we don't modify scroll_position itself since the parent handles positioning.
                    let max_row = snapshot.max_point().row();
                    let start_row = cmp::min(
                        DisplayRow((scroll_position.y + clipped_top_in_lines).floor() as u32),
                        max_row,
                    );
                    let end_row = cmp::min(
                        (scroll_position.y + clipped_top_in_lines + visible_height_in_lines).ceil()
                            as u32,
                        max_row.next_row().0,
                    );
                    let end_row = DisplayRow(end_row);

                    let row_infos = snapshot // note we only get the visual range
                        .row_infos(start_row)
                        .take((start_row..end_row).len())
                        .collect::<Vec<RowInfo>>();
                    let is_row_soft_wrapped = |row: usize| {
                        row_infos
                            .get(row)
                            .is_none_or(|info| info.buffer_row.is_none())
                    };

                    let start_anchor = if start_row == Default::default() {
                        Anchor::Min
                    } else {
                        snapshot.buffer_snapshot().anchor_before(
                            DisplayPoint::new(start_row, 0).to_offset(&snapshot, Bias::Left),
                        )
                    };
                    let end_anchor = if end_row > max_row {
                        Anchor::Max
                    } else {
                        snapshot.buffer_snapshot().anchor_before(
                            DisplayPoint::new(end_row, 0).to_offset(&snapshot, Bias::Right),
                        )
                    };

                    let mut highlighted_rows = self
                        .editor
                        .update(cx, |editor, cx| editor.highlighted_display_rows(window, cx));

                    let mut highlighted_ranges = self
                        .editor_with_selections(cx)
                        .map(|editor| {
                            if editor == self.editor {
                                editor.read(cx).background_highlights_in_range(
                                    start_anchor..end_anchor,
                                    &snapshot.display_snapshot,
                                    cx.theme(),
                                )
                            } else {
                                editor.update(cx, |editor, cx| {
                                    let snapshot = editor.snapshot(window, cx);
                                    let start_anchor = if start_row == Default::default() {
                                        Anchor::Min
                                    } else {
                                        snapshot.buffer_snapshot().anchor_before(
                                            DisplayPoint::new(start_row, 0)
                                                .to_offset(&snapshot, Bias::Left),
                                        )
                                    };
                                    let end_anchor = if end_row > max_row {
                                        Anchor::Max
                                    } else {
                                        snapshot.buffer_snapshot().anchor_before(
                                            DisplayPoint::new(end_row, 0)
                                                .to_offset(&snapshot, Bias::Right),
                                        )
                                    };

                                    editor.background_highlights_in_range(
                                        start_anchor..end_anchor,
                                        &snapshot.display_snapshot,
                                        cx.theme(),
                                    )
                                })
                            }
                        })
                        .unwrap_or_default();

                    struct DiffHunkHighlightColors {
                        filled_background: Hsla,
                        hollow_background: Hsla,
                        hollow_border: Hsla,
                    }

                    let colors = cx.theme().colors();
                    let added_diff_hunk_colors = DiffHunkHighlightColors {
                        filled_background: colors.editor_diff_hunk_added_background,
                        hollow_background: colors.editor_diff_hunk_added_hollow_background,
                        hollow_border: colors.editor_diff_hunk_added_hollow_border,
                    };
                    let deleted_diff_hunk_colors = DiffHunkHighlightColors {
                        filled_background: colors.editor_diff_hunk_deleted_background,
                        hollow_background: colors.editor_diff_hunk_deleted_hollow_background,
                        hollow_border: colors.editor_diff_hunk_deleted_hollow_border,
                    };
                    let drag_highlight_color = colors.editor_active_line_background;
                    let drag_border_color = colors.border_focused;

                    for (ix, row_info) in row_infos.iter().enumerate() {
                        let Some(diff_status) = row_info.diff_status else {
                            continue;
                        };

                        let diff_hunk_colors = match diff_status.kind {
                            DiffHunkStatusKind::Added => &added_diff_hunk_colors,
                            DiffHunkStatusKind::Deleted => &deleted_diff_hunk_colors,
                            DiffHunkStatusKind::Modified => {
                                debug_panic!("modified diff status for row info");
                                continue;
                            }
                        };

                        let hollow_highlight = LineHighlight {
                            background: diff_hunk_colors.hollow_background.into(),
                            border: Some(diff_hunk_colors.hollow_border),
                            include_gutter: true,
                            type_id: None,
                        };

                        let filled_highlight = LineHighlight {
                            background: solid_background(diff_hunk_colors.filled_background),
                            border: None,
                            include_gutter: true,
                            type_id: None,
                        };

                        let background = if self.diff_hunk_hollow(diff_status, cx) {
                            hollow_highlight
                        } else {
                            filled_highlight
                        };

                        let base_display_point =
                            DisplayPoint::new(start_row + DisplayRow(ix as u32), 0);

                        highlighted_rows
                            .entry(base_display_point.row())
                            .or_insert(background);
                    }

                    // Add diff review drag selection highlight to text area
                    if let Some(drag_state) = &self.editor.read(cx).diff_review_drag_state {
                        let range = drag_state.row_range(&snapshot.display_snapshot);
                        let start_row = range.start().0;
                        let end_row = range.end().0;
                        let drag_highlight = LineHighlight {
                            background: solid_background(drag_highlight_color),
                            border: Some(drag_border_color),
                            include_gutter: true,
                            type_id: None,
                        };
                        for row_num in start_row..=end_row {
                            highlighted_rows
                                .entry(DisplayRow(row_num))
                                .or_insert(drag_highlight);
                        }
                    }

                    let highlighted_gutter_ranges =
                        self.editor.read(cx).gutter_highlights_in_range(
                            start_anchor..end_anchor,
                            &snapshot.display_snapshot,
                            cx,
                        );

                    let document_colors = self
                        .editor
                        .read(cx)
                        .colors
                        .as_ref()
                        .map(|colors| colors.editor_display_highlights(&snapshot));
                    let redacted_ranges = self.editor.read(cx).redacted_ranges(
                        start_anchor..end_anchor,
                        &snapshot.display_snapshot,
                        cx,
                    );

                    let (local_selections, selected_buffer_ids, latest_selection_anchors): (
                        Vec<Selection<Point>>,
                        Vec<BufferId>,
                        HashMap<BufferId, Anchor>,
                    ) = self
                        .editor_with_selections(cx)
                        .map(|editor| {
                            editor.update(cx, |editor, cx| {
                                let is_singleton =
                                    editor.buffer_kind(cx) == ItemBufferKind::Singleton;

                                // Singleton buffers only need the newest selection anchor here.
                                let selected_buffer_ids = if is_singleton {
                                    Vec::new()
                                } else {
                                    let all_selections =
                                        editor.selections.all::<Point>(&snapshot.display_snapshot);
                                    let mut selected_buffer_ids =
                                        Vec::with_capacity(all_selections.len());

                                    for selection in all_selections {
                                        for buffer_id in snapshot
                                            .buffer_snapshot()
                                            .buffer_ids_for_range(selection.range())
                                        {
                                            if selected_buffer_ids.last() != Some(&buffer_id) {
                                                selected_buffer_ids.push(buffer_id);
                                            }
                                        }
                                    }

                                    selected_buffer_ids
                                };

                                let mut selections = editor.selections.disjoint_in_range(
                                    start_anchor..end_anchor,
                                    &snapshot.display_snapshot,
                                );
                                selections
                                    .extend(editor.selections.pending(&snapshot.display_snapshot));

                                let latest_selection_anchors: HashMap<BufferId, Anchor> =
                                    if is_singleton {
                                        let head = editor.selections.newest_anchor().head();
                                        snapshot
                                            .buffer_snapshot()
                                            .anchor_to_buffer_anchor(head)
                                            .map(|(text_anchor, _)| (text_anchor.buffer_id, head))
                                            .into_iter()
                                            .collect()
                                    } else {
                                        let all_anchor_selections = editor
                                            .selections
                                            .all_anchors(&snapshot.display_snapshot);
                                        let mut anchors_by_buffer: HashMap<
                                            BufferId,
                                            (usize, Anchor),
                                        > = HashMap::default();
                                        for selection in all_anchor_selections.iter() {
                                            let head = selection.head();
                                            if let Some((text_anchor, _)) = snapshot
                                                .buffer_snapshot()
                                                .anchor_to_buffer_anchor(head)
                                            {
                                                anchors_by_buffer
                                                    .entry(text_anchor.buffer_id)
                                                    .and_modify(|(latest_id, latest_anchor)| {
                                                        if selection.id > *latest_id {
                                                            *latest_id = selection.id;
                                                            *latest_anchor = head;
                                                        }
                                                    })
                                                    .or_insert((selection.id, head));
                                            }
                                        }
                                        anchors_by_buffer
                                            .into_iter()
                                            .map(|(buffer_id, (_, anchor))| (buffer_id, anchor))
                                            .collect()
                                    };

                                (selections, selected_buffer_ids, latest_selection_anchors)
                            })
                        })
                        .unwrap_or_else(|| (Vec::new(), Vec::new(), HashMap::default()));

                    let (selections, mut active_rows, newest_selection_head) = self
                        .layout_selections(
                            start_anchor,
                            end_anchor,
                            &local_selections,
                            &snapshot,
                            start_row,
                            end_row,
                            window,
                            cx,
                        );

                    // relative rows are based on newest selection, even outside the visible area
                    let current_selection_head = self.editor.update(cx, |editor, cx| {
                        (editor.selections.count() != 0).then(|| {
                            let newest = editor
                                .selections
                                .newest::<Point>(&editor.display_snapshot(cx));

                            SelectionLayout::new(
                                newest,
                                editor.selections.line_mode(),
                                editor.cursor_offset_on_selection,
                                editor.cursor_shape,
                                &snapshot,
                                true,
                                true,
                                None,
                            )
                            .head
                            .row()
                        })
                    });

                    let run_indicator_rows = self.editor.update(cx, |editor, cx| {
                        editor.active_run_indicators(start_row..end_row, window, cx)
                    });

                    let mut breakpoint_rows = self.editor.update(cx, |editor, cx| {
                        editor.active_breakpoints(start_row..end_row, window, cx)
                    });

                    for (display_row, (_, bp, state)) in &breakpoint_rows {
                        if bp.is_enabled() && state.is_none_or(|s| s.verified) {
                            active_rows.entry(*display_row).or_default().breakpoint = true;
                        }
                    }

                    let gutter = Gutter {
                        line_height,
                        range: start_row..end_row,
                        scroll_position,
                        dimensions: &gutter_dimensions,
                        hitbox: &gutter_hitbox,
                        snapshot: &snapshot,
                        row_infos: &row_infos,
                    };

                    let line_numbers = self.layout_line_numbers(
                        &gutter,
                        &active_rows,
                        current_selection_head,
                        window,
                        cx,
                    );

                    let mut expand_toggles =
                        window.with_element_namespace("expand_toggles", |window| {
                            self.layout_expand_toggles(
                                &gutter_hitbox,
                                gutter_dimensions,
                                em_width,
                                line_height,
                                scroll_position,
                                start_row,
                                &row_infos,
                                window,
                                cx,
                            )
                        });

                    let mut crease_toggles =
                        window.with_element_namespace("crease_toggles", |window| {
                            self.layout_crease_toggles(
                                start_row..end_row,
                                &row_infos,
                                &active_rows,
                                &snapshot,
                                window,
                                cx,
                            )
                        });
                    let crease_trailers =
                        window.with_element_namespace("crease_trailers", |window| {
                            self.layout_crease_trailers(
                                row_infos.iter().cloned(),
                                &snapshot,
                                window,
                                cx,
                            )
                        });

                    let display_hunks = self.layout_gutter_diff_hunks(
                        line_height,
                        &gutter_hitbox,
                        start_row..end_row,
                        &snapshot,
                        scroll_position,
                        window,
                        cx,
                    );

                    Self::layout_word_diff_highlights(
                        &display_hunks,
                        &row_infos,
                        start_row,
                        &snapshot,
                        &mut highlighted_ranges,
                        cx,
                    );

                    let bg_segments_per_row = Self::bg_segments_per_row(
                        start_row..end_row,
                        &selections,
                        highlighted_ranges.iter().cloned().chain(
                            document_colors
                                .iter()
                                .flat_map(|(_, colors)| colors.iter().cloned()),
                        ),
                        self.style.background,
                    );

                    let mut line_layouts = Self::layout_lines(
                        start_row..end_row,
                        &snapshot,
                        &self.style,
                        editor_width,
                        is_row_soft_wrapped,
                        &bg_segments_per_row,
                        window,
                        cx,
                    );
                    let new_renderer_widths = (!is_minimap).then(|| {
                        line_layouts
                            .iter()
                            .flat_map(|layout| &layout.fragments)
                            .filter_map(|fragment| {
                                if let LineFragment::Element { id, size, .. } = fragment {
                                    Some((*id, size.width))
                                } else {
                                    None
                                }
                            })
                    });
                    let renderer_widths_changed = request_layout.has_remaining_prepaint_depth()
                        && new_renderer_widths.is_some_and(|new_renderer_widths| {
                            self.editor.update(cx, |editor, cx| {
                                editor.update_renderer_widths(new_renderer_widths, cx)
                            })
                        });
                    if renderer_widths_changed {
                        return self.prepaint(
                            None,
                            _inspector_id,
                            bounds,
                            request_layout,
                            window,
                            cx,
                        );
                    }

                    let longest_line_blame_width = self
                        .editor
                        .update(cx, |editor, cx| {
                            if !editor.show_git_blame_inline {
                                return None;
                            }
                            let blame = editor.blame.as_ref()?;
                            let (_, blame_entry) = blame
                                .update(cx, |blame, cx| {
                                    let row_infos =
                                        snapshot.row_infos(snapshot.longest_row()).next()?;
                                    blame.blame_for_rows(&[row_infos], cx).next()
                                })
                                .flatten()?;
                            let mut element = render_inline_blame_entry(blame_entry, style, cx)?;
                            let inline_blame_padding =
                                ProjectSettings::get_global(cx).git.inline_blame.padding as f32
                                    * em_advance;
                            Some(
                                element
                                    .layout_as_root(AvailableSpace::min_size(), window, cx)
                                    .width
                                    + inline_blame_padding,
                            )
                        })
                        .unwrap_or(Pixels::ZERO);

                    let longest_line_width = layout_line(
                        snapshot.longest_row(),
                        &snapshot,
                        style,
                        editor_width,
                        is_row_soft_wrapped,
                        window,
                        cx,
                    )
                    .width;

                    let scrollbar_layout_information = ScrollbarLayoutInformation::new(
                        text_hitbox.bounds,
                        glyph_grid_cell,
                        size(
                            longest_line_width,
                            Pixels::from(max_row.as_f64() * f64::from(line_height)),
                        ),
                        longest_line_blame_width,
                        EditorSettings::get_global(cx),
                        scroll_beyond_last_line,
                    );

                    let mut scroll_width = scrollbar_layout_information.scroll_range.width;

                    let sticky_header_excerpt = if snapshot.buffer_snapshot().show_headers() {
                        snapshot.sticky_header_excerpt(scroll_position.y)
                    } else {
                        None
                    };
                    let sticky_header_excerpt_id = sticky_header_excerpt
                        .as_ref()
                        .map(|top| top.excerpt.buffer_id());

                    let buffer = snapshot.buffer_snapshot();
                    let start_buffer_row = MultiBufferRow(start_anchor.to_point(&buffer).row);
                    let end_buffer_row = MultiBufferRow(end_anchor.to_point(&buffer).row);

                    let preliminary_scroll_pixel_position = point(
                        scroll_position.x * f64::from(em_layout_width),
                        scroll_position.y * f64::from(line_height),
                    );
                    let indent_guides = self.layout_indent_guides(
                        content_origin,
                        text_hitbox.origin,
                        start_buffer_row..end_buffer_row,
                        preliminary_scroll_pixel_position,
                        line_height,
                        &snapshot,
                        window,
                        cx,
                    );
                    let indent_guides_for_spacers = indent_guides.clone();

                    let blocks = (!is_minimap)
                        .then(|| {
                            window.with_element_namespace("blocks", |window| {
                                self.render_blocks(
                                    start_row..end_row,
                                    &snapshot,
                                    &hitbox,
                                    &text_hitbox,
                                    editor_width,
                                    &mut scroll_width,
                                    &editor_margins,
                                    em_width,
                                    gutter_dimensions.full_width(),
                                    line_height,
                                    &mut line_layouts,
                                    &local_selections,
                                    &selected_buffer_ids,
                                    &latest_selection_anchors,
                                    is_row_soft_wrapped,
                                    sticky_header_excerpt_id,
                                    &indent_guides_for_spacers,
                                    window,
                                    cx,
                                )
                            })
                        })
                        .unwrap_or_default();
                    let RenderBlocksOutput {
                        non_spacer_blocks: mut blocks,
                        mut spacer_blocks,
                        row_block_types,
                        resized_blocks,
                    } = blocks;
                    if let Some(resized_blocks) = resized_blocks {
                        if request_layout.has_remaining_prepaint_depth() {
                            self.editor.update(cx, |editor, cx| {
                                editor.resize_blocks(
                                    resized_blocks,
                                    autoscroll_request.map(|(autoscroll, _)| autoscroll),
                                    cx,
                                )
                            });
                            return self.prepaint(
                                None,
                                _inspector_id,
                                bounds,
                                request_layout,
                                window,
                                cx,
                            );
                        } else {
                            debug_panic!(
                                "dropping block resize because prepaint depth \
                                 limit was reached"
                            );
                        }
                    }

                    let sticky_buffer_header = if self.should_show_buffer_headers() {
                        sticky_header_excerpt.map(|sticky_header_excerpt| {
                            window.with_element_namespace("blocks", |window| {
                                self.layout_sticky_buffer_header(
                                    sticky_header_excerpt,
                                    scroll_position,
                                    line_height,
                                    right_margin,
                                    &snapshot,
                                    &hitbox,
                                    &selected_buffer_ids,
                                    &blocks,
                                    &latest_selection_anchors,
                                    window,
                                    cx,
                                )
                            })
                        })
                    } else {
                        None
                    };

                    let scroll_max: gpui::Point<ScrollPixelOffset> = point(
                        ScrollPixelOffset::from(
                            ((scroll_width - editor_width) / em_layout_width).max(0.0),
                        ),
                        max_scroll_top,
                    );

                    self.editor.update(cx, |editor, cx| {
                        if editor.scroll_manager.clamp_scroll_left(scroll_max.x, cx) {
                            scroll_position.x = scroll_max.x.min(scroll_position.x);
                        }

                        if needs_horizontal_autoscroll.0
                            && let Some(new_scroll_position) = editor.autoscroll_horizontally(
                                start_row,
                                editor_width,
                                scroll_width,
                                em_advance,
                                &line_layouts,
                                autoscroll_request,
                                window,
                                cx,
                            )
                        {
                            scroll_position.x = new_scroll_position.x;
                        }
                    });

                    if !em_layout_width.is_zero() {
                        scroll_position.x = window
                            .pixel_snap_f64(scroll_position.x * f64::from(em_layout_width))
                            / f64::from(em_layout_width);
                    }

                    let scroll_pixel_position = point(
                        scroll_position.x * f64::from(em_layout_width),
                        scroll_position.y * f64::from(line_height),
                    );
                    let sticky_headers = if !is_minimap
                        && is_singleton
                        && EditorSettings::get_global(cx).sticky_scroll.enabled
                    {
                        let relative = self.editor.read(cx).relative_line_numbers(cx);
                        self.layout_sticky_headers(
                            &snapshot,
                            editor_width,
                            is_row_soft_wrapped,
                            line_height,
                            scroll_pixel_position,
                            content_origin,
                            &gutter_dimensions,
                            &gutter_hitbox,
                            &text_hitbox,
                            relative,
                            current_selection_head,
                            window,
                            cx,
                        )
                    } else {
                        None
                    };
                    let indent_guides =
                        if scroll_pixel_position != preliminary_scroll_pixel_position {
                            self.layout_indent_guides(
                                content_origin,
                                text_hitbox.origin,
                                start_buffer_row..end_buffer_row,
                                scroll_pixel_position,
                                line_height,
                                &snapshot,
                                window,
                                cx,
                            )
                        } else {
                            indent_guides
                        };

                    let crease_trailers =
                        window.with_element_namespace("crease_trailers", |window| {
                            self.prepaint_crease_trailers(
                                crease_trailers,
                                &line_layouts,
                                line_height,
                                content_origin,
                                scroll_pixel_position,
                                scroll_position,
                                start_row,
                                em_width,
                                window,
                                cx,
                            )
                        });

                    let (edit_prediction_popover, edit_prediction_popover_origin) = self
                        .editor
                        .update(cx, |editor, cx| {
                            editor.render_edit_prediction_popover(
                                &text_hitbox.bounds,
                                content_origin,
                                right_margin,
                                &snapshot,
                                start_row..end_row,
                                scroll_position.y,
                                scroll_position.y + height_in_lines,
                                &line_layouts,
                                line_height,
                                scroll_position,
                                scroll_pixel_position,
                                newest_selection_head,
                                editor_width,
                                style,
                                window,
                                cx,
                            )
                        })
                        .unzip();

                    let mut inline_diagnostics = self.layout_inline_diagnostics(
                        &line_layouts,
                        &crease_trailers,
                        &row_block_types,
                        content_origin,
                        scroll_position,
                        scroll_pixel_position,
                        edit_prediction_popover_origin,
                        start_row,
                        end_row,
                        line_height,
                        em_width,
                        style,
                        window,
                        cx,
                    );

                    let mut inline_blame_layout = None;
                    let mut inline_code_actions = None;
                    if let Some(newest_selection_head) = newest_selection_head {
                        let display_row = newest_selection_head.row();
                        if (start_row..end_row).contains(&display_row)
                            && !row_block_types.contains_key(&display_row)
                        {
                            inline_code_actions = self.layout_inline_code_actions(
                                newest_selection_head,
                                content_origin,
                                scroll_position,
                                scroll_pixel_position,
                                line_height,
                                &snapshot,
                                window,
                                cx,
                            );

                            let line_ix = display_row.minus(start_row) as usize;
                            if let (Some(row_info), Some(line_layout), Some(crease_trailer)) = (
                                row_infos.get(line_ix),
                                line_layouts.get(line_ix),
                                crease_trailers.get(line_ix),
                            ) {
                                let crease_trailer_layout = crease_trailer.as_ref();
                                if let Some(layout) = self.layout_inline_blame(
                                    display_row,
                                    row_info,
                                    line_layout,
                                    crease_trailer_layout,
                                    em_width,
                                    content_origin,
                                    scroll_position,
                                    scroll_pixel_position,
                                    line_height,
                                    window,
                                    cx,
                                ) {
                                    inline_blame_layout = Some(layout);
                                    // Blame overrides inline diagnostics
                                    inline_diagnostics.remove(&display_row);
                                }
                            } else {
                                log::error!(
                                    "bug: line_ix {} is out of bounds - row_infos.len(): {}, \
                                    line_layouts.len(): {}, \
                                    crease_trailers.len(): {}",
                                    line_ix,
                                    row_infos.len(),
                                    line_layouts.len(),
                                    crease_trailers.len(),
                                );
                            }
                        }
                    }

                    let blamed_display_rows = self.layout_blame_entries(
                        &row_infos,
                        em_width,
                        scroll_position,
                        start_row,
                        line_height,
                        &gutter_hitbox,
                        gutter_dimensions.git_blame_entries_width,
                        window,
                        cx,
                    );

                    let line_elements = self.prepaint_lines(
                        start_row,
                        &mut line_layouts,
                        line_height,
                        scroll_position,
                        scroll_pixel_position,
                        content_origin,
                        window,
                        cx,
                    );

                    window.with_element_namespace("blocks", |window| {
                        self.layout_blocks(
                            &mut blocks,
                            &hitbox,
                            &gutter_hitbox,
                            line_height,
                            scroll_position,
                            scroll_pixel_position,
                            &editor_margins,
                            window,
                            cx,
                        );
                        self.layout_blocks(
                            &mut spacer_blocks,
                            &hitbox,
                            &gutter_hitbox,
                            line_height,
                            scroll_position,
                            scroll_pixel_position,
                            &editor_margins,
                            window,
                            cx,
                        );
                    });

                    let cursors = self.collect_cursors(&snapshot, cx);
                    let visible_row_range = start_row..end_row;
                    let non_visible_cursors = cursors
                        .iter()
                        .any(|c| !visible_row_range.contains(&c.0.row()));

                    let visible_cursors = self.layout_visible_cursors(
                        &snapshot,
                        &selections,
                        &row_block_types,
                        start_row..end_row,
                        &line_layouts,
                        &text_hitbox,
                        content_origin,
                        scroll_position,
                        scroll_pixel_position,
                        line_height,
                        em_width,
                        em_advance,
                        autoscroll_containing_element,
                        &redacted_ranges,
                        window,
                        cx,
                    );
                    let navigation_overlay_paint_commands = self.layout_navigation_overlays(
                        &snapshot,
                        start_row..end_row,
                        &line_layouts,
                        &text_hitbox,
                        content_origin,
                        scroll_position,
                        scroll_pixel_position,
                        line_height,
                        window,
                        cx,
                    );

                    let scrollbars_layout = self.layout_scrollbars(
                        &snapshot,
                        &scrollbar_layout_information,
                        content_offset,
                        scroll_position,
                        non_visible_cursors,
                        right_margin,
                        editor_width,
                        window,
                        cx,
                    );

                    let gutter_settings = EditorSettings::get_global(cx).gutter;

                    let context_menu_layout =
                        if let Some(newest_selection_head) = newest_selection_head {
                            let newest_selection_point =
                                newest_selection_head.to_point(&snapshot.display_snapshot);
                            if (start_row..end_row).contains(&newest_selection_head.row()) {
                                self.layout_cursor_popovers(
                                    line_height,
                                    &text_hitbox,
                                    content_origin,
                                    right_margin,
                                    start_row,
                                    scroll_pixel_position,
                                    &line_layouts,
                                    newest_selection_head,
                                    newest_selection_point,
                                    style,
                                    window,
                                    cx,
                                )
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                    self.layout_gutter_menu(
                        line_height,
                        &text_hitbox,
                        content_origin,
                        right_margin,
                        scroll_pixel_position,
                        gutter_dimensions.width - gutter_dimensions.left_padding,
                        window,
                        cx,
                    );

                    let test_indicators = if gutter_settings.runnables {
                        self.layout_run_indicators(
                            &gutter,
                            &run_indicator_rows,
                            &breakpoint_rows,
                            window,
                            cx,
                        )
                    } else {
                        Vec::new()
                    };

                    let show_bookmarks =
                        snapshot.show_bookmarks.unwrap_or(gutter_settings.bookmarks);

                    let bookmark_rows = self.editor.update(cx, |editor, cx| {
                        let mut rows = editor.active_bookmarks(start_row..end_row, window, cx);
                        rows.retain(|k| !run_indicator_rows.contains(k));
                        rows.retain(|k| !breakpoint_rows.contains_key(k));
                        rows
                    });

                    let bookmarks = if show_bookmarks {
                        self.layout_bookmarks(&gutter, &bookmark_rows, window, cx)
                    } else {
                        Vec::new()
                    };

                    let show_breakpoints = snapshot
                        .show_breakpoints
                        .unwrap_or(gutter_settings.breakpoints);

                    breakpoint_rows.retain(|k, _| !run_indicator_rows.contains(k));
                    let mut breakpoints = if show_breakpoints {
                        self.layout_breakpoints(&gutter, &breakpoint_rows, window, cx)
                    } else {
                        Vec::new()
                    };

                    let gutter_hover_button = self
                        .editor
                        .read(cx)
                        .gutter_hover_button
                        .0
                        .filter(|phantom| phantom.is_active)
                        .map(|phantom| phantom.display_row);

                    if let Some(row) = gutter_hover_button
                        && !breakpoint_rows.contains_key(&row)
                        && !run_indicator_rows.contains(&row)
                        && !bookmark_rows.contains(&row)
                        && (show_bookmarks || show_breakpoints)
                    {
                        let position = snapshot
                            .display_point_to_anchor(DisplayPoint::new(row, 0), Bias::Right);
                        breakpoints.extend(
                            self.layout_gutter_hover_button(&gutter, position, row, window, cx),
                        );
                    }

                    let git_gutter_width = Self::gutter_strip_width(line_height)
                        + gutter_dimensions
                            .git_blame_entries_width
                            .unwrap_or_default();
                    let available_width = gutter_dimensions.left_padding - git_gutter_width;

                    let max_line_number_length = self
                        .editor
                        .read(cx)
                        .buffer()
                        .read(cx)
                        .snapshot(cx)
                        .widest_line_number()
                        .ilog10()
                        + 1;

                    let diff_review_button = self
                        .should_render_diff_review_button(
                            start_row..end_row,
                            &row_infos,
                            &snapshot,
                            cx,
                        )
                        .map(|(display_row, buffer_row)| {
                            let is_wide = max_line_number_length
                                >= EditorSettings::get_global(cx).gutter.min_line_number_digits
                                    as u32
                                && buffer_row.is_some_and(|row| {
                                    (row + 1).ilog10() + 1 == max_line_number_length
                                })
                                || gutter_dimensions.right_padding == px(0.);

                            let button_width = if is_wide {
                                available_width - px(6.)
                            } else {
                                available_width + em_width - px(6.)
                            };

                            let button = self.editor.update(cx, |editor, cx| {
                                editor
                                    .render_diff_review_button(display_row, button_width, cx)
                                    .into_any_element()
                            });
                            gutter.prepaint_button(button, display_row, window, cx)
                        });

                    self.layout_signature_help(
                        &hitbox,
                        content_origin,
                        scroll_pixel_position,
                        newest_selection_head,
                        start_row,
                        &line_layouts,
                        line_height,
                        em_width,
                        context_menu_layout,
                        window,
                        cx,
                    );

                    if !cx.has_active_drag() {
                        self.layout_hover_popovers(
                            &snapshot,
                            &hitbox,
                            start_row..end_row,
                            content_origin,
                            scroll_pixel_position,
                            &line_layouts,
                            line_height,
                            em_width,
                            context_menu_layout,
                            window,
                            cx,
                        );

                        self.layout_blame_popover(&snapshot, &hitbox, line_height, window, cx);
                    }

                    let mouse_context_menu = self.layout_mouse_context_menu(
                        &snapshot,
                        start_row..end_row,
                        content_origin,
                        window,
                        cx,
                    );

                    window.with_element_namespace("crease_toggles", |window| {
                        self.prepaint_crease_toggles(
                            &mut crease_toggles,
                            line_height,
                            &gutter_dimensions,
                            gutter_settings,
                            scroll_position,
                            start_row,
                            &gutter_hitbox,
                            window,
                            cx,
                        )
                    });

                    window.with_element_namespace("expand_toggles", |window| {
                        self.prepaint_expand_toggles(&mut expand_toggles, window, cx)
                    });

                    let wrap_guides = self.layout_wrap_guides(
                        em_advance,
                        scroll_position,
                        content_origin,
                        scrollbars_layout.as_ref(),
                        vertical_scrollbar_width,
                        &hitbox,
                        window,
                        cx,
                    );

                    let minimap = window.with_element_namespace("minimap", |window| {
                        self.layout_minimap(
                            &snapshot,
                            minimap_width,
                            scroll_position,
                            &scrollbar_layout_information,
                            scrollbars_layout.as_ref(),
                            window,
                            cx,
                        )
                    });

                    let invisible_symbol_font_size = font_size / 2.;
                    let whitespace_map = &self
                        .editor
                        .read(cx)
                        .buffer
                        .read(cx)
                        .language_settings(cx)
                        .whitespace_map;

                    let tab_char = whitespace_map.tab.clone();
                    let tab_len = tab_char.len();
                    let tab_invisible = window.text_system().shape_line(
                        tab_char,
                        invisible_symbol_font_size,
                        &[TextRun {
                            len: tab_len,
                            font: self.style.text.font(),
                            color: cx.theme().colors().editor_invisible,
                            ..Default::default()
                        }],
                        None,
                    );

                    let space_char = whitespace_map.space.clone();
                    let space_len = space_char.len();
                    let space_invisible = window.text_system().shape_line(
                        space_char,
                        invisible_symbol_font_size,
                        &[TextRun {
                            len: space_len,
                            font: self.style.text.font(),
                            color: cx.theme().colors().editor_invisible,
                            ..Default::default()
                        }],
                        None,
                    );

                    let mode = snapshot.mode.clone();

                    let sticky_scroll_header_height = sticky_headers
                        .as_ref()
                        .and_then(|headers| headers.lines.last())
                        .map_or(Pixels::ZERO, |last| last.offset + line_height);

                    let has_sticky_buffer_header =
                        sticky_buffer_header.is_some() || sticky_header_excerpt_id.is_some();
                    let sticky_header_height = if has_sticky_buffer_header {
                        let full_height = FILE_HEADER_HEIGHT as f32 * line_height;
                        let display_row = blocks
                            .iter()
                            .filter(|block| block.is_buffer_header)
                            .find_map(|block| {
                                block.row.filter(|row| row.0 > scroll_position.y as u32)
                            });
                        let offset = match display_row {
                            Some(display_row) => {
                                let max_row = display_row.0.saturating_sub(FILE_HEADER_HEIGHT);
                                let offset = (scroll_position.y - max_row as f64).max(0.0);
                                let slide_up =
                                    Pixels::from(offset * ScrollPixelOffset::from(line_height));

                                (full_height - slide_up).max(Pixels::ZERO)
                            }
                            None => full_height,
                        };
                        let header_bottom_padding =
                            BUFFER_HEADER_PADDING.to_pixels(window.rem_size());
                        sticky_scroll_header_height + offset - header_bottom_padding
                    } else {
                        sticky_scroll_header_height
                    };

                    let (diff_hunk_controls, diff_hunk_control_bounds) =
                        if is_read_only && !self.editor.read(cx).delegate_stage_and_restore {
                            (vec![], vec![])
                        } else {
                            self.layout_diff_hunk_controls(
                                start_row..end_row,
                                &row_infos,
                                &text_hitbox,
                                current_selection_head,
                                line_height,
                                right_margin,
                                scroll_pixel_position,
                                sticky_header_height,
                                &display_hunks,
                                &highlighted_rows,
                                self.editor.clone(),
                                window,
                                cx,
                            )
                        };

                    let position_map = Rc::new(PositionMap {
                        size: bounds.size,
                        visible_row_range,
                        scroll_position,
                        scroll_pixel_position,
                        scroll_max,
                        line_layouts,
                        line_height,
                        em_advance,
                        em_layout_width,
                        snapshot,
                        text_align: self.style.text.text_align,
                        content_width: text_hitbox.size.width,
                        gutter_hitbox: gutter_hitbox.clone(),
                        text_hitbox: text_hitbox.clone(),
                        inline_blame_bounds: inline_blame_layout
                            .as_ref()
                            .map(|layout| (layout.bounds, layout.buffer_id, layout.entry.clone())),
                        display_hunks: display_hunks.clone(),
                        diff_hunk_control_bounds,
                    });

                    let visible_horizontal_scrollbar =
                        scrollbars_layout.as_ref().is_some_and(|scrollbars_layout| {
                            scrollbars_layout.visible && scrollbars_layout.horizontal.is_some()
                        });

                    self.editor.update(cx, |editor, _| {
                        editor.last_position_map = Some(position_map.clone());
                        editor.last_right_margin = right_margin;
                        editor.last_horizontal_scrollbar_visible = visible_horizontal_scrollbar;
                    });

                    EditorLayout {
                        mode,
                        position_map,
                        visible_display_row_range: start_row..end_row,
                        wrap_guides,
                        indent_guides,
                        hitbox,
                        gutter_hitbox,
                        display_hunks,
                        content_origin,
                        scrollbars_layout,
                        minimap,
                        active_rows,
                        highlighted_rows,
                        highlighted_ranges,
                        highlighted_gutter_ranges,
                        redacted_ranges,
                        document_colors,
                        line_elements,
                        line_numbers,
                        blamed_display_rows,
                        inline_diagnostics,
                        inline_blame_layout,
                        inline_code_actions,
                        blocks,
                        spacer_blocks,
                        cursors,
                        visible_cursors,
                        navigation_overlay_paint_commands,
                        selections,
                        edit_prediction_popover,
                        diff_hunk_controls,
                        mouse_context_menu,
                        test_indicators,
                        bookmarks,
                        breakpoints,
                        diff_review_button,
                        crease_toggles,
                        crease_trailers,
                        tab_invisible,
                        space_invisible,
                        sticky_buffer_header,
                        sticky_headers,
                        expand_toggles,
                        text_align: self.style.text.text_align,
                        content_width: text_hitbox.size.width,
                    }
                })
            })
        })
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<gpui::Pixels>,
        _: &mut Self::RequestLayoutState,
        layout: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if !layout.mode.is_minimap() {
            let focus_handle = self.editor.focus_handle(cx);
            let key_context = self
                .editor
                .update(cx, |editor, cx| editor.key_context(window, cx));

            window.set_key_context(key_context);
            window.handle_input(
                &focus_handle,
                ElementInputHandler::new(bounds, self.editor.clone()),
                cx,
            );
            self.register_actions(window, cx);
            self.register_key_listeners(window, cx, layout);
        }

        let text_style = TextStyleRefinement {
            font_size: Some(self.style.text.font_size),
            line_height: Some(self.style.text.line_height),
            ..Default::default()
        };
        let rem_size = self.rem_size(cx);
        window.with_rem_size(rem_size, |window| {
            window.with_text_style(Some(text_style), |window| {
                window.with_content_mask(Some(ContentMask::new(bounds)), |window| {
                    self.paint_mouse_listeners(layout, window, cx);

                    // Mask the editor behind sticky scroll headers. Important
                    // for transparent backgrounds.
                    let below_sticky_headers_mask = layout
                        .sticky_headers
                        .as_ref()
                        .and_then(|h| h.lines.last())
                        .map(|last| {
                            ContentMask::new(Bounds {
                                origin: point(
                                    bounds.origin.x,
                                    bounds.origin.y + last.offset + layout.position_map.line_height,
                                ),
                                size: size(
                                    bounds.size.width,
                                    (bounds.size.height
                                        - last.offset
                                        - layout.position_map.line_height)
                                        .max(Pixels::ZERO),
                                ),
                            })
                        });

                    window.with_content_mask(below_sticky_headers_mask, |window| {
                        self.paint_background(layout, window, cx);

                        self.paint_indent_guides(layout, window, cx);

                        if layout.gutter_hitbox.size.width > Pixels::ZERO {
                            self.paint_blamed_display_rows(layout, window, cx);
                            self.paint_line_numbers(layout, window, cx);
                        }

                        self.paint_text(layout, window, cx);

                        if !layout.spacer_blocks.is_empty() {
                            window.with_element_namespace("blocks", |window| {
                                self.paint_spacer_blocks(layout, window, cx);
                            });
                        }

                        if layout.gutter_hitbox.size.width > Pixels::ZERO {
                            self.paint_gutter_highlights(layout, window, cx);
                            self.paint_gutter_indicators(layout, window, cx);
                        }

                        if !layout.blocks.is_empty() {
                            window.with_element_namespace("blocks", |window| {
                                self.paint_non_spacer_blocks(layout, window, cx);
                            });
                        }
                    });

                    window.with_element_namespace("blocks", |window| {
                        if let Some(mut sticky_header) = layout.sticky_buffer_header.take() {
                            sticky_header.paint(window, cx)
                        }
                    });

                    self.paint_sticky_headers(layout, window, cx);
                    self.paint_minimap(layout, window, cx);
                    self.paint_scrollbars(layout, window, cx);
                    self.paint_edit_prediction_popover(layout, window, cx);
                    self.paint_mouse_context_menu(layout, window, cx);
                });
            })
        })
    }
}

pub(super) fn gutter_bounds(
    editor_bounds: Bounds<Pixels>,
    gutter_dimensions: GutterDimensions,
) -> Bounds<Pixels> {
    Bounds {
        origin: editor_bounds.origin,
        size: size(gutter_dimensions.width, editor_bounds.size.height),
    }
}

impl IntoElement for EditorElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Editor, HighlightKey, MultiBuffer, NavigationOverlayKey, NavigationOverlayLabel,
        NavigationTargetOverlay, SelectionEffects,
        display_map::{BlockPlacement, BlockProperties},
        editor_tests::{init_test, update_test_language_settings},
    };
    use gpui::{TestAppContext, VisualTestContext};
    use language::{Buffer, language_settings, tree_sitter_python};
    use log::info;
    use rand::{RngCore, rngs::StdRng};
    use std::num::NonZeroU32;
    use util::test::sample_text;

    enum PrimaryNavigationOverlay {}

    const PRIMARY_NAVIGATION_OVERLAY_KEY: NavigationOverlayKey =
        NavigationOverlayKey::unique::<PrimaryNavigationOverlay>();

    fn navigation_overlay(
        label_text: &'static str,
        target_range: Range<Anchor>,
        covered_text_range: Option<Range<Anchor>>,
    ) -> NavigationTargetOverlay {
        NavigationTargetOverlay {
            target_range,
            label: NavigationOverlayLabel {
                text: SharedString::from(label_text),
                text_color: Hsla::black(),
                x_offset: Pixels::ZERO,
                scale_factor: 1.0,
            },
            covered_text_range,
        }
    }

    fn navigation_label_layouts(state: &EditorLayout) -> Vec<&NavigationLabelLayout> {
        state
            .navigation_overlay_paint_commands
            .iter()
            .map(|command| match command {
                NavigationOverlayPaintCommand::Label(label) => label,
            })
            .collect()
    }

    const fn placeholder_hitbox() -> Hitbox {
        use gpui::HitboxId;
        let zero_bounds = Bounds {
            origin: point(Pixels::ZERO, Pixels::ZERO),
            size: Size {
                width: Pixels::ZERO,
                height: Pixels::ZERO,
            },
        };

        Hitbox {
            id: HitboxId::placeholder(),
            bounds: zero_bounds,
            content_mask: ContentMask::new(zero_bounds),
            behavior: HitboxBehavior::Normal,
        }
    }

    fn test_gutter(line_height: Pixels, snapshot: &EditorSnapshot) -> Gutter<'_> {
        const DIMENSIONS: GutterDimensions = GutterDimensions {
            left_padding: Pixels::ZERO,
            right_padding: Pixels::ZERO,
            width: px(30.0),
            margin: Pixels::ZERO,
            git_blame_entries_width: None,
        };
        const EMPTY_ROW_INFO: RowInfo = RowInfo {
            buffer_id: None,
            buffer_row: None,
            multibuffer_row: None,
            diff_status: None,
            expand_info: None,
            wrapped_buffer_row: None,
        };

        const fn row_info(row: u32) -> RowInfo {
            RowInfo {
                buffer_row: Some(row),
                ..EMPTY_ROW_INFO
            }
        }

        const ROW_INFOS: [RowInfo; 6] = [
            row_info(0),
            row_info(1),
            row_info(2),
            row_info(3),
            row_info(4),
            row_info(5),
        ];

        const HITBOX: Hitbox = placeholder_hitbox();
        Gutter {
            line_height,
            range: DisplayRow(0)..DisplayRow(6),
            scroll_position: gpui::Point::default(),
            dimensions: &DIMENSIONS,
            hitbox: &HITBOX,
            snapshot: snapshot,
            row_infos: &ROW_INFOS,
        }
    }

    #[gpui::test]
    async fn test_soft_wrap_editor_width_auto_height_editor(cx: &mut TestAppContext) {
        init_test(cx, |_| {});
        let window = cx.add_window(|window, cx| {
            let buffer = MultiBuffer::build_simple(&"a ".to_string().repeat(100), cx);
            let mut editor = Editor::new(
                EditorMode::AutoHeight {
                    min_lines: 1,
                    max_lines: None,
                },
                buffer,
                None,
                window,
                cx,
            );
            editor.set_soft_wrap_mode(language_settings::SoftWrap::EditorWidth, cx);
            editor
        });
        let cx = &mut VisualTestContext::from_window(*window, cx);
        let editor = window.root(cx).unwrap();
        let style = cx.update(|_, cx| editor.update(cx, |editor, cx| editor.style(cx).clone()));

        for x in 1..=100 {
            let (_, state) = cx.draw(
                Default::default(),
                size(px(200. + 0.13 * x as f32), px(500.)),
                |_, _| EditorElement::new(&editor, style.clone()),
            );

            assert!(
                state.position_map.scroll_max.x == 0.,
                "Soft wrapped editor should have no horizontal scrolling!"
            );
        }
    }

    #[gpui::test]
    async fn test_soft_wrap_editor_width_full_editor(cx: &mut TestAppContext) {
        init_test(cx, |_| {});
        let window = cx.add_window(|window, cx| {
            let buffer = MultiBuffer::build_simple(&"a ".to_string().repeat(100), cx);
            let mut editor = Editor::new(EditorMode::full(), buffer, None, window, cx);
            editor.set_soft_wrap_mode(language_settings::SoftWrap::EditorWidth, cx);
            editor
        });
        let cx = &mut VisualTestContext::from_window(*window, cx);
        let editor = window.root(cx).unwrap();
        let style = cx.update(|_, cx| editor.update(cx, |editor, cx| editor.style(cx).clone()));

        for x in 1..=100 {
            let (_, state) = cx.draw(
                Default::default(),
                size(px(200. + 0.13 * x as f32), px(500.)),
                |_, _| EditorElement::new(&editor, style.clone()),
            );

            assert!(
                state.position_map.scroll_max.x == 0.,
                "Soft wrapped editor should have no horizontal scrolling!"
            );
        }
    }

    #[gpui::test]
    async fn test_point_for_position_clipped_rows(cx: &mut TestAppContext) {
        init_test(cx, |_| {});

        let text = "aaa\nbbb";
        let window = cx.add_window(|window, cx| {
            let buffer = MultiBuffer::build_simple(text, cx);
            Editor::new(EditorMode::full(), buffer, None, window, cx)
        });

        let cx = &mut VisualTestContext::from_window(*window, cx);
        let editor = window.root(cx).unwrap();
        let style = editor.update(cx, |editor, cx| editor.style(cx).clone());
        let line_height = window
            .update(cx, |_, window, _| {
                style.text.line_height_in_pixels(window.rem_size())
            })
            .unwrap();

        // the first line is clipped
        let (_, state) = cx.draw(
            point(Pixels::ZERO, Pixels::ZERO - line_height * 1.5),
            size(px(500.), px(500.)),
            |_, _| EditorElement::new(&editor, style),
        );

        // click at the end of the second line
        let target_point = DisplayPoint::new(DisplayRow(1), 3);
        let click_x = state.content_origin.x
            + editor.update_in(cx, |editor, window, cx| {
                editor
                    .snapshot(window, cx)
                    .x_for_display_point(target_point, &editor.text_layout_details(window, cx))
            });

        let point = state
            .position_map
            .point_for_position(point(click_x, px(0.)));
        assert_eq!(point.nearest_valid, target_point);
    }

    #[gpui::test]
    fn test_navigation_overlay_covered_text_highlights_are_replaced(cx: &mut TestAppContext) {
        init_test(cx, |_| {});
        let window = cx.add_window(|window, cx| {
            let buffer = MultiBuffer::build_simple("overlay replacement", cx);
            Editor::new(EditorMode::full(), buffer, None, window, cx)
        });
        let editor = window.root(cx).unwrap();

        editor.update(cx, |editor, cx| {
            let buffer_snapshot = editor.buffer().read(cx).snapshot(cx);
            let target_start = buffer_snapshot.anchor_after(Point::new(0, 0));
            let target_end = buffer_snapshot.anchor_after(Point::new(0, 7));
            let covered_text_end = buffer_snapshot.anchor_after(Point::new(0, 2));

            editor.set_navigation_overlays(
                PRIMARY_NAVIGATION_OVERLAY_KEY,
                vec![navigation_overlay(
                    "ov",
                    target_start..target_end,
                    Some(target_start..covered_text_end),
                )],
                cx,
            );
            assert!(
                editor
                    .text_highlights(
                        HighlightKey::NavigationOverlay(PRIMARY_NAVIGATION_OVERLAY_KEY),
                        cx,
                    )
                    .is_some()
            );

            editor.set_navigation_overlays(
                PRIMARY_NAVIGATION_OVERLAY_KEY,
                vec![navigation_overlay("ov", target_start..target_end, None)],
                cx,
            );
            assert!(
                editor
                    .text_highlights(
                        HighlightKey::NavigationOverlay(PRIMARY_NAVIGATION_OVERLAY_KEY),
                        cx,
                    )
                    .is_none()
            );
        });
    }

    #[gpui::test]
    async fn test_navigation_overlay_repositions_when_editor_width_changes(
        cx: &mut TestAppContext,
    ) {
        init_test(cx, |_| {});
        let text = "jump target overlay ".repeat(16);
        let window = cx.add_window(|window, cx| {
            let buffer = MultiBuffer::build_simple(&text, cx);
            let mut editor = Editor::new(EditorMode::full(), buffer, None, window, cx);
            editor.set_soft_wrap_mode(language_settings::SoftWrap::EditorWidth, cx);
            editor
        });
        let cx = &mut VisualTestContext::from_window(*window, cx);
        let editor = window.root(cx).unwrap();

        editor.update(cx, |editor, cx| {
            let buffer_snapshot = editor.buffer().read(cx).snapshot(cx);
            let target_start = buffer_snapshot.anchor_after(Point::new(0, 30));
            let target_end = buffer_snapshot.anchor_after(Point::new(0, 40));

            editor.set_navigation_overlays(
                PRIMARY_NAVIGATION_OVERLAY_KEY,
                vec![navigation_overlay("jj", target_start..target_end, None)],
                cx,
            );
        });

        let style = cx.update(|_, cx| editor.update(cx, |editor, cx| editor.style(cx).clone()));
        let (_, wide_state) = cx.draw(Default::default(), size(px(520.), px(260.)), |_, _| {
            EditorElement::new(&editor, style.clone())
        });
        let (_, narrow_state) = cx.draw(Default::default(), size(px(140.), px(260.)), |_, _| {
            EditorElement::new(&editor, style.clone())
        });

        let wide_label_layouts = navigation_label_layouts(&wide_state);
        let narrow_label_layouts = navigation_label_layouts(&narrow_state);

        assert_eq!(wide_label_layouts.len(), 1);
        assert_eq!(narrow_label_layouts.len(), 1);

        let wide_label_origin = wide_label_layouts[0].origin;
        let narrow_label_origin = narrow_label_layouts[0].origin;

        assert!(
            narrow_label_origin.y > wide_label_origin.y,
            "expected inline label to move to a later wrapped row when the editor narrows"
        );
        assert!(
            narrow_label_origin.x < wide_label_origin.x,
            "expected inline label to recompute its horizontal position for the wrapped row"
        );
    }

    #[gpui::test]
    fn test_layout_line_numbers(cx: &mut TestAppContext) {
        init_test(cx, |_| {});
        let window = cx.add_window(|window, cx| {
            let buffer = MultiBuffer::build_simple(&sample_text(6, 6, 'a'), cx);
            Editor::new(EditorMode::full(), buffer, None, window, cx)
        });

        let editor = window.root(cx).unwrap();
        let style = editor.update(cx, |editor, cx| editor.style(cx).clone());
        let line_height = window
            .update(cx, |_, window, _| {
                style.text.line_height_in_pixels(window.rem_size())
            })
            .unwrap();
        let element = EditorElement::new(&editor, style);
        let snapshot = window
            .update(cx, |editor, window, cx| editor.snapshot(window, cx))
            .unwrap();

        let layouts = cx
            .update_window(*window, |_, window, cx| {
                element.layout_line_numbers(
                    &test_gutter(line_height, &snapshot),
                    &BTreeMap::default(),
                    Some(DisplayRow(0)),
                    window,
                    cx,
                )
            })
            .unwrap();
        assert_eq!(layouts.len(), 6);

        let relative_rows = window
            .update(cx, |editor, window, cx| {
                let snapshot = editor.snapshot(window, cx);
                snapshot.calculate_relative_line_numbers(
                    &(DisplayRow(0)..DisplayRow(6)),
                    DisplayRow(3),
                    false,
                )
            })
            .unwrap();
        assert_eq!(relative_rows[&DisplayRow(0)], 3);
        assert_eq!(relative_rows[&DisplayRow(1)], 2);
        assert_eq!(relative_rows[&DisplayRow(2)], 1);
        // current line has no relative number
        assert!(!relative_rows.contains_key(&DisplayRow(3)));
        assert_eq!(relative_rows[&DisplayRow(4)], 1);
        assert_eq!(relative_rows[&DisplayRow(5)], 2);

        // works if cursor is before screen
        let relative_rows = window
            .update(cx, |editor, window, cx| {
                let snapshot = editor.snapshot(window, cx);
                snapshot.calculate_relative_line_numbers(
                    &(DisplayRow(3)..DisplayRow(6)),
                    DisplayRow(1),
                    false,
                )
            })
            .unwrap();
        assert_eq!(relative_rows.len(), 3);
        assert_eq!(relative_rows[&DisplayRow(3)], 2);
        assert_eq!(relative_rows[&DisplayRow(4)], 3);
        assert_eq!(relative_rows[&DisplayRow(5)], 4);

        // works if cursor is after screen
        let relative_rows = window
            .update(cx, |editor, window, cx| {
                let snapshot = editor.snapshot(window, cx);
                snapshot.calculate_relative_line_numbers(
                    &(DisplayRow(0)..DisplayRow(3)),
                    DisplayRow(6),
                    false,
                )
            })
            .unwrap();
        assert_eq!(relative_rows.len(), 3);
        assert_eq!(relative_rows[&DisplayRow(0)], 5);
        assert_eq!(relative_rows[&DisplayRow(1)], 4);
        assert_eq!(relative_rows[&DisplayRow(2)], 3);

        let gutter = Gutter {
            row_infos: &(0..6)
                .map(|row| RowInfo {
                    buffer_row: Some(row),
                    diff_status: (row == DELETED_LINE).then(|| {
                        DiffHunkStatus::deleted(
                            buffer_diff::DiffHunkSecondaryStatus::NoSecondaryHunk,
                        )
                    }),
                    ..Default::default()
                })
                .collect::<Vec<_>>(),
            ..test_gutter(line_height, &snapshot)
        };

        const DELETED_LINE: u32 = 3;
        let layouts = cx
            .update_window(*window, |_, window, cx| {
                element.layout_line_numbers(
                    &gutter,
                    &BTreeMap::default(),
                    Some(DisplayRow(0)),
                    window,
                    cx,
                )
            })
            .unwrap();
        assert_eq!(layouts.len(), 5,);
        assert!(
            layouts.get(&MultiBufferRow(DELETED_LINE)).is_none(),
            "Deleted line should not have a line number"
        );
    }

    #[gpui::test]
    async fn test_layout_line_numbers_with_folded_lines(cx: &mut TestAppContext) {
        init_test(cx, |_| {});

        let python_lang = languages::language("python", tree_sitter_python::LANGUAGE.into());

        let window = cx.add_window(|window, cx| {
            let buffer = cx.new(|cx| {
                Buffer::local(
                    indoc::indoc! {"
                        fn test() -> int {
                            return 2;
                        }

                        fn another_test() -> int {
                            # This is a very peculiar method that is hard to grasp.
                            return 4;
                        }
                    "},
                    cx,
                )
                .with_language(python_lang, cx)
            });

            let buffer = MultiBuffer::build_from_buffer(buffer, cx);
            Editor::new(EditorMode::full(), buffer, None, window, cx)
        });

        let editor = window.root(cx).unwrap();
        let style = editor.update(cx, |editor, cx| editor.style(cx).clone());
        let line_height = window
            .update(cx, |_, window, _| {
                style.text.line_height_in_pixels(window.rem_size())
            })
            .unwrap();
        let element = EditorElement::new(&editor, style);
        let snapshot = window
            .update(cx, |editor, window, cx| {
                editor.fold_at(MultiBufferRow(0), window, cx);
                editor.snapshot(window, cx)
            })
            .unwrap();

        let layouts = cx
            .update_window(*window, |_, window, cx| {
                element.layout_line_numbers(
                    &test_gutter(line_height, &snapshot),
                    &BTreeMap::default(),
                    Some(DisplayRow(3)),
                    window,
                    cx,
                )
            })
            .unwrap();
        assert_eq!(layouts.len(), 6);

        let relative_rows = window
            .update(cx, |editor, window, cx| {
                let snapshot = editor.snapshot(window, cx);
                snapshot.calculate_relative_line_numbers(
                    &(DisplayRow(0)..DisplayRow(6)),
                    DisplayRow(3),
                    false,
                )
            })
            .unwrap();
        assert_eq!(relative_rows[&DisplayRow(0)], 3);
        assert_eq!(relative_rows[&DisplayRow(1)], 2);
        assert_eq!(relative_rows[&DisplayRow(2)], 1);
        // current line has no relative number
        assert!(!relative_rows.contains_key(&DisplayRow(3)));
        assert_eq!(relative_rows[&DisplayRow(4)], 1);
        assert_eq!(relative_rows[&DisplayRow(5)], 2);
    }

    #[gpui::test]
    fn test_layout_line_numbers_wrapping(cx: &mut TestAppContext) {
        init_test(cx, |_| {});
        let window = cx.add_window(|window, cx| {
            let buffer = MultiBuffer::build_simple(&sample_text(6, 6, 'a'), cx);
            Editor::new(EditorMode::full(), buffer, None, window, cx)
        });

        update_test_language_settings(cx, &|s| {
            s.defaults.preferred_line_length = Some(5_u32);
            s.defaults.soft_wrap = Some(language_settings::SoftWrap::Bounded);
        });

        let editor = window.root(cx).unwrap();
        let style = editor.update(cx, |editor, cx| editor.style(cx).clone());
        let line_height = window
            .update(cx, |_, window, _| {
                style.text.line_height_in_pixels(window.rem_size())
            })
            .unwrap();
        let element = EditorElement::new(&editor, style);
        let snapshot = window
            .update(cx, |editor, window, cx| editor.snapshot(window, cx))
            .unwrap();

        let layouts = cx
            .update_window(*window, |_, window, cx| {
                element.layout_line_numbers(
                    &test_gutter(line_height, &snapshot),
                    &BTreeMap::default(),
                    Some(DisplayRow(0)),
                    window,
                    cx,
                )
            })
            .unwrap();
        assert_eq!(layouts.len(), 3);

        let relative_rows = window
            .update(cx, |editor, window, cx| {
                let snapshot = editor.snapshot(window, cx);
                snapshot.calculate_relative_line_numbers(
                    &(DisplayRow(0)..DisplayRow(6)),
                    DisplayRow(3),
                    true,
                )
            })
            .unwrap();

        assert_eq!(relative_rows[&DisplayRow(0)], 3);
        assert_eq!(relative_rows[&DisplayRow(1)], 2);
        assert_eq!(relative_rows[&DisplayRow(2)], 1);
        // current line has no relative number
        assert!(!relative_rows.contains_key(&DisplayRow(3)));
        assert_eq!(relative_rows[&DisplayRow(4)], 1);
        assert_eq!(relative_rows[&DisplayRow(5)], 2);

        let layouts = cx
            .update_window(*window, |_, window, cx| {
                element.layout_line_numbers(
                    &Gutter {
                        row_infos: &(0..6)
                            .map(|row| RowInfo {
                                buffer_row: Some(row),
                                diff_status: Some(DiffHunkStatus::deleted(
                                    buffer_diff::DiffHunkSecondaryStatus::NoSecondaryHunk,
                                )),
                                ..Default::default()
                            })
                            .collect::<Vec<_>>(),
                        ..test_gutter(line_height, &snapshot)
                    },
                    &BTreeMap::from_iter([(DisplayRow(0), LineHighlightSpec::default())]),
                    Some(DisplayRow(0)),
                    window,
                    cx,
                )
            })
            .unwrap();
        assert!(
            layouts.is_empty(),
            "Deleted lines should have no line number"
        );

        let relative_rows = window
            .update(cx, |editor, window, cx| {
                let snapshot = editor.snapshot(window, cx);
                snapshot.calculate_relative_line_numbers(
                    &(DisplayRow(0)..DisplayRow(6)),
                    DisplayRow(3),
                    true,
                )
            })
            .unwrap();

        // Deleted lines should still have relative numbers
        assert_eq!(relative_rows[&DisplayRow(0)], 3);
        assert_eq!(relative_rows[&DisplayRow(1)], 2);
        assert_eq!(relative_rows[&DisplayRow(2)], 1);
        // current line, even if deleted, has no relative number
        assert!(!relative_rows.contains_key(&DisplayRow(3)));
        assert_eq!(relative_rows[&DisplayRow(4)], 1);
        assert_eq!(relative_rows[&DisplayRow(5)], 2);
    }

    #[gpui::test]
    async fn test_vim_visual_selections(cx: &mut TestAppContext) {
        init_test(cx, |_| {});

        let window = cx.add_window(|window, cx| {
            let buffer = MultiBuffer::build_simple(&(sample_text(6, 6, 'a') + "\n"), cx);
            Editor::new(EditorMode::full(), buffer, None, window, cx)
        });
        let cx = &mut VisualTestContext::from_window(*window, cx);
        let editor = window.root(cx).unwrap();
        let style = cx.update(|_, cx| editor.update(cx, |editor, cx| editor.style(cx).clone()));

        window
            .update(cx, |editor, window, cx| {
                editor.cursor_offset_on_selection = true;
                editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                    s.select_ranges([
                        Point::new(0, 0)..Point::new(1, 0),
                        Point::new(3, 2)..Point::new(3, 3),
                        Point::new(5, 6)..Point::new(6, 0),
                    ]);
                });
            })
            .unwrap();

        let (_, state) = cx.draw(
            point(px(500.), px(500.)),
            size(px(500.), px(500.)),
            |_, _| EditorElement::new(&editor, style),
        );

        assert_eq!(state.selections.len(), 1);
        let local_selections = &state.selections[0].1;
        assert_eq!(local_selections.len(), 3);
        // moves cursor back one line
        assert_eq!(
            local_selections[0].head,
            DisplayPoint::new(DisplayRow(0), 6)
        );
        assert_eq!(
            local_selections[0].range,
            DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(1), 0)
        );

        // moves cursor back one column
        assert_eq!(
            local_selections[1].range,
            DisplayPoint::new(DisplayRow(3), 2)..DisplayPoint::new(DisplayRow(3), 3)
        );
        assert_eq!(
            local_selections[1].head,
            DisplayPoint::new(DisplayRow(3), 2)
        );

        // leaves cursor on the max point
        assert_eq!(
            local_selections[2].range,
            DisplayPoint::new(DisplayRow(5), 6)..DisplayPoint::new(DisplayRow(6), 0)
        );
        assert_eq!(
            local_selections[2].head,
            DisplayPoint::new(DisplayRow(6), 0)
        );

        // active lines does not include 1 (even though the range of the selection does)
        assert_eq!(
            state.active_rows.keys().cloned().collect::<Vec<_>>(),
            vec![DisplayRow(0), DisplayRow(3), DisplayRow(5), DisplayRow(6)]
        );
    }

    #[gpui::test]
    fn test_layout_with_placeholder_text_and_blocks(cx: &mut TestAppContext) {
        init_test(cx, |_| {});

        let window = cx.add_window(|window, cx| {
            let buffer = MultiBuffer::build_simple("", cx);
            Editor::new(EditorMode::full(), buffer, None, window, cx)
        });
        let cx = &mut VisualTestContext::from_window(*window, cx);
        let editor = window.root(cx).unwrap();
        let style = cx.update(|_, cx| editor.update(cx, |editor, cx| editor.style(cx).clone()));
        window
            .update(cx, |editor, window, cx| {
                editor.set_placeholder_text("hello", window, cx);
                editor.insert_blocks(
                    [BlockProperties {
                        style: BlockStyle::Fixed,
                        placement: BlockPlacement::Above(Anchor::Min),
                        height: Some(3),
                        render: Arc::new(|cx| div().h(3. * cx.window.line_height()).into_any()),
                        priority: 0,
                    }],
                    None,
                    cx,
                );

                // Blur the editor so that it displays placeholder text.
                window.blur();
            })
            .unwrap();

        let (_, state) = cx.draw(
            point(px(500.), px(500.)),
            size(px(500.), px(500.)),
            |_, _| EditorElement::new(&editor, style),
        );
        assert_eq!(state.position_map.line_layouts.len(), 4);
        assert_eq!(state.line_numbers.len(), 1);
        assert_eq!(
            state
                .line_numbers
                .get(&MultiBufferRow(0))
                .map(|line_number| line_number
                    .segments
                    .first()
                    .unwrap()
                    .shaped_line
                    .text
                    .as_ref()),
            Some("1")
        );
    }

    #[gpui::test]
    fn test_all_invisibles_drawing(cx: &mut TestAppContext) {
        const TAB_SIZE: u32 = 4;

        let input_text = "\t \t|\t| a b";
        let expected_invisibles = vec![
            Invisible::Tab {
                line_start_offset: 0,
                line_end_offset: TAB_SIZE as usize,
            },
            Invisible::Whitespace {
                line_start_offset: TAB_SIZE as usize,
                line_end_offset: TAB_SIZE as usize + 1,
            },
            Invisible::Tab {
                line_start_offset: TAB_SIZE as usize + 1,
                line_end_offset: TAB_SIZE as usize * 2,
            },
            Invisible::Tab {
                line_start_offset: TAB_SIZE as usize * 2 + 1,
                line_end_offset: TAB_SIZE as usize * 3,
            },
            Invisible::Whitespace {
                line_start_offset: TAB_SIZE as usize * 3 + 1,
                line_end_offset: TAB_SIZE as usize * 3 + 2,
            },
            Invisible::Whitespace {
                line_start_offset: TAB_SIZE as usize * 3 + 3,
                line_end_offset: TAB_SIZE as usize * 3 + 4,
            },
        ];
        assert_eq!(
            expected_invisibles.len(),
            input_text
                .chars()
                .filter(|initial_char| initial_char.is_whitespace())
                .count(),
            "Hardcoded expected invisibles differ from the actual ones in '{input_text}'"
        );

        for show_line_numbers in [true, false] {
            init_test(cx, |s| {
                s.defaults.show_whitespaces = Some(ShowWhitespaceSetting::All);
                s.defaults.tab_size = NonZeroU32::new(TAB_SIZE);
            });

            let actual_invisibles = collect_invisibles_from_new_editor(
                cx,
                EditorMode::full(),
                input_text,
                px(500.0),
                show_line_numbers,
            );

            assert_eq!(expected_invisibles, actual_invisibles);
        }
    }

    #[gpui::test]
    fn test_multibyte_whitespace_uses_utf8_byte_offsets(cx: &mut TestAppContext) {
        init_test(cx, |s| {
            s.defaults.show_whitespaces = Some(ShowWhitespaceSetting::All);
        });

        // Regression test for #49186. NBSP (U+00A0) is rendered via the invisible
        // character `replacement` pipeline, which flushes the internal `line`
        // scratch buffer mid-line. Any whitespace invisible that follows must use
        // the absolute byte offset within the logical line (here: byte 4 for the
        // trailing ASCII space), not an offset relative to the post-flush buffer.
        let actual_invisibles = collect_invisibles_from_new_editor(
            cx,
            EditorMode::full(),
            "a\u{00A0}b ",
            px(500.0),
            false,
        );

        assert_eq!(
            actual_invisibles,
            vec![Invisible::Whitespace {
                line_start_offset: 4,
                line_end_offset: 5,
            }]
        );
    }

    #[gpui::test]
    fn test_replacement_chunks_are_clipped_to_max_line_len(cx: &mut TestAppContext) {
        init_test(cx, |_| {});

        let window = cx.add_window(|window, cx| {
            let buffer = MultiBuffer::build_simple("", cx);
            Editor::new(EditorMode::full(), buffer, None, window, cx)
        });
        let cx = &mut VisualTestContext::from_window(*window, cx);
        let editor = window.root(cx).unwrap();
        let style = cx.update(|_, cx| editor.update(cx, |editor, cx| editor.style(cx).clone()));
        let editor_mode = EditorMode::full();
        let max_line_len = "\u{00a0}abcdef".len();

        window
            .update(cx, |_, window, cx| {
                let chunks = std::iter::once(HighlightedChunk {
                    text: "\u{00a0}",
                    style: None,
                    is_tab: false,
                    is_inlay: false,
                    replacement: Some(ChunkReplacement::Str("\u{2007}".into())),
                })
                .chain(std::iter::once(HighlightedChunk {
                    text: "abcdefghi",
                    style: None,
                    is_tab: false,
                    is_inlay: false,
                    replacement: None,
                }))
                .chain(
                    std::iter::repeat_with(|| HighlightedChunk {
                        text: "\u{00a0}",
                        style: None,
                        is_tab: false,
                        is_inlay: false,
                        replacement: Some(ChunkReplacement::Str("\u{2007}".into())),
                    })
                    .take(8),
                );

                let layouts = LineWithInvisibles::from_chunks(
                    chunks,
                    &style,
                    max_line_len,
                    1,
                    &editor_mode,
                    px(500.),
                    |_| false,
                    &[],
                    window,
                    cx,
                );

                assert_eq!(layouts.len(), 1);
                assert_eq!(layouts[0].len, max_line_len);
                assert!(layouts[0].fragments.len() <= max_line_len);
            })
            .unwrap();
    }

    #[gpui::test]
    fn test_invisibles_dont_appear_in_certain_editors(cx: &mut TestAppContext) {
        init_test(cx, |s| {
            s.defaults.show_whitespaces = Some(ShowWhitespaceSetting::All);
            s.defaults.tab_size = NonZeroU32::new(4);
        });

        for editor_mode_without_invisibles in [
            EditorMode::SingleLine,
            EditorMode::AutoHeight {
                min_lines: 1,
                max_lines: Some(100),
            },
        ] {
            for show_line_numbers in [true, false] {
                let invisibles = collect_invisibles_from_new_editor(
                    cx,
                    editor_mode_without_invisibles.clone(),
                    "\t\t\t| | a b",
                    px(500.0),
                    show_line_numbers,
                );
                assert!(
                    invisibles.is_empty(),
                    "For editor mode {editor_mode_without_invisibles:?} no invisibles was expected but got {invisibles:?}"
                );
            }
        }
    }

    #[gpui::test]
    fn test_wrapped_invisibles_drawing(cx: &mut TestAppContext) {
        let tab_size = 4;
        let input_text = "a\tbcd     ".repeat(9);
        let repeated_invisibles = [
            Invisible::Tab {
                line_start_offset: 1,
                line_end_offset: tab_size as usize,
            },
            Invisible::Whitespace {
                line_start_offset: tab_size as usize + 3,
                line_end_offset: tab_size as usize + 4,
            },
            Invisible::Whitespace {
                line_start_offset: tab_size as usize + 4,
                line_end_offset: tab_size as usize + 5,
            },
            Invisible::Whitespace {
                line_start_offset: tab_size as usize + 5,
                line_end_offset: tab_size as usize + 6,
            },
            Invisible::Whitespace {
                line_start_offset: tab_size as usize + 6,
                line_end_offset: tab_size as usize + 7,
            },
            Invisible::Whitespace {
                line_start_offset: tab_size as usize + 7,
                line_end_offset: tab_size as usize + 8,
            },
        ];
        let expected_invisibles = std::iter::once(repeated_invisibles)
            .cycle()
            .take(9)
            .flatten()
            .collect::<Vec<_>>();
        assert_eq!(
            expected_invisibles.len(),
            input_text
                .chars()
                .filter(|initial_char| initial_char.is_whitespace())
                .count(),
            "Hardcoded expected invisibles differ from the actual ones in '{input_text}'"
        );
        info!("Expected invisibles: {expected_invisibles:?}");

        init_test(cx, |_| {});

        // Put the same string with repeating whitespace pattern into editors of various size,
        // take deliberately small steps during resizing, to put all whitespace kinds near the wrap point.
        let resize_step = 10.0;
        let mut editor_width = 200.0;
        while editor_width <= 1000.0 {
            for show_line_numbers in [true, false] {
                update_test_language_settings(cx, &|s| {
                    s.defaults.tab_size = NonZeroU32::new(tab_size);
                    s.defaults.show_whitespaces = Some(ShowWhitespaceSetting::All);
                    s.defaults.preferred_line_length = Some(editor_width as u32);
                    s.defaults.soft_wrap = Some(language_settings::SoftWrap::Bounded);
                });

                let actual_invisibles = collect_invisibles_from_new_editor(
                    cx,
                    EditorMode::full(),
                    &input_text,
                    px(editor_width),
                    show_line_numbers,
                );

                // Whatever the editor size is, ensure it has the same invisible kinds in the same order
                // (no good guarantees about the offsets: wrapping could trigger padding and its tests should check the offsets).
                let mut i = 0;
                for (actual_index, actual_invisible) in actual_invisibles.iter().enumerate() {
                    i = actual_index;
                    match expected_invisibles.get(i) {
                        Some(expected_invisible) => match (expected_invisible, actual_invisible) {
                            (Invisible::Whitespace { .. }, Invisible::Whitespace { .. })
                            | (Invisible::Tab { .. }, Invisible::Tab { .. }) => {}
                            _ => {
                                panic!(
                                    "At index {i}, expected invisible {expected_invisible:?} does not match actual {actual_invisible:?} by kind. Actual invisibles: {actual_invisibles:?}"
                                )
                            }
                        },
                        None => {
                            panic!("Unexpected extra invisible {actual_invisible:?} at index {i}")
                        }
                    }
                }
                let missing_expected_invisibles = &expected_invisibles[i + 1..];
                assert!(
                    missing_expected_invisibles.is_empty(),
                    "Missing expected invisibles after index {i}: {missing_expected_invisibles:?}"
                );

                editor_width += resize_step;
            }
        }
    }

    fn collect_invisibles_from_new_editor(
        cx: &mut TestAppContext,
        editor_mode: EditorMode,
        input_text: &str,
        editor_width: Pixels,
        show_line_numbers: bool,
    ) -> Vec<Invisible> {
        info!(
            "Creating editor with mode {editor_mode:?}, width {}px and text '{input_text}'",
            f32::from(editor_width)
        );
        let window = cx.add_window(|window, cx| {
            let buffer = MultiBuffer::build_simple(input_text, cx);
            Editor::new(editor_mode, buffer, None, window, cx)
        });
        let cx = &mut VisualTestContext::from_window(*window, cx);
        let editor = window.root(cx).unwrap();

        let style = editor.update(cx, |editor, cx| editor.style(cx).clone());
        window
            .update(cx, |editor, _, cx| {
                editor.set_soft_wrap_mode(language_settings::SoftWrap::EditorWidth, cx);
                editor.set_wrap_width(Some(editor_width), cx);
                editor.set_show_line_numbers(show_line_numbers, cx);
            })
            .unwrap();
        let (_, state) = cx.draw(
            point(px(500.), px(500.)),
            size(px(500.), px(500.)),
            |_, _| EditorElement::new(&editor, style),
        );
        state
            .position_map
            .line_layouts
            .iter()
            .flat_map(|line_with_invisibles| &line_with_invisibles.invisibles)
            .cloned()
            .collect()
    }

    #[gpui::test]
    fn test_merge_overlapping_ranges() {
        let base_bg = Hsla::white();
        let color1 = Hsla {
            h: 0.0,
            s: 0.5,
            l: 0.5,
            a: 0.5,
        };
        let color2 = Hsla {
            h: 120.0,
            s: 0.5,
            l: 0.5,
            a: 0.5,
        };

        let display_point = |col| DisplayPoint::new(DisplayRow(0), col);
        let cols = |v: &Vec<(Range<DisplayPoint>, Hsla)>| -> Vec<(u32, u32)> {
            v.iter()
                .map(|(r, _)| (r.start.column(), r.end.column()))
                .collect()
        };

        // Test overlapping ranges blend colors
        let overlapping = vec![
            (display_point(5)..display_point(15), color1),
            (display_point(10)..display_point(20), color2),
        ];
        let result = EditorElement::merge_overlapping_ranges(overlapping, base_bg);
        assert_eq!(cols(&result), vec![(5, 10), (10, 15), (15, 20)]);

        // Test middle segment should have blended color
        let blended = Hsla::blend(Hsla::blend(base_bg, color1), color2);
        assert_eq!(result[1].1, blended);

        // Test adjacent same-color ranges merge
        let adjacent_same = vec![
            (display_point(5)..display_point(10), color1),
            (display_point(10)..display_point(15), color1),
        ];
        let result = EditorElement::merge_overlapping_ranges(adjacent_same, base_bg);
        assert_eq!(cols(&result), vec![(5, 15)]);

        // Test contained range splits
        let contained = vec![
            (display_point(5)..display_point(20), color1),
            (display_point(10)..display_point(15), color2),
        ];
        let result = EditorElement::merge_overlapping_ranges(contained, base_bg);
        assert_eq!(cols(&result), vec![(5, 10), (10, 15), (15, 20)]);

        // Test multiple overlaps split at every boundary
        let color3 = Hsla {
            h: 240.0,
            s: 0.5,
            l: 0.5,
            a: 0.5,
        };
        let complex = vec![
            (display_point(5)..display_point(12), color1),
            (display_point(8)..display_point(16), color2),
            (display_point(10)..display_point(14), color3),
        ];
        let result = EditorElement::merge_overlapping_ranges(complex, base_bg);
        assert_eq!(
            cols(&result),
            vec![(5, 8), (8, 10), (10, 12), (12, 14), (14, 16)]
        );
    }

    #[gpui::test]
    fn test_bg_segments_per_row() {
        let base_bg = Hsla::white();

        // Case A: selection spans three display rows: row 1 [5, end), full row 2, row 3 [0, 7)
        {
            let selection_color = Hsla {
                h: 200.0,
                s: 0.5,
                l: 0.5,
                a: 0.5,
            };
            let player_color = PlayerColor {
                cursor: selection_color,
                background: selection_color,
                selection: selection_color,
            };

            let spanning_selection = SelectionLayout {
                head: DisplayPoint::new(DisplayRow(3), 7),
                cursor_shape: CursorShape::Bar,
                is_newest: true,
                is_local: true,
                range: DisplayPoint::new(DisplayRow(1), 5)..DisplayPoint::new(DisplayRow(3), 7),
                active_rows: DisplayRow(1)..DisplayRow(4),
                user_name: None,
            };

            let selections = vec![(player_color, vec![spanning_selection])];
            let result = EditorElement::bg_segments_per_row(
                DisplayRow(0)..DisplayRow(5),
                &selections,
                [].into_iter(),
                base_bg,
            );

            assert_eq!(result.len(), 5);
            assert!(result[0].is_empty());
            assert_eq!(result[1].len(), 1);
            assert_eq!(result[2].len(), 1);
            assert_eq!(result[3].len(), 1);
            assert!(result[4].is_empty());

            assert_eq!(result[1][0].0.start, DisplayPoint::new(DisplayRow(1), 5));
            assert_eq!(result[1][0].0.end.row(), DisplayRow(1));
            assert_eq!(result[1][0].0.end.column(), u32::MAX);
            assert_eq!(result[2][0].0.start, DisplayPoint::new(DisplayRow(2), 0));
            assert_eq!(result[2][0].0.end.row(), DisplayRow(2));
            assert_eq!(result[2][0].0.end.column(), u32::MAX);
            assert_eq!(result[3][0].0.start, DisplayPoint::new(DisplayRow(3), 0));
            assert_eq!(result[3][0].0.end, DisplayPoint::new(DisplayRow(3), 7));
        }

        // Case B: selection ends exactly at the start of row 3, excluding row 3
        {
            let selection_color = Hsla {
                h: 120.0,
                s: 0.5,
                l: 0.5,
                a: 0.5,
            };
            let player_color = PlayerColor {
                cursor: selection_color,
                background: selection_color,
                selection: selection_color,
            };

            let selection = SelectionLayout {
                head: DisplayPoint::new(DisplayRow(2), 0),
                cursor_shape: CursorShape::Bar,
                is_newest: true,
                is_local: true,
                range: DisplayPoint::new(DisplayRow(1), 5)..DisplayPoint::new(DisplayRow(3), 0),
                active_rows: DisplayRow(1)..DisplayRow(3),
                user_name: None,
            };

            let selections = vec![(player_color, vec![selection])];
            let result = EditorElement::bg_segments_per_row(
                DisplayRow(0)..DisplayRow(4),
                &selections,
                [].into_iter(),
                base_bg,
            );

            assert_eq!(result.len(), 4);
            assert!(result[0].is_empty());
            assert_eq!(result[1].len(), 1);
            assert_eq!(result[2].len(), 1);
            assert!(result[3].is_empty());

            assert_eq!(result[1][0].0.start, DisplayPoint::new(DisplayRow(1), 5));
            assert_eq!(result[1][0].0.end.row(), DisplayRow(1));
            assert_eq!(result[1][0].0.end.column(), u32::MAX);
            assert_eq!(result[2][0].0.start, DisplayPoint::new(DisplayRow(2), 0));
            assert_eq!(result[2][0].0.end.row(), DisplayRow(2));
            assert_eq!(result[2][0].0.end.column(), u32::MAX);
        }
    }

    #[cfg(test)]
    fn generate_test_run(len: usize, color: Hsla) -> TextRun {
        TextRun {
            len,
            color,
            ..Default::default()
        }
    }

    #[gpui::test]
    fn test_split_runs_by_bg_segments(cx: &mut gpui::TestAppContext) {
        init_test(cx, |_| {});

        let dx = |start: u32, end: u32| {
            DisplayPoint::new(DisplayRow(0), start)..DisplayPoint::new(DisplayRow(0), end)
        };

        let text_color = Hsla {
            h: 210.0,
            s: 0.1,
            l: 0.4,
            a: 1.0,
        };
        let bg_1 = Hsla {
            h: 30.0,
            s: 0.6,
            l: 0.8,
            a: 1.0,
        };
        let bg_2 = Hsla {
            h: 200.0,
            s: 0.6,
            l: 0.2,
            a: 1.0,
        };
        let min_contrast = 45.0;
        let adjusted_bg1 = ensure_minimum_contrast(text_color, bg_1, min_contrast);
        let adjusted_bg2 = ensure_minimum_contrast(text_color, bg_2, min_contrast);

        // Case A: single run; disjoint segments inside the run
        {
            let runs = vec![generate_test_run(20, text_color)];
            let segs = vec![(dx(5, 10), bg_1), (dx(12, 16), bg_2)];
            let out = LineWithInvisibles::split_runs_by_bg_segments(&runs, &segs, min_contrast, 0);
            // Expected slices: [0,5) [5,10) [10,12) [12,16) [16,20)
            assert_eq!(
                out.iter().map(|r| r.len).collect::<Vec<_>>(),
                vec![5, 5, 2, 4, 4]
            );
            assert_eq!(out[0].color, text_color);
            assert_eq!(out[1].color, adjusted_bg1);
            assert_eq!(out[2].color, text_color);
            assert_eq!(out[3].color, adjusted_bg2);
            assert_eq!(out[4].color, text_color);
        }

        // Case B: multiple runs; segment extends to end of line (u32::MAX)
        {
            let runs = vec![
                generate_test_run(8, text_color),
                generate_test_run(7, text_color),
            ];
            let segs = vec![(dx(6, u32::MAX), bg_1)];
            let out = LineWithInvisibles::split_runs_by_bg_segments(&runs, &segs, min_contrast, 0);
            // Expected slices across runs: [0,6) [6,8) | [0,7)
            assert_eq!(out.iter().map(|r| r.len).collect::<Vec<_>>(), vec![6, 2, 7]);
            assert_eq!(out[0].color, text_color);
            assert_eq!(out[1].color, adjusted_bg1);
            assert_eq!(out[2].color, adjusted_bg1);
        }

        // Case C: multi-byte characters
        {
            // for text: "Hello 🌍 世界!"
            let runs = vec![
                generate_test_run(5, text_color), // "Hello"
                generate_test_run(6, text_color), // " 🌍 "
                generate_test_run(6, text_color), // "世界"
                generate_test_run(1, text_color), // "!"
            ];
            // selecting "🌍 世"
            let segs = vec![(dx(6, 14), bg_1)];
            let out = LineWithInvisibles::split_runs_by_bg_segments(&runs, &segs, min_contrast, 0);
            // "Hello" | " " | "🌍 " | "世" | "界" | "!"
            assert_eq!(
                out.iter().map(|r| r.len).collect::<Vec<_>>(),
                vec![5, 1, 5, 3, 3, 1]
            );
            assert_eq!(out[0].color, text_color); // "Hello"
            assert_eq!(out[2].color, adjusted_bg1); // "🌍 "
            assert_eq!(out[3].color, adjusted_bg1); // "世"
            assert_eq!(out[4].color, text_color); // "界"
            assert_eq!(out[5].color, text_color); // "!"
        }

        // Case D: split multiple consecutive text runs with segments
        {
            let segs = vec![
                (dx(2, 4), bg_1),   // selecting "cd"
                (dx(4, 8), bg_2),   // selecting "efgh"
                (dx(9, 11), bg_1),  // selecting "jk"
                (dx(12, 16), bg_2), // selecting "mnop"
                (dx(18, 19), bg_1), // selecting "s"
            ];

            // for text: "abcdef"
            let runs = vec![
                generate_test_run(2, text_color), // ab
                generate_test_run(4, text_color), // cdef
            ];
            let out = LineWithInvisibles::split_runs_by_bg_segments(&runs, &segs, min_contrast, 0);
            // new splits "ab", "cd", "ef"
            assert_eq!(out.iter().map(|r| r.len).collect::<Vec<_>>(), vec![2, 2, 2]);
            assert_eq!(out[0].color, text_color);
            assert_eq!(out[1].color, adjusted_bg1);
            assert_eq!(out[2].color, adjusted_bg2);

            // for text: "ghijklmn"
            let runs = vec![
                generate_test_run(3, text_color), // ghi
                generate_test_run(2, text_color), // jk
                generate_test_run(3, text_color), // lmn
            ];
            let out = LineWithInvisibles::split_runs_by_bg_segments(&runs, &segs, min_contrast, 6); // 2 + 4 from first run
            // new splits "gh", "i", "jk", "l", "mn"
            assert_eq!(
                out.iter().map(|r| r.len).collect::<Vec<_>>(),
                vec![2, 1, 2, 1, 2]
            );
            assert_eq!(out[0].color, adjusted_bg2);
            assert_eq!(out[1].color, text_color);
            assert_eq!(out[2].color, adjusted_bg1);
            assert_eq!(out[3].color, text_color);
            assert_eq!(out[4].color, adjusted_bg2);

            // for text: "opqrs"
            let runs = vec![
                generate_test_run(1, text_color), // o
                generate_test_run(4, text_color), // pqrs
            ];
            let out = LineWithInvisibles::split_runs_by_bg_segments(&runs, &segs, min_contrast, 14); // 6 + 3 + 2 + 3 from first two runs
            // new splits "o", "p", "qr", "s"
            assert_eq!(
                out.iter().map(|r| r.len).collect::<Vec<_>>(),
                vec![1, 1, 2, 1]
            );
            assert_eq!(out[0].color, adjusted_bg2);
            assert_eq!(out[1].color, adjusted_bg2);
            assert_eq!(out[2].color, text_color);
            assert_eq!(out[3].color, adjusted_bg1);
        }
    }

    #[test]
    fn test_spacer_pattern_period() {
        // line height is smaller than target height, so we just return half the line height
        assert_eq!(EditorElement::spacer_pattern_period(10.0, 20.0), 5.0);

        // line height is exactly half the target height, perfect match
        assert_eq!(EditorElement::spacer_pattern_period(20.0, 10.0), 10.0);

        // line height is close to half the target height
        assert_eq!(EditorElement::spacer_pattern_period(20.0, 9.0), 10.0);

        // line height is close to 1/4 the target height
        assert_eq!(EditorElement::spacer_pattern_period(20.0, 4.8), 5.0);
    }

    #[gpui::test(iterations = 100)]
    fn test_random_spacer_pattern_period(mut rng: StdRng) {
        let line_height = rng.next_u32() as f32;
        let target_height = rng.next_u32() as f32;

        let result = EditorElement::spacer_pattern_period(line_height, target_height);

        let k = line_height / result;
        assert!(k - k.round() < 0.0000001); // approximately integer
        assert!((k.round() as u32).is_multiple_of(2));
    }

    #[test]
    fn test_calculate_wrap_width() {
        let editor_width = px(800.0);
        let em_width = px(8.0);

        assert_eq!(
            calculate_wrap_width(SoftWrap::GitDiff, editor_width, em_width),
            None,
        );

        assert_eq!(
            calculate_wrap_width(SoftWrap::None, editor_width, em_width),
            Some(px((MAX_LINE_LEN as f32 / 2.0 * 8.0).ceil())),
        );

        assert_eq!(
            calculate_wrap_width(SoftWrap::EditorWidth, editor_width, em_width),
            Some(px(800.0)),
        );

        assert_eq!(
            calculate_wrap_width(SoftWrap::Bounded(72), editor_width, em_width),
            Some(px((72.0 * 8.0_f32).ceil())),
        );
        assert_eq!(
            calculate_wrap_width(SoftWrap::Bounded(200), px(400.0), em_width),
            Some(px(400.0)),
        );
    }
}
