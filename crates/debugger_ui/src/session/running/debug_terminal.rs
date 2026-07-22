use super::*;

pub struct DebugTerminal {
    pub terminal: Option<Entity<TerminalView>>,
    focus_handle: FocusHandle,
    _subscriptions: [Subscription; 1],
}

impl DebugTerminal {
    fn empty(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let focus_subscription = cx.on_focus(&focus_handle, window, |this, window, cx| {
            if let Some(terminal) = this.terminal.as_ref() {
                terminal.focus_handle(cx).focus(window, cx);
            }
        });

        Self {
            terminal: None,
            focus_handle,
            _subscriptions: [focus_subscription],
        }
    }
}

impl gpui::Render for DebugTerminal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(cx.theme().colors().editor_background)
            .children(self.terminal.clone())
    }
}
impl Focusable for DebugTerminal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
