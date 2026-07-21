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
mod diff_hunk_controls;
mod document_colors;
mod editor_layout;
mod element_helpers;
mod guides_layout;
mod gutter;
mod gutter_controls;
mod gutter_indicators;
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
mod metrics_layout;
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

                    let layout_data::EditorMetrics {
                        font_size,
                        line_height,
                        em_width,
                        em_advance,
                        em_layout_width,
                        glyph_grid_cell,
                        gutter_dimensions,
                        text_width,
                        vertical_scrollbar_width,
                        minimap_width,
                        right_margin,
                        editor_width,
                        editor_margins,
                    } = self.layout_metrics(
                        bounds,
                        &snapshot,
                        style,
                        window.rem_size(),
                        window,
                        cx,
                    );

                    snapshot = self.update_snapshot_layout(
                        bounds,
                        snapshot,
                        gutter_dimensions,
                        line_height,
                        editor_width,
                        em_advance,
                        em_layout_width,
                        window,
                        cx,
                    );

                    let surface = Self::layout_surface(bounds, text_width, &editor_margins, window);
                    let hitbox = surface.hitbox;
                    let gutter_hitbox = surface.gutter_hitbox;
                    let text_hitbox = surface.text_hitbox;
                    let content_offset = surface.content_offset;
                    let content_origin = surface.content_origin;

                    let height_in_lines = f64::from(bounds.size.height / line_height);
                    let max_scroll_row = snapshot.max_point().row().as_f64();

                    // The max scroll position for the top of the window
                    let scroll_beyond_last_line = self.editor.read(cx).scroll_beyond_last_line(cx);
                    let max_scroll_top = match scroll_beyond_last_line {
                        ScrollBeyondLastLine::OnePage => max_scroll_row,
                        ScrollBeyondLastLine::Off => {
                            (max_scroll_row - height_in_lines + 1.).max(0.)
                        }
                        ScrollBeyondLastLine::VerticalScrollMargin => {
                            let settings = EditorSettings::get_global(cx);
                            (max_scroll_row - height_in_lines
                                + 1.
                                + settings.vertical_scroll_margin)
                                .max(0.)
                        }
                    };

                    let layout_data::VerticalAutoscroll {
                        autoscroll_request,
                        autoscroll_containing_element,
                        needs_horizontal_autoscroll,
                    } = self.layout_vertical_autoscroll(
                        bounds,
                        line_height,
                        max_scroll_top,
                        &mut snapshot,
                        window,
                        cx,
                    );

                    let mut scroll_position = snapshot.scroll_position();
                    if !line_height.is_zero() {
                        scroll_position.y = window
                            .pixel_snap_f64(scroll_position.y * f64::from(line_height))
                            / f64::from(line_height);
                    }
                    let visible_rows =
                        Self::visible_rows(bounds, line_height, scroll_position, &snapshot, window);
                    let max_row = visible_rows.max_row;
                    let start_row = visible_rows.start_row;
                    let end_row = visible_rows.end_row;
                    let row_infos = visible_rows.row_infos;
                    let start_anchor = visible_rows.start_anchor;
                    let end_anchor = visible_rows.end_anchor;
                    let is_row_soft_wrapped = |row: usize| {
                        row_infos
                            .get(row)
                            .is_none_or(|info| info.buffer_row.is_none())
                    };

                    let mut highlighted_rows = self
                        .editor
                        .update(cx, |editor, cx| editor.highlighted_display_rows(window, cx));

                    let mut highlighted_ranges = self.collect_background_highlights(
                        start_anchor,
                        end_anchor,
                        start_row,
                        end_row,
                        max_row,
                        &snapshot,
                        window,
                        cx,
                    );

                    self.add_diff_and_drag_highlights(
                        &mut highlighted_rows,
                        &row_infos,
                        start_row,
                        &snapshot,
                        cx,
                    );

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

                    let (local_selections, selected_buffer_ids, latest_selection_anchors) =
                        self.collect_selection_inputs(start_anchor, end_anchor, &snapshot, cx);

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

                    let layout_data::RowActivity {
                        current_selection_head,
                        run_indicator_rows,
                        mut breakpoint_rows,
                    } = self.layout_row_activity(
                        start_row..end_row,
                        &snapshot,
                        &mut active_rows,
                        window,
                        cx,
                    );

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

                    let scrollbar_layout_information = self.layout_scrollbar_information(
                        &snapshot,
                        text_hitbox.bounds,
                        glyph_grid_cell,
                        max_row,
                        line_height,
                        em_advance,
                        editor_width,
                        is_row_soft_wrapped,
                        scroll_beyond_last_line,
                        style,
                        window,
                        cx,
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

                    let layout_data::ScrollPositionLayout {
                        scroll_position,
                        scroll_pixel_position,
                        scroll_max,
                    } = self.layout_scroll_position(
                        scroll_position,
                        max_scroll_top,
                        start_row,
                        editor_width,
                        scroll_width,
                        em_advance,
                        em_layout_width,
                        line_height,
                        &line_layouts,
                        needs_horizontal_autoscroll,
                        autoscroll_request,
                        window,
                        cx,
                    );
                    let layout_data::StickyHeaderLayouts {
                        sticky_headers,
                        indent_guides,
                    } = self.layout_sticky_headers_and_guides(
                        is_minimap,
                        is_singleton,
                        &snapshot,
                        editor_width,
                        is_row_soft_wrapped,
                        line_height,
                        scroll_pixel_position,
                        preliminary_scroll_pixel_position,
                        content_origin,
                        &gutter_dimensions,
                        &gutter_hitbox,
                        &text_hitbox,
                        current_selection_head,
                        start_buffer_row..end_buffer_row,
                        indent_guides,
                        window,
                        cx,
                    );

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

                    let layout_data::InlineDecorationLayouts {
                        inline_diagnostics,
                        inline_blame_layout,
                        inline_code_actions,
                    } = self.layout_inline_decorations(
                        &line_layouts,
                        &crease_trailers,
                        &row_block_types,
                        &row_infos,
                        content_origin,
                        scroll_position,
                        scroll_pixel_position,
                        edit_prediction_popover_origin,
                        newest_selection_head,
                        start_row,
                        end_row,
                        line_height,
                        em_width,
                        style,
                        &snapshot,
                        window,
                        cx,
                    );

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

                    let layout_data::GutterIndicatorLayouts {
                        test_indicators,
                        bookmarks,
                        breakpoints,
                        diff_review_button,
                    } = self.layout_gutter_indicators(
                        &gutter,
                        start_row..end_row,
                        &row_infos,
                        &snapshot,
                        &run_indicator_rows,
                        &mut breakpoint_rows,
                        gutter_settings,
                        gutter_dimensions,
                        line_height,
                        em_width,
                        window,
                        cx,
                    );

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

                    let (tab_invisible, space_invisible) =
                        self.layout_invisible_symbols(font_size, window, cx);

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

impl IntoElement for EditorElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

#[cfg(test)]
#[path = "element/tests/mod.rs"]
mod tests;
