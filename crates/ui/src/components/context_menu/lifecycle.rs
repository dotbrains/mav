use super::*;

impl ContextMenu {
    pub fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        f: impl FnOnce(Self, &mut Window, &mut Context<Self>) -> Self,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let _on_blur_subscription = cx.on_blur(
            &focus_handle,
            window,
            |this: &mut ContextMenu, window, cx| {
                if let Some(ignore_until) = this.ignore_blur_until {
                    if Instant::now() < ignore_until {
                        return;
                    } else {
                        this.ignore_blur_until = None;
                    }
                }

                if this.main_menu.is_none() {
                    if let SubmenuState::Open(open_submenu) = &this.submenu_state {
                        let submenu_focus = open_submenu.entity.read(cx).focus_handle.clone();
                        if submenu_focus.contains_focused(window, cx) {
                            return;
                        }
                    }
                }

                this.cancel(&menu::Cancel, window, cx)
            },
        );
        window.refresh();

        f(
            Self {
                builder: None,
                items: Default::default(),
                focus_handle,
                action_context: None,
                selected_index: None,
                delayed: false,
                clicked: false,
                end_slot_action: None,
                key_context: "menu".into(),
                _on_blur_subscription,
                keep_open_on_confirm: false,
                fixed_width: None,
                main_menu: None,
                main_menu_observed_bounds: Rc::new(Cell::new(None)),
                documentation_aside: None,
                aside_trigger_bounds: Rc::new(RefCell::new(HashMap::default())),
                submenu_state: SubmenuState::Closed,
                hover_target: HoverTarget::MainMenu,
                submenu_safety_threshold_x: None,
                submenu_trigger_bounds: Rc::new(Cell::new(None)),
                submenu_trigger_mouse_down: false,
                ignore_blur_until: None,
            },
            window,
            cx,
        )
    }

    pub fn build(
        window: &mut Window,
        cx: &mut App,
        f: impl FnOnce(Self, &mut Window, &mut Context<Self>) -> Self,
    ) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx, f))
    }

    /// Builds a [`ContextMenu`] that will stay open when making changes instead of closing after each confirmation.
    ///
    /// The main difference from [`ContextMenu::build`] is the type of the `builder`, as we need to be able to hold onto
    /// it to call it again.
    pub fn build_persistent(
        window: &mut Window,
        cx: &mut App,
        builder: impl Fn(Self, &mut Window, &mut Context<Self>) -> Self + 'static,
    ) -> Entity<Self> {
        cx.new(|cx| {
            let builder = Rc::new(builder);

            let focus_handle = cx.focus_handle();
            let _on_blur_subscription = cx.on_blur(
                &focus_handle,
                window,
                |this: &mut ContextMenu, window, cx| {
                    if let Some(ignore_until) = this.ignore_blur_until {
                        if Instant::now() < ignore_until {
                            return;
                        } else {
                            this.ignore_blur_until = None;
                        }
                    }

                    if this.main_menu.is_none() {
                        if let SubmenuState::Open(open_submenu) = &this.submenu_state {
                            let submenu_focus = open_submenu.entity.read(cx).focus_handle.clone();
                            if submenu_focus.contains_focused(window, cx) {
                                return;
                            }
                        }
                    }

                    this.cancel(&menu::Cancel, window, cx)
                },
            );
            window.refresh();

            (builder.clone())(
                Self {
                    builder: Some(builder),
                    items: Default::default(),
                    focus_handle,
                    action_context: None,
                    selected_index: None,
                    delayed: false,
                    clicked: false,
                    end_slot_action: None,
                    key_context: "menu".into(),
                    _on_blur_subscription,
                    keep_open_on_confirm: true,
                    fixed_width: None,
                    main_menu: None,
                    main_menu_observed_bounds: Rc::new(Cell::new(None)),
                    documentation_aside: None,
                    aside_trigger_bounds: Rc::new(RefCell::new(HashMap::default())),
                    submenu_state: SubmenuState::Closed,
                    hover_target: HoverTarget::MainMenu,
                    submenu_safety_threshold_x: None,
                    submenu_trigger_bounds: Rc::new(Cell::new(None)),
                    submenu_trigger_mouse_down: false,
                    ignore_blur_until: None,
                },
                window,
                cx,
            )
        })
    }

    /// Rebuilds the menu.
    ///
    /// This is used to refresh the menu entries when entries are toggled when the menu is configured with
    /// `keep_open_on_confirm = true`.
    ///
    /// This only works if the [`ContextMenu`] was constructed using [`ContextMenu::build_persistent`]. Otherwise it is
    /// a no-op.
    pub fn rebuild(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(builder) = self.builder.clone() else {
            return;
        };

        // The way we rebuild the menu is a bit of a hack.
        let focus_handle = cx.focus_handle();
        let new_menu = (builder.clone())(
            Self {
                builder: Some(builder),
                items: Default::default(),
                focus_handle: focus_handle.clone(),
                action_context: None,
                selected_index: None,
                delayed: false,
                clicked: false,
                end_slot_action: None,
                key_context: "menu".into(),
                _on_blur_subscription: cx.on_blur(
                    &focus_handle,
                    window,
                    |this: &mut ContextMenu, window, cx| {
                        if let Some(ignore_until) = this.ignore_blur_until {
                            if Instant::now() < ignore_until {
                                return;
                            } else {
                                this.ignore_blur_until = None;
                            }
                        }

                        if this.main_menu.is_none() {
                            if let SubmenuState::Open(open_submenu) = &this.submenu_state {
                                let submenu_focus =
                                    open_submenu.entity.read(cx).focus_handle.clone();
                                if submenu_focus.contains_focused(window, cx) {
                                    return;
                                }
                            }
                        }

                        this.cancel(&menu::Cancel, window, cx)
                    },
                ),
                keep_open_on_confirm: false,
                fixed_width: None,
                main_menu: None,
                main_menu_observed_bounds: Rc::new(Cell::new(None)),
                documentation_aside: None,
                aside_trigger_bounds: Rc::new(RefCell::new(HashMap::default())),
                submenu_state: SubmenuState::Closed,
                hover_target: HoverTarget::MainMenu,
                submenu_safety_threshold_x: None,
                submenu_trigger_bounds: Rc::new(Cell::new(None)),
                submenu_trigger_mouse_down: false,
                ignore_blur_until: None,
            },
            window,
            cx,
        );

        self.items = new_menu.items;

        cx.notify();
    }
}
