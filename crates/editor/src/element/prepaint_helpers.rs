use super::*;

impl EditorElement {
    pub(super) fn prepaint_crease_toggles(
        &self,
        crease_toggles: &mut [Option<AnyElement>],
        line_height: Pixels,
        gutter_dimensions: &GutterDimensions,
        gutter_settings: crate::editor_settings::Gutter,
        scroll_position: gpui::Point<ScrollOffset>,
        start_row: DisplayRow,
        gutter_hitbox: &Hitbox,
        window: &mut Window,
        cx: &mut App,
    ) {
        for (ix, crease_toggle) in crease_toggles.iter_mut().enumerate() {
            if let Some(crease_toggle) = crease_toggle {
                debug_assert!(gutter_settings.folds);
                let available_space = size(
                    AvailableSpace::MinContent,
                    AvailableSpace::Definite(line_height * 0.55),
                );
                let crease_toggle_size = crease_toggle.layout_as_root(available_space, window, cx);

                let display_row = DisplayRow(start_row.0 + ix as u32);
                let position = point(
                    gutter_dimensions.width - gutter_dimensions.right_padding,
                    line_height * (display_row.as_f64() - scroll_position.y) as f32,
                );
                let centering_offset = point(
                    (gutter_dimensions.fold_area_width() - crease_toggle_size.width) / 2.,
                    (line_height - crease_toggle_size.height) / 2.,
                );
                let origin = gutter_hitbox.origin + position + centering_offset;
                crease_toggle.prepaint_as_root(origin, available_space, window, cx);
            }
        }
    }

    pub(super) fn prepaint_expand_toggles(
        &self,
        expand_toggles: &mut [Option<(AnyElement, gpui::Point<Pixels>)>],
        window: &mut Window,
        cx: &mut App,
    ) {
        for (expand_toggle, origin) in expand_toggles.iter_mut().flatten() {
            let available_space = size(AvailableSpace::MinContent, AvailableSpace::MinContent);
            expand_toggle.layout_as_root(available_space, window, cx);
            expand_toggle.prepaint_as_root(*origin, available_space, window, cx);
        }
    }

    pub(super) fn prepaint_crease_trailers(
        &self,
        trailers: Vec<Option<AnyElement>>,
        lines: &[LineWithInvisibles],
        line_height: Pixels,
        content_origin: gpui::Point<Pixels>,
        scroll_pixel_position: gpui::Point<ScrollPixelOffset>,
        scroll_position: gpui::Point<ScrollOffset>,
        start_row: DisplayRow,
        em_width: Pixels,
        window: &mut Window,
        cx: &mut App,
    ) -> Vec<Option<CreaseTrailerLayout>> {
        trailers
            .into_iter()
            .enumerate()
            .map(|(ix, element)| {
                let mut element = element?;
                let available_space = size(
                    AvailableSpace::MinContent,
                    AvailableSpace::Definite(line_height),
                );
                let size = element.layout_as_root(available_space, window, cx);

                let line = &lines[ix];
                let padding = if line.width == Pixels::ZERO {
                    Pixels::ZERO
                } else {
                    4. * em_width
                };
                let position = point(
                    Pixels::from(scroll_pixel_position.x) + line.width + padding,
                    line_height
                        * (DisplayRow(start_row.0 + ix as u32).as_f64() - scroll_position.y) as f32,
                );
                let centering_offset = point(px(0.), (line_height - size.height) / 2.);
                let origin = content_origin + position + centering_offset;
                element.prepaint_as_root(origin, available_space, window, cx);
                Some(CreaseTrailerLayout {
                    element,
                    bounds: Bounds::new(origin, size),
                })
            })
            .collect()
    }

    // Folds contained in a hunk are ignored apart from shrinking visual size
    // If a fold contains any hunks then that fold line is marked as modified
}
