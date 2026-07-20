use super::*;

impl Pane {
    pub fn active_item_index(&self) -> usize {
        self.active_item_index
    }

    pub fn is_active_item_pinned(&self) -> bool {
        self.is_tab_pinned(self.active_item_index)
    }

    pub fn activation_history(&self) -> &[ActivationHistoryEntry] {
        &self.activation_history
    }

    pub fn set_should_display_tab_bar<F>(&mut self, should_display_tab_bar: F)
    where
        F: 'static + Fn(&Window, &mut Context<Pane>) -> bool,
    {
        self.should_display_tab_bar = Rc::new(should_display_tab_bar);
    }

    pub fn set_should_display_welcome_page(&mut self, should_display_welcome_page: bool) {
        self.should_display_welcome_page = should_display_welcome_page;
    }

    pub fn set_pane_kind(&mut self, pane_kind: PaneKind, cx: &mut Context<Self>) {
        self.pane_kind = pane_kind;
        cx.notify();
    }

    pub fn pane_kind(&self) -> PaneKind {
        self.pane_kind
    }

    pub fn is_tabbed(&self) -> bool {
        self.pane_kind.is_tabbed()
    }

    pub fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        if self.visible != visible {
            self.visible = visible;
            cx.notify();
        }
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn preferred_horizontal_split_size(&self) -> Option<Pixels> {
        self.preferred_horizontal_split_size
    }

    pub fn remember_horizontal_split_size(&mut self, size: Pixels) {
        if size > Pixels::ZERO {
            self.preferred_horizontal_split_size = Some(size);
        }
    }

    pub fn set_reserve_traffic_light_space(
        &mut self,
        reserve_traffic_light_space: bool,
        cx: &mut Context<Self>,
    ) {
        if self.reserve_traffic_light_space != reserve_traffic_light_space {
            self.reserve_traffic_light_space = reserve_traffic_light_space;
            cx.notify();
        }
    }

    pub fn should_reserve_traffic_light_space(&self, window: &Window, cx: &App) -> bool {
        if !cfg!(target_os = "macos")
            || window.is_fullscreen()
            || !(self.reserve_traffic_light_space || self.zoomed)
            || Self::titlebar_visible(cx)
        {
            return false;
        }

        !self.left_sidebar_visible(cx)
    }

    fn titlebar_visible(_cx: &App) -> bool {
        false
    }

    fn left_sidebar_visible(&self, cx: &App) -> bool {
        self.workspace
            .upgrade()
            .and_then(|workspace| workspace.read(cx).multi_workspace().cloned())
            .and_then(|multi_workspace| multi_workspace.upgrade())
            .is_some_and(|multi_workspace| {
                let sidebar = multi_workspace.read(cx).sidebar_render_state(cx);
                sidebar.open && sidebar.side == SidebarSide::Left
            })
    }

    pub fn set_can_split(
        &mut self,
        can_split_predicate: Option<
            Arc<dyn Fn(&mut Self, &dyn Any, &mut Window, &mut Context<Self>) -> bool + 'static>,
        >,
    ) {
        self.can_split_predicate = can_split_predicate;
    }

    pub fn set_can_toggle_zoom(&mut self, can_toggle_zoom: bool, cx: &mut Context<Self>) {
        self.can_toggle_zoom = can_toggle_zoom;
        cx.notify();
    }

    pub fn set_close_pane_if_empty(&mut self, close_pane_if_empty: bool, cx: &mut Context<Self>) {
        self.close_pane_if_empty = close_pane_if_empty;
        cx.notify();
    }

    pub fn set_can_navigate(&mut self, can_navigate: bool, cx: &mut Context<Self>) {
        self.toolbar.update(cx, |toolbar, cx| {
            toolbar.set_can_navigate(can_navigate, cx);
        });
        cx.notify();
    }

    pub fn set_render_tab_bar<F>(&mut self, cx: &mut Context<Self>, render: F)
    where
        F: 'static + Fn(&mut Pane, &mut Window, &mut Context<Pane>) -> AnyElement,
    {
        self.render_tab_bar = Rc::new(render);
        cx.notify();
    }

    pub fn set_render_tab_bar_buttons<F>(&mut self, cx: &mut Context<Self>, render: F)
    where
        F: 'static
            + Fn(
                &mut Pane,
                &mut Window,
                &mut Context<Pane>,
            ) -> (Option<AnyElement>, Option<AnyElement>),
    {
        self.render_tab_bar_buttons = Rc::new(render);
        cx.notify();
    }
}
