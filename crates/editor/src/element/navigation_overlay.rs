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

impl EditorElement {
    pub(super) fn layout_navigation_overlays(
        &self,
        snapshot: &EditorSnapshot,
        visible_display_row_range: Range<DisplayRow>,
        line_layouts: &[LineWithInvisibles],
        text_hitbox: &Hitbox,
        content_origin: gpui::Point<Pixels>,
        scroll_position: gpui::Point<ScrollOffset>,
        scroll_pixel_position: gpui::Point<ScrollPixelOffset>,
        line_height: Pixels,
        window: &mut Window,
        cx: &mut App,
    ) -> Vec<NavigationOverlayPaintCommand> {
        let mut overlay_sets = self
            .editor
            .read(cx)
            .navigation_overlay_sets()
            .iter()
            .map(|(key, overlays)| (*key, overlays.clone()))
            .collect::<Vec<_>>();
        if overlay_sets.is_empty() {
            return Vec::new();
        }
        overlay_sets.sort_by_key(|(key, _)| *key);

        let layout_context = NavigationOverlayLayoutContext {
            display_snapshot: &snapshot.display_snapshot,
            visible_display_row_range: &visible_display_row_range,
            line_layouts,
            text_align: self.style.text.text_align,
            content_width: text_hitbox.size.width,
            content_origin,
            scroll_position,
            scroll_pixel_position,
            line_height,
            editor_font: self.style.text.font(),
            editor_font_size: self.style.text.font_size.to_pixels(window.rem_size()),
        };
        let mut navigation_overlay_paint_commands = Vec::new();

        for (_, overlays) in overlay_sets {
            for overlay in overlays.as_ref() {
                Self::layout_navigation_label(
                    overlay,
                    &layout_context,
                    window,
                    cx,
                    &mut navigation_overlay_paint_commands,
                );
            }
        }

        navigation_overlay_paint_commands
    }

    pub(super) fn layout_navigation_label(
        overlay: &crate::NavigationTargetOverlay,
        context: &NavigationOverlayLayoutContext<'_>,
        window: &mut Window,
        cx: &mut App,
        paint_commands: &mut Vec<NavigationOverlayPaintCommand>,
    ) {
        let label = &overlay.label;
        let label_display_point = overlay
            .target_range
            .start
            .to_display_point(context.display_snapshot);
        let label_row = label_display_point.row();
        if !context.visible_display_row_range.contains(&label_row) {
            return;
        }

        let row_index = label_row.minus(context.visible_display_row_range.start) as usize;
        let row_layout = &context.line_layouts[row_index];
        let label_column = label_display_point.column().min(row_layout.len as u32) as usize;
        let label_x = row_layout.x_for_index(label_column)
            + row_layout.alignment_offset(context.text_align, context.content_width)
            - context.scroll_pixel_position.x.into()
            + label.x_offset;
        let label_y = ((label_row.as_f64() - context.scroll_position.y)
            * ScrollPixelOffset::from(context.line_height))
        .into();
        let label_text_size = (context.editor_font_size * label.scale_factor.max(0.0)).max(px(1.0));
        let origin = context.content_origin + point(label_x, label_y);

        let mut element = div()
            .block_mouse_except_scroll()
            .font(context.editor_font.clone())
            .text_size(label_text_size)
            .text_color(label.text_color)
            .line_height(context.line_height)
            .child(label.text.clone())
            .into_any_element();
        element.prepaint_as_root(origin, AvailableSpace::min_size(), window, cx);

        paint_commands.push(NavigationOverlayPaintCommand::Label(
            NavigationLabelLayout { element, origin },
        ));
    }
}

impl EditorElement {
    pub(super) fn paint_navigation_overlays(
        &mut self,
        layout: &mut EditorLayout,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_element_namespace("navigation_overlays", |window| {
            for command in &mut layout.navigation_overlay_paint_commands {
                let NavigationOverlayPaintCommand::Label(label) = command;
                label.element.paint(window, cx);
            }
        });
    }
}
