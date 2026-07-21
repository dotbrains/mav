use super::*;

impl Sidebar {
    pub(super) fn dispatch_context(&self, window: &Window, cx: &Context<Self>) -> KeyContext {
        let mut dispatch_context = KeyContext::new_with_defaults();
        dispatch_context.add("Sidebar");
        dispatch_context.add("menu");

        let is_renaming_thread = self
            .thread_rename_editor
            .focus_handle(cx)
            .is_focused(window);

        let identifier = if is_renaming_thread {
            "editing"
        } else {
            "not_searching"
        };

        dispatch_context.add(identifier);
        dispatch_context
    }

    pub(super) fn focus_in(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus_handle.is_focused(window) {
            return;
        }

        cx.notify();
    }

    pub(super) fn cancel(&mut self, _: &Cancel, window: &mut Window, cx: &mut Context<Self>) {
        if self.renaming_thread_id.is_some() {
            self.finish_thread_rename(window, cx);
            return;
        }

        if self.filter_editor.read(cx).is_focused(window) {
            if self.reset_filter_editor_text(window, cx) {
                self.selection = None;
                self.update_entries(cx);
                return;
            }

            if self.selection.is_none() {
                self.select_first_entry();
            }
            if self.selection.is_some() {
                self.focus_handle.focus(window, cx);
                cx.notify();
            }
            return;
        }

        if self.reset_filter_editor_text(window, cx) {
            self.update_entries(cx);
        } else {
            self.selection = None;
            self.focus_handle.focus(window, cx);
            cx.notify();
        }
    }

    pub(super) fn focus_sidebar_filter(
        &mut self,
        _: &FocusSidebarFilter,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selection = None;
        if let SidebarView::Archive(archive) = &self.view {
            archive.update(cx, |view, _cx| {
                view.clear_selection();
            });
        }
        self.focus_handle.focus(window, cx);

        cx.notify();
    }

    pub(super) fn reset_filter_editor_text(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.filter_editor.update(cx, |editor, cx| {
            if editor.buffer().read(cx).len(cx).0 > 0 {
                editor.set_text("", window, cx);
                true
            } else {
                false
            }
        })
    }

    pub(super) fn has_filter_query(&self, _cx: &App) -> bool {
        false
    }
}
