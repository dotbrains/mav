use super::*;

const LABEL_LINE_HEIGHT_PADDING_PX: f32 = 2.0;

pub struct CursorLayout {
    pub(super) origin: gpui::Point<Pixels>,
    pub(super) block_width: Pixels,
    pub(super) line_height: Pixels,
    pub(super) color: Hsla,
    pub(super) shape: CursorShape,
    pub(super) block_text: Option<ShapedLine>,
    pub(super) cursor_name: Option<AnyElement>,
}

#[derive(Debug)]
pub struct CursorName {
    pub(super) string: SharedString,
    pub(super) color: Hsla,
    pub(super) is_top_row: bool,
}

impl CursorLayout {
    pub fn new(
        origin: gpui::Point<Pixels>,
        block_width: Pixels,
        line_height: Pixels,
        color: Hsla,
        shape: CursorShape,
        block_text: Option<ShapedLine>,
    ) -> CursorLayout {
        CursorLayout {
            origin,
            block_width,
            line_height,
            color,
            shape,
            block_text,
            cursor_name: None,
        }
    }

    pub fn bounding_rect(&self, origin: gpui::Point<Pixels>) -> Bounds<Pixels> {
        Bounds {
            origin: self.origin + origin,
            size: size(self.block_width, self.line_height),
        }
    }

    fn bounds(&self, origin: gpui::Point<Pixels>) -> Bounds<Pixels> {
        match self.shape {
            CursorShape::Bar => Bounds {
                origin: self.origin + origin,
                size: size(px(2.0), self.line_height),
            },
            CursorShape::Block | CursorShape::Hollow => Bounds {
                origin: self.origin + origin,
                size: size(self.block_width, self.line_height),
            },
            CursorShape::Underline => Bounds {
                origin: self.origin
                    + origin
                    + gpui::Point::new(Pixels::ZERO, self.line_height - px(2.0)),
                size: size(self.block_width, px(2.0)),
            },
        }
    }

    pub fn layout(
        &mut self,
        origin: gpui::Point<Pixels>,
        cursor_name: Option<CursorName>,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(cursor_name) = cursor_name {
            let bounds = self.bounds(origin);
            let text_size = self.line_height / 1.5;

            let name_origin = if cursor_name.is_top_row {
                point(bounds.right() - px(1.), bounds.top())
            } else {
                match self.shape {
                    CursorShape::Bar => point(
                        bounds.right() - px(2.),
                        bounds.top() - text_size / 2. - px(1.),
                    ),
                    _ => point(
                        bounds.right() - px(1.),
                        bounds.top() - text_size / 2. - px(1.),
                    ),
                }
            };
            let mut name_element = div()
                .bg(self.color)
                .text_size(text_size)
                .px_0p5()
                .line_height(text_size + px(LABEL_LINE_HEIGHT_PADDING_PX))
                .text_color(cursor_name.color)
                .child(cursor_name.string)
                .into_any_element();

            name_element.prepaint_as_root(name_origin, AvailableSpace::min_size(), window, cx);

            self.cursor_name = Some(name_element);
        }
    }

    pub fn paint(&mut self, origin: gpui::Point<Pixels>, window: &mut Window, cx: &mut App) {
        let bounds = window.pixel_snap_bounds(self.bounds(origin));

        let cursor = if matches!(self.shape, CursorShape::Hollow) {
            outline(bounds, self.color, BorderStyle::Solid)
        } else {
            fill(bounds, self.color)
        };

        if let Some(name) = &mut self.cursor_name {
            name.paint(window, cx);
        }

        window.paint_quad(cursor);

        if let Some(block_text) = &self.block_text {
            block_text
                .paint(
                    self.origin + origin,
                    self.line_height,
                    TextAlign::Left,
                    None,
                    window,
                    cx,
                )
                .log_err();
        }
    }

    pub fn shape(&self) -> CursorShape {
        self.shape
    }
}
