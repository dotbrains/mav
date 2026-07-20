use super::*;

impl Editor {
    pub fn context_menu_first(
        &mut self,
        _: &ContextMenuFirst,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(context_menu) = self.context_menu.borrow_mut().as_mut() {
            context_menu.select_first(self.completion_provider.as_deref(), window, cx);
        }
    }

    pub fn context_menu_prev(
        &mut self,
        _: &ContextMenuPrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(context_menu) = self.context_menu.borrow_mut().as_mut() {
            context_menu.select_prev(self.completion_provider.as_deref(), window, cx);
        }
    }

    pub fn context_menu_next(
        &mut self,
        _: &ContextMenuNext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(context_menu) = self.context_menu.borrow_mut().as_mut() {
            context_menu.select_next(self.completion_provider.as_deref(), window, cx);
        }
    }

    pub fn context_menu_last(
        &mut self,
        _: &ContextMenuLast,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(context_menu) = self.context_menu.borrow_mut().as_mut() {
            context_menu.select_last(self.completion_provider.as_deref(), window, cx);
        }
    }

    pub fn signature_help_prev(
        &mut self,
        _: &SignatureHelpPrevious,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(popover) = self.signature_help_state.popover_mut() {
            if popover.current_signature == 0 {
                popover.current_signature = popover.signatures.len() - 1;
            } else {
                popover.current_signature -= 1;
            }
            cx.notify();
        }
    }

    pub fn signature_help_next(
        &mut self,
        _: &SignatureHelpNext,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(popover) = self.signature_help_state.popover_mut() {
            if popover.current_signature + 1 == popover.signatures.len() {
                popover.current_signature = 0;
            } else {
                popover.current_signature += 1;
            }
            cx.notify();
        }
    }
}
