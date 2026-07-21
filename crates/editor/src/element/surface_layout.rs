use super::*;

impl EditorElement {
    pub(super) fn layout_surface(
        bounds: Bounds<Pixels>,
        text_width: Pixels,
        editor_margins: &EditorMargins,
        window: &mut Window,
    ) -> layout_data::EditorSurface {
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        let gutter_hitbox = window.insert_hitbox(
            gutter_bounds(bounds, editor_margins.gutter),
            HitboxBehavior::Normal,
        );
        let text_hitbox = window.insert_hitbox(
            Bounds {
                origin: gutter_hitbox.top_right(),
                size: size(text_width, bounds.size.height),
            },
            HitboxBehavior::Normal,
        );

        // Offset the content_bounds from the text_bounds by the gutter margin (which
        // is roughly half a character wide) to make hit testing work more like how we want.
        let content_offset = point(editor_margins.gutter.margin, Pixels::ZERO);
        let content_origin = text_hitbox.origin + content_offset;

        layout_data::EditorSurface {
            hitbox,
            gutter_hitbox,
            text_hitbox,
            content_offset,
            content_origin,
        }
    }
}
