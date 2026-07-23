use super::*;

pub(crate) struct InitialFocusState {
    pub(crate) focus_handle: FocusHandle,
    pub(crate) blink_manager: Entity<BlinkManager>,
}

impl Editor {
    pub(crate) fn initial_focus_state(
        is_minimap: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> InitialFocusState {
        let blink_manager = cx.new(|cx| {
            let mut blink_manager = BlinkManager::new(
                CURSOR_BLINK_INTERVAL,
                |cx| EditorSettings::get_global(cx).cursor_blink,
                cx,
            );
            if is_minimap {
                blink_manager.disable(cx);
            }
            blink_manager
        });

        let focus_handle = cx.focus_handle();
        if !is_minimap {
            cx.on_focus(&focus_handle, window, Self::handle_focus)
                .detach();
            cx.on_focus_in(&focus_handle, window, Self::handle_focus_in)
                .detach();
            cx.on_focus_out(&focus_handle, window, Self::handle_focus_out)
                .detach();
            cx.on_blur(&focus_handle, window, Self::handle_blur)
                .detach();
            cx.observe_pending_input(window, Self::observe_pending_input)
                .detach();
        }

        InitialFocusState {
            focus_handle,
            blink_manager,
        }
    }
}
