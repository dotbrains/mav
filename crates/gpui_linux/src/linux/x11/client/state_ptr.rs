use super::*;

impl X11ClientStatePtr {
    pub fn get_client(&self) -> Option<X11Client> {
        self.0.upgrade().map(X11Client)
    }

    pub fn drop_window(&self, x_window: u32) {
        let Some(client) = self.get_client() else {
            return;
        };
        let mut state = client.0.borrow_mut();

        if let Some(window_ref) = state.windows.remove(&x_window)
            && let Some(RefreshState::PeriodicRefresh {
                event_loop_token, ..
            }) = window_ref.refresh_state
        {
            state.loop_handle.remove(event_loop_token);
        }
        if state.mouse_focused_window == Some(x_window) {
            state.mouse_focused_window = None;
        }
        if state.keyboard_focused_window == Some(x_window) {
            state.keyboard_focused_window = None;
        }
        if state.cursor_hidden_window == Some(x_window) {
            state.cursor_hidden_window = None;
        }
        state.cursor_styles.remove(&x_window);
    }

    pub fn update_ime_position(&self, bounds: Bounds<Pixels>) {
        let Some(client) = self.get_client() else {
            return;
        };
        let mut state = client.0.borrow_mut();
        if state.composing || state.ximc.is_none() {
            return;
        }

        let Some(mut ximc) = state.ximc.take() else {
            log::error!("bug: xim connection not set");
            return;
        };
        let Some(xim_handler) = state.xim_handler.take() else {
            log::error!("bug: xim handler not set");
            state.ximc = Some(ximc);
            return;
        };
        let scaled_bounds = bounds.scale(state.scale_factor);
        let ic_attributes = ximc
            .build_ic_attributes()
            .push(
                xim::AttributeName::InputStyle,
                xim::InputStyle::PREEDIT_CALLBACKS,
            )
            .push(xim::AttributeName::ClientWindow, xim_handler.window)
            .push(xim::AttributeName::FocusWindow, xim_handler.window)
            .nested_list(xim::AttributeName::PreeditAttributes, |b| {
                b.push(
                    xim::AttributeName::SpotLocation,
                    xim::Point {
                        x: u32::from(scaled_bounds.origin.x + scaled_bounds.size.width) as i16,
                        y: u32::from(scaled_bounds.origin.y + scaled_bounds.size.height) as i16,
                    },
                );
            })
            .build();
        let _ = ximc
            .set_ic_values(xim_handler.im_id, xim_handler.ic_id, ic_attributes)
            .log_err();
        state.ximc = Some(ximc);
        state.xim_handler = Some(xim_handler);
    }
}

#[derive(Clone)]
pub(crate) struct X11Client(pub(crate) Rc<RefCell<X11ClientState>>);
