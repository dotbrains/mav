use super::*;

pub(super) enum NavigationOverlayPaintCommand {
    Label(NavigationLabelLayout),
}

pub(super) struct NavigationLabelLayout {
    pub(super) element: AnyElement,
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) origin: gpui::Point<Pixels>,
}

pub(super) struct NavigationOverlayLayoutContext<'a> {
    pub(super) display_snapshot: &'a DisplaySnapshot,
    pub(super) visible_display_row_range: &'a Range<DisplayRow>,
    pub(super) line_layouts: &'a [LineWithInvisibles],
    pub(super) text_align: TextAlign,
    pub(super) content_width: Pixels,
    pub(super) content_origin: gpui::Point<Pixels>,
    pub(super) scroll_position: gpui::Point<ScrollOffset>,
    pub(super) scroll_pixel_position: gpui::Point<ScrollPixelOffset>,
    pub(super) line_height: Pixels,
    pub(super) editor_font: Font,
    pub(super) editor_font_size: Pixels,
}
