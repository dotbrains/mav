use super::*;

impl EditorElement {
    pub(super) fn layout_metrics(
        &self,
        bounds: Bounds<Pixels>,
        snapshot: &EditorSnapshot,
        style: &EditorStyle,
        rem_size: Pixels,
        window: &mut Window,
        cx: &mut App,
    ) -> layout_data::EditorMetrics {
        let font_id = window.text_system().resolve_font(&style.text.font());
        let font_size = style.text.font_size.to_pixels(rem_size);
        let line_height = style.text.line_height_in_pixels(rem_size);
        let em_width = window.text_system().em_width(font_id, font_size).unwrap();
        let em_advance = window.text_system().em_advance(font_id, font_size).unwrap();
        let em_layout_width = window.text_system().em_layout_width(font_id, font_size);
        let glyph_grid_cell = size(em_advance, line_height);

        let gutter_dimensions = snapshot.gutter_dimensions(font_id, font_size, style, window, cx);
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

        layout_data::EditorMetrics {
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
        }
    }
}
