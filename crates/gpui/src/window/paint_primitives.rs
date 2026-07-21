use super::*;

impl Window {
    /// Creates a new painting layer for the specified bounds. A "layer" is a batch
    /// of geometry that are non-overlapping and have the same draw order. This is typically used
    /// for performance reasons.
    ///
    /// This method should only be called as part of the paint phase of element drawing.
    pub fn paint_layer<R>(&mut self, bounds: Bounds<Pixels>, f: impl FnOnce(&mut Self) -> R) -> R {
        self.invalidator.debug_assert_paint();

        let content_mask = self.content_mask();
        let clipped_bounds = bounds.intersect(&content_mask.bounds);
        if !clipped_bounds.is_empty() {
            self.next_frame
                .scene
                .push_layer(self.cover_bounds(clipped_bounds));
        }

        let result = f(self);

        if !clipped_bounds.is_empty() {
            self.next_frame.scene.pop_layer();
        }

        result
    }

    /// Paint the drop (non-inset) shadows from `shadows` into the scene at the current
    /// z-index. Inset shadows are skipped; paint those with [`Self::paint_inset_shadows`]
    /// after the element's background so they layer on top of the fill.
    ///
    /// This method should only be called as part of the paint phase of element drawing.
    pub fn paint_drop_shadows(
        &mut self,
        bounds: Bounds<Pixels>,
        corner_radii: Corners<Pixels>,
        shadows: &[BoxShadow],
    ) {
        self.invalidator.debug_assert_paint();

        let scale_factor = self.scale_factor();
        let content_mask = self.snapped_content_mask();
        let opacity = self.element_opacity();
        let element_bounds = self.cover_bounds(bounds);
        let element_corner_radii = corner_radii.scale(scale_factor);
        for shadow in shadows {
            if shadow.inset {
                continue;
            }
            let shadow_bounds = (bounds + shadow.offset).dilate(shadow.spread_radius);
            self.next_frame.scene.insert_primitive(Shadow {
                order: 0,
                blur_radius: shadow.blur_radius.scale(scale_factor),
                bounds: self.cover_bounds(shadow_bounds),
                content_mask,
                corner_radii: corner_radii.scale(scale_factor),
                color: shadow.color.opacity(opacity),
                element_bounds,
                element_corner_radii,
                inset: 0,
                pad: 0,
            });
        }
    }

    /// Paint the inset shadows from `shadows` into the scene at the current z-index. Should
    /// be called after the element's background so the shadow layers on top of the fill.
    /// Drop shadows are skipped; paint those with [`Self::paint_drop_shadows`] before the background.
    pub fn paint_inset_shadows(
        &mut self,
        bounds: Bounds<Pixels>,
        corner_radii: Corners<Pixels>,
        shadows: &[BoxShadow],
    ) {
        self.invalidator.debug_assert_paint();

        let scale_factor = self.scale_factor();
        let content_mask = self.snapped_content_mask();
        let opacity = self.element_opacity();
        let element_bounds = self.cover_bounds(bounds);
        let element_corner_radii = corner_radii.scale(scale_factor);
        for shadow in shadows {
            if !shadow.inset {
                continue;
            }
            let hole = (bounds + shadow.offset).dilate(-shadow.spread_radius);
            // Clamp at zero so a large spread can't produce negative radii, which would
            // break the SDF in the shader.
            let zero = Pixels::ZERO;
            let hole_corner_radii = Corners {
                top_left: (corner_radii.top_left - shadow.spread_radius).max(zero),
                top_right: (corner_radii.top_right - shadow.spread_radius).max(zero),
                bottom_right: (corner_radii.bottom_right - shadow.spread_radius).max(zero),
                bottom_left: (corner_radii.bottom_left - shadow.spread_radius).max(zero),
            };
            self.next_frame.scene.insert_primitive(Shadow {
                order: 0,
                blur_radius: shadow.blur_radius.scale(scale_factor),
                bounds: self.cover_bounds(hole),
                content_mask,
                corner_radii: hole_corner_radii.scale(scale_factor),
                color: shadow.color.opacity(opacity),
                element_bounds,
                element_corner_radii,
                inset: 1,
                pad: 0,
            });
        }
    }

    /// Paint one or more quads into the scene for the next frame at the current stacking context.
    /// Quads are colored rectangular regions with an optional background, border, and corner radius.
    /// see [`fill`], [`outline`], and [`quad`] to construct this type.
    ///
    /// This method should only be called as part of the paint phase of element drawing.
    ///
    /// Note that the `quad.corner_radii` are allowed to exceed the bounds, creating sharp corners
    /// where the circular arcs meet. This will not display well when combined with dashed borders.
    /// Use `Corners::clamp_radii_for_quad_size` if the radii should fit within the bounds.
    pub fn paint_quad(&mut self, quad: PaintQuad) {
        self.invalidator.debug_assert_paint();

        let opacity = self.element_opacity();
        let snapped_bounds = self.snap_bounds(quad.bounds);
        let snapped_border_widths = self.snap_border_widths(quad.border_widths);
        self.next_frame.scene.insert_primitive(Quad {
            order: 0,
            bounds: snapped_bounds,
            content_mask: self.snapped_content_mask(),
            background: quad.background.opacity(opacity),
            border_color: quad.border_color.opacity(opacity),
            corner_radii: quad.corner_radii.scale(self.scale_factor()),
            border_widths: snapped_border_widths,
            border_style: quad.border_style,
        });
    }

    /// Paint the given `Path` into the scene for the next frame at the current z-index.
    ///
    /// This method should only be called as part of the paint phase of element drawing.
    pub fn paint_path(&mut self, mut path: Path<Pixels>, color: impl Into<Background>) {
        self.invalidator.debug_assert_paint();

        let scale_factor = self.scale_factor();
        let content_mask = self.content_mask();
        let opacity = self.element_opacity();
        path.content_mask = content_mask;
        let color: Background = color.into();
        path.color = color.opacity(opacity);
        self.next_frame
            .scene
            .insert_primitive(path.scale(scale_factor));
    }

    /// Paint an underline into the scene for the next frame at the current z-index.
    ///
    /// This method should only be called as part of the paint phase of element drawing.
    pub fn paint_underline(
        &mut self,
        origin: Point<Pixels>,
        width: Pixels,
        style: &UnderlineStyle,
    ) {
        self.invalidator.debug_assert_paint();

        let scale_factor = self.scale_factor();
        let thickness = self.snap_stroke(style.thickness);
        let height = if style.wavy {
            ScaledPixels(thickness.0 * 3.)
        } else {
            thickness
        };
        let bounds = Bounds {
            origin: origin.map(|c| ScaledPixels(round_to_device_pixel(c.0, scale_factor))),
            size: size(self.snap_stroke(width), height),
        };
        let element_opacity = self.element_opacity();

        self.next_frame.scene.insert_primitive(Underline {
            order: 0,
            pad: 0,
            bounds,
            content_mask: self.snapped_content_mask(),
            color: style.color.unwrap_or_default().opacity(element_opacity),
            thickness,
            wavy: if style.wavy { 1 } else { 0 },
        });
    }

    /// Paint a strikethrough into the scene for the next frame at the current z-index.
    ///
    /// This method should only be called as part of the paint phase of element drawing.
    pub fn paint_strikethrough(
        &mut self,
        origin: Point<Pixels>,
        width: Pixels,
        style: &StrikethroughStyle,
    ) {
        self.invalidator.debug_assert_paint();

        let scale_factor = self.scale_factor();
        let height = style.thickness;
        let bounds = Bounds {
            origin: origin.map(|c| ScaledPixels(round_to_device_pixel(c.0, scale_factor))),
            size: size(self.snap_stroke(width), self.snap_stroke(height)),
        };
        let opacity = self.element_opacity();

        self.next_frame.scene.insert_primitive(Underline {
            order: 0,
            pad: 0,
            bounds,
            content_mask: self.snapped_content_mask(),
            thickness: self.snap_stroke(style.thickness),
            color: style.color.unwrap_or_default().opacity(opacity),
            wavy: 0,
        });
    }
}
