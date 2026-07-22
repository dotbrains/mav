use super::*;

impl X11Window {
    pub(super) fn window_decorations_impl(&self) -> gpui::Decorations {
        let state = self.0.state.borrow();

        if !state.client_side_decorations_supported {
            return Decorations::Server;
        }

        match state.decorations {
            WindowDecorations::Server => Decorations::Server,
            WindowDecorations::Client => {
                let tiling = if state.fullscreen {
                    Tiling::tiled()
                } else if let Some(edge_constraints) = &state.edge_constraints {
                    edge_constraints.to_tiling()
                } else {
                    Tiling {
                        top: state.maximized_vertical,
                        bottom: state.maximized_vertical,
                        left: state.maximized_horizontal,
                        right: state.maximized_horizontal,
                    }
                };
                Decorations::Client { tiling }
            }
        }
    }

    pub(super) fn set_client_inset_impl(&self, inset: Pixels) {
        let mut state = self.0.state.borrow_mut();

        let dp = (f32::from(inset) * state.scale_factor) as u32;

        let insets = if state.fullscreen {
            [0, 0, 0, 0]
        } else if let Some(edge_constraints) = &state.edge_constraints {
            let left = if edge_constraints.left_tiled { 0 } else { dp };
            let top = if edge_constraints.top_tiled { 0 } else { dp };
            let right = if edge_constraints.right_tiled { 0 } else { dp };
            let bottom = if edge_constraints.bottom_tiled { 0 } else { dp };

            [left, right, top, bottom]
        } else {
            let (left, right) = if state.maximized_horizontal {
                (0, 0)
            } else {
                (dp, dp)
            };
            let (top, bottom) = if state.maximized_vertical {
                (0, 0)
            } else {
                (dp, dp)
            };
            [left, right, top, bottom]
        };

        if state.last_insets != insets {
            state.last_insets = insets;

            check_reply(
                || "X11 ChangeProperty for _GTK_FRAME_EXTENTS failed.",
                self.0.xcb.change_property(
                    xproto::PropMode::REPLACE,
                    self.0.x_window,
                    state.atoms._GTK_FRAME_EXTENTS,
                    xproto::AtomEnum::CARDINAL,
                    size_of::<u32>() as u8 * 8,
                    4,
                    bytemuck::cast_slice::<u32, u8>(&insets),
                ),
            )
            .log_err();
        }
    }

    pub(super) fn request_decorations_impl(&self, mut decorations: gpui::WindowDecorations) {
        let mut state = self.0.state.borrow_mut();

        if matches!(decorations, gpui::WindowDecorations::Client)
            && !state.client_side_decorations_supported
        {
            log::info!(
                "x11: no compositor present, falling back to server-side window decorations"
            );
            decorations = gpui::WindowDecorations::Server;
        }

        let hints_data: [u32; 5] = match decorations {
            WindowDecorations::Server => [1 << 1, 0, 1, 0, 0],
            WindowDecorations::Client => [1 << 1, 0, 0, 0, 0],
        };

        let success = check_reply(
            || "X11 ChangeProperty for _MOTIF_WM_HINTS failed.",
            self.0.xcb.change_property(
                xproto::PropMode::REPLACE,
                self.0.x_window,
                state.atoms._MOTIF_WM_HINTS,
                state.atoms._MOTIF_WM_HINTS,
                size_of::<u32>() as u8 * 8,
                5,
                bytemuck::cast_slice::<u32, u8>(&hints_data),
            ),
        )
        .log_err();

        let Some(()) = success else {
            return;
        };

        match decorations {
            WindowDecorations::Server => {
                state.decorations = WindowDecorations::Server;
                let is_transparent = state.is_transparent();
                state.renderer.update_transparency(is_transparent);
            }
            WindowDecorations::Client => {
                state.decorations = WindowDecorations::Client;
                let is_transparent = state.is_transparent();
                state.renderer.update_transparency(is_transparent);
            }
        }

        drop(state);
        let mut callbacks = self.0.callbacks.borrow_mut();
        if let Some(appearance_changed) = callbacks.appearance_changed.as_mut() {
            appearance_changed();
        }
    }
}
