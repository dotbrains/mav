use super::*;

impl Pane {
    pub fn nav_history_for_item<T: Item>(&self, item: &Entity<T>) -> ItemNavHistory {
        self.nav_history.for_item(Arc::new(item.downgrade()))
    }

    pub fn nav_history(&self) -> &NavHistory {
        &self.nav_history
    }

    pub fn nav_history_mut(&mut self) -> &mut NavHistory {
        &mut self.nav_history
    }

    pub fn fork_nav_history(&self) -> NavHistory {
        self.nav_history.fork()
    }

    pub fn set_nav_history(&mut self, history: NavHistory, cx: &Context<Self>) {
        self.nav_history = history;
        self.nav_history.set_pane(cx.entity().downgrade());
    }

    pub fn disable_history(&mut self) {
        self.nav_history.disable();
    }

    pub fn enable_history(&mut self) {
        self.nav_history.enable();
    }

    pub fn can_navigate_backward(&self) -> bool {
        !self.nav_history.0.lock().backward_stack.is_empty()
    }

    pub fn can_navigate_forward(&self) -> bool {
        !self.nav_history.0.lock().forward_stack.is_empty()
    }

    pub fn navigate_backward(&mut self, _: &GoBack, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(workspace) = self.workspace.upgrade() {
            let pane = cx.entity().downgrade();
            window.defer(cx, move |window, cx| {
                workspace.update(cx, |workspace, cx| {
                    workspace.go_back(pane, window, cx).detach_and_log_err(cx)
                })
            })
        }
    }

    pub(super) fn navigate_forward(
        &mut self,
        _: &GoForward,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(workspace) = self.workspace.upgrade() {
            let pane = cx.entity().downgrade();
            window.defer(cx, move |window, cx| {
                workspace.update(cx, |workspace, cx| {
                    workspace
                        .go_forward(pane, window, cx)
                        .detach_and_log_err(cx)
                })
            })
        }
    }

    pub fn go_to_older_tag(
        &mut self,
        _: &GoToOlderTag,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(workspace) = self.workspace.upgrade() {
            let pane = cx.entity().downgrade();
            window.defer(cx, move |window, cx| {
                workspace.update(cx, |workspace, cx| {
                    workspace
                        .navigate_tag_history(pane, TagNavigationMode::Older, window, cx)
                        .detach_and_log_err(cx)
                })
            })
        }
    }

    pub fn go_to_newer_tag(
        &mut self,
        _: &GoToNewerTag,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(workspace) = self.workspace.upgrade() {
            let pane = cx.entity().downgrade();
            window.defer(cx, move |window, cx| {
                workspace.update(cx, |workspace, cx| {
                    workspace
                        .navigate_tag_history(pane, TagNavigationMode::Newer, window, cx)
                        .detach_and_log_err(cx)
                })
            })
        }
    }

    pub(super) fn history_updated(&mut self, cx: &mut Context<Self>) {
        self.toolbar.update(cx, |_, cx| cx.notify());
    }

    pub fn set_pinned_count(&mut self, count: usize) {
        self.pinned_tab_count = count;
    }

    pub fn pinned_count(&self) -> usize {
        self.pinned_tab_count
    }
}
