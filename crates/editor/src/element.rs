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
mod paint;
mod paint_background;
mod paint_helpers;
mod position_map;
mod prepaint_helpers;
mod register_actions;
mod request_layout;
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
        self.request_layout_impl(window, cx)
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
        self.paint_impl(bounds, layout, window, cx);
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
#[path = "element/tests/mod.rs"]
mod tests;
