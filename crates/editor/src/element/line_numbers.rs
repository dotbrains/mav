use super::*;

#[derive(Debug)]
pub struct LineNumberSegment {
    pub(super) shaped_line: ShapedLine,
    pub(super) hitbox: Option<Hitbox>,
}

#[derive(Debug)]
pub struct LineNumberLayout {
    pub(super) segments: SmallVec<[LineNumberSegment; 1]>,
}

impl EditorElement {
    pub(super) fn paint_line_numbers(
        &mut self,
        layout: &mut EditorLayout,
        window: &mut Window,
        cx: &mut App,
    ) {
        let is_singleton = self.editor.read(cx).buffer_kind(cx) == ItemBufferKind::Singleton;

        let line_height = layout.position_map.line_height;
        window.set_cursor_style(CursorStyle::Arrow, &layout.gutter_hitbox);

        for line_layout in layout.line_numbers.values() {
            for LineNumberSegment {
                shaped_line,
                hitbox,
            } in &line_layout.segments
            {
                let Some(hitbox) = hitbox else {
                    continue;
                };

                let Some(()) = (if !is_singleton && hitbox.is_hovered(window) {
                    let color = cx.theme().colors().editor_hover_line_number;

                    let line = self.shape_line_number(shaped_line.text.clone(), color, window);
                    line.paint(
                        hitbox.origin,
                        line_height,
                        TextAlign::Left,
                        None,
                        window,
                        cx,
                    )
                    .log_err()
                } else {
                    shaped_line
                        .paint(
                            hitbox.origin,
                            line_height,
                            TextAlign::Left,
                            None,
                            window,
                            cx,
                        )
                        .log_err()
                }) else {
                    continue;
                };

                // In singleton buffers, we select corresponding lines on the line number click, so use | -like cursor.
                // In multi buffers, we open file at the line number clicked, so use a pointing hand cursor.
                if is_singleton {
                    window.set_cursor_style(CursorStyle::IBeam, hitbox);
                } else {
                    window.set_cursor_style(CursorStyle::PointingHand, hitbox);
                }
            }
        }
    }
}

impl EditorElement {
    pub(super) fn shape_line_number(
        &self,
        text: SharedString,
        color: Hsla,
        window: &mut Window,
    ) -> ShapedLine {
        let run = TextRun {
            len: text.len(),
            font: self.style.text.font(),
            color,
            ..Default::default()
        };
        window.text_system().shape_line(
            text,
            self.style.text.font_size.to_pixels(window.rem_size()),
            &[run],
            None,
        )
    }
}
