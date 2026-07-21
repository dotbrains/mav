use super::*;

impl EditorElement {
    pub(super) fn paint_document_colors(&self, layout: &mut EditorLayout, window: &mut Window) {
        let Some((colors_render_mode, image_colors)) = &layout.document_colors else {
            return;
        };
        if image_colors.is_empty()
            || colors_render_mode == &DocumentColorsRenderMode::None
            || colors_render_mode == &DocumentColorsRenderMode::Inlay
        {
            return;
        }

        let line_end_overshoot = layout.line_end_overshoot();

        for (range, color) in image_colors {
            match colors_render_mode {
                DocumentColorsRenderMode::Inlay | DocumentColorsRenderMode::None => return,
                DocumentColorsRenderMode::Background => {
                    self.paint_highlighted_range(
                        range.clone(),
                        true,
                        *color,
                        Pixels::ZERO,
                        line_end_overshoot,
                        layout,
                        window,
                    );
                }
                DocumentColorsRenderMode::Border => {
                    self.paint_highlighted_range(
                        range.clone(),
                        false,
                        *color,
                        Pixels::ZERO,
                        line_end_overshoot,
                        layout,
                        window,
                    );
                }
            }
        }
    }
}
