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
                    let (snapshot, is_read_only) = self.editor.update(cx, |editor, cx| {
                        (editor.snapshot(window, cx), editor.read_only(cx))
                    });
                    let style = &self.style;

                    let initial_layout = self.layout_initial_prepaint(
                        bounds,
                        snapshot,
                        style,
                        window.rem_size(),
                        window,
                        cx,
                    );

                    let initial_prepaint_layout::InitialPrepaintLayout {
                        snapshot,
                        font_size,
                        line_height,
                        em_width,
                        em_advance,
                        em_layout_width,
                        glyph_grid_cell,
                        gutter_dimensions,
                        vertical_scrollbar_width,
                        minimap_width,
                        right_margin,
                        editor_width,
                        editor_margins,
                        hitbox,
                        gutter_hitbox,
                        text_hitbox,
                        content_offset,
                        content_origin,
                        height_in_lines,
                        max_scroll_top,
                        scroll_beyond_last_line,
                        autoscroll_request,
                        autoscroll_containing_element,
                        needs_horizontal_autoscroll,
                        scroll_position,
                        visible_rows,
                    } = initial_layout;

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

                    let highlight_inputs::HighlightInputs {
                        highlighted_rows,
                        mut highlighted_ranges,
                        highlighted_gutter_ranges,
                        document_colors,
                        redacted_ranges,
                    } = self.collect_highlight_inputs(
                        start_anchor,
                        end_anchor,
                        start_row,
                        end_row,
                        max_row,
                        &row_infos,
                        &snapshot,
                        window,
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

                    let layout_data::LineSetupLayouts {
                        line_numbers,
                        mut expand_toggles,
                        mut crease_toggles,
                        crease_trailers,
                        display_hunks,
                        mut line_layouts,
                    } = self.layout_line_setup(
                        &gutter,
                        &active_rows,
                        current_selection_head,
                        &gutter_hitbox,
                        gutter_dimensions,
                        em_width,
                        line_height,
                        scroll_position,
                        start_row..end_row,
                        &row_infos,
                        &snapshot,
                        &mut highlighted_ranges,
                        &selections,
                        document_colors.as_ref(),
                        editor_width,
                        is_row_soft_wrapped,
                        window,
                        cx,
                    );
                    if self.renderer_widths_changed(is_minimap, &line_layouts, request_layout, cx) {
                        return self.prepaint(
                            None,
                            _inspector_id,
                            bounds,
                            request_layout,
                            window,
                            cx,
                        );
                    }

                    let Some(pre_scroll_layout::PreScrollLayouts {
                        scrollbar_layout_information,
                        blocks,
                        spacer_blocks,
                        row_block_types,
                        sticky_header_excerpt_id,
                        sticky_buffer_header,
                        scroll_position,
                        scroll_pixel_position,
                        scroll_max,
                        sticky_headers,
                        indent_guides,
                    }) = self.layout_pre_scroll_phase(
                        is_minimap,
                        is_singleton,
                        max_row,
                        start_row..end_row,
                        start_anchor,
                        end_anchor,
                        current_selection_head,
                        scroll_position,
                        max_scroll_top,
                        glyph_grid_cell,
                        em_advance,
                        em_layout_width,
                        line_height,
                        content_origin,
                        &text_hitbox,
                        &snapshot,
                        &hitbox,
                        editor_width,
                        &editor_margins,
                        em_width,
                        gutter_dimensions,
                        &gutter_hitbox,
                        right_margin,
                        &mut line_layouts,
                        &local_selections,
                        &selected_buffer_ids,
                        &latest_selection_anchors,
                        is_row_soft_wrapped,
                        scroll_beyond_last_line,
                        needs_horizontal_autoscroll,
                        autoscroll_request,
                        request_layout,
                        window,
                        cx,
                    )
                    else {
                        return self.prepaint(
                            None,
                            _inspector_id,
                            bounds,
                            request_layout,
                            window,
                            cx,
                        );
                    };

                    let post_scroll_prepaint::PostScrollPrepaintLayouts {
                        crease_trailers,
                        edit_prediction_popover,
                        inline_diagnostics,
                        inline_blame_layout,
                        inline_code_actions,
                        blamed_display_rows,
                        line_elements,
                        blocks,
                        spacer_blocks,
                        line_layouts,
                    } = self.layout_post_scroll_prepaint(
                        crease_trailers,
                        blocks,
                        spacer_blocks,
                        line_layouts,
                        &row_block_types,
                        &row_infos,
                        content_origin,
                        &text_hitbox,
                        right_margin,
                        scroll_position,
                        scroll_pixel_position,
                        newest_selection_head,
                        start_row,
                        end_row,
                        height_in_lines,
                        line_height,
                        em_width,
                        style,
                        &snapshot,
                        editor_width,
                        &gutter_hitbox,
                        gutter_dimensions.git_blame_entries_width,
                        &hitbox,
                        &editor_margins,
                        window,
                        cx,
                    );

                    let final_surface_layout::FinalSurfaceLayouts {
                        cursors,
                        visible_cursors,
                        navigation_overlay_paint_commands,
                        scrollbars_layout,
                        mouse_context_menu,
                        test_indicators,
                        bookmarks,
                        breakpoints,
                        diff_review_button,
                        wrap_guides,
                        minimap,
                        tab_invisible,
                        space_invisible,
                        mode,
                        diff_hunk_controls,
                        position_map,
                        crease_toggles,
                        expand_toggles,
                    } = self.layout_final_surface(
                        bounds.size,
                        start_row..end_row,
                        &snapshot,
                        &selections,
                        &row_block_types,
                        line_layouts,
                        &text_hitbox,
                        &gutter_hitbox,
                        content_origin,
                        scroll_position,
                        scroll_pixel_position,
                        scroll_max,
                        line_height,
                        em_width,
                        em_advance,
                        em_layout_width,
                        autoscroll_containing_element,
                        &redacted_ranges,
                        &scrollbar_layout_information,
                        content_offset,
                        right_margin,
                        editor_width,
                        &hitbox,
                        gutter_dimensions,
                        &gutter,
                        &row_infos,
                        &run_indicator_rows,
                        &mut breakpoint_rows,
                        newest_selection_head,
                        style,
                        current_selection_head,
                        is_read_only,
                        &sticky_headers,
                        sticky_buffer_header.is_some() || sticky_header_excerpt_id.is_some(),
                        &blocks,
                        &display_hunks,
                        &highlighted_rows,
                        vertical_scrollbar_width,
                        minimap_width,
                        font_size,
                        inline_blame_layout.as_ref(),
                        &mut crease_toggles,
                        &mut expand_toggles,
                        window,
                        cx,
                    );

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
