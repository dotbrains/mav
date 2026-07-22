use super::*;

impl ContextMenu {
    pub fn confirm(&mut self, _: &menu::Confirm, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.selected_index else {
            return;
        };

        if let Some(ContextMenuItem::Submenu { builder, .. }) = self.items.get(ix) {
            self.open_submenu(
                ix,
                builder.clone(),
                SubmenuOpenTrigger::Keyboard,
                window,
                cx,
            );

            if let SubmenuState::Open(open_submenu) = &self.submenu_state {
                let focus_handle = open_submenu.entity.read(cx).focus_handle.clone();
                window.focus(&focus_handle, cx);
                open_submenu.entity.update(cx, |submenu, cx| {
                    submenu.select_first(&SelectFirst, window, cx);
                });
            }

            cx.notify();
            return;
        }

        let context = self.action_context.as_ref();

        if let Some(
            ContextMenuItem::Entry(ContextMenuEntry {
                handler,
                disabled: false,
                ..
            })
            | ContextMenuItem::CustomEntry { handler, .. },
        ) = self.items.get(ix)
        {
            (handler)(context, window, cx)
        }

        if self.main_menu.is_some() && !self.keep_open_on_confirm {
            self.clicked = true;
        }

        if self.keep_open_on_confirm {
            self.rebuild(window, cx);
        } else {
            cx.emit(DismissEvent);
        }
    }

    pub fn secondary_confirm(
        &mut self,
        _: &menu::SecondaryConfirm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ix) = self.selected_index else {
            return;
        };

        if let Some(ContextMenuItem::Submenu { builder, .. }) = self.items.get(ix) {
            self.open_submenu(
                ix,
                builder.clone(),
                SubmenuOpenTrigger::Keyboard,
                window,
                cx,
            );

            if let SubmenuState::Open(open_submenu) = &self.submenu_state {
                let focus_handle = open_submenu.entity.read(cx).focus_handle.clone();
                window.focus(&focus_handle, cx);
                open_submenu.entity.update(cx, |submenu, cx| {
                    submenu.select_first(&SelectFirst, window, cx);
                });
            }

            cx.notify();
            return;
        }

        let context = self.action_context.as_ref();

        if let Some(ContextMenuItem::Entry(ContextMenuEntry {
            handler,
            secondary_handler,
            disabled: false,
            ..
        })) = self.items.get(ix)
        {
            if let Some(secondary) = secondary_handler {
                (secondary)(context, window, cx)
            } else {
                (handler)(context, window, cx)
            }
        } else if let Some(ContextMenuItem::CustomEntry { handler, .. }) = self.items.get(ix) {
            (handler)(context, window, cx)
        }

        if self.main_menu.is_some() && !self.keep_open_on_confirm {
            self.clicked = true;
        }

        if self.keep_open_on_confirm {
            self.rebuild(window, cx);
        } else {
            cx.emit(DismissEvent);
        }
    }

    pub fn cancel(&mut self, _: &menu::Cancel, window: &mut Window, cx: &mut Context<Self>) {
        if self.main_menu.is_some() {
            cx.emit(DismissEvent);

            // Restore keyboard focus to the parent menu so arrow keys / Escape / Enter work again.
            if let Some(parent) = &self.main_menu {
                let parent_focus = parent.read(cx).focus_handle.clone();

                parent.update(cx, |parent, _cx| {
                    parent.ignore_blur_until = Some(Instant::now() + Duration::from_millis(200));
                });

                window.focus(&parent_focus, cx);
            }

            return;
        }

        cx.emit(DismissEvent);
    }

    pub fn end_slot(&mut self, _: &dyn Action, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.selected_index.and_then(|ix| self.items.get(ix)) else {
            return;
        };
        let ContextMenuItem::Entry(entry) = item else {
            return;
        };
        let Some(handler) = entry.end_slot_handler.as_ref() else {
            return;
        };
        handler(None, window, cx);
        self.rebuild(window, cx);
        cx.notify();
    }

    pub fn clear_selected(&mut self) {
        self.selected_index = None;
    }

    pub fn select_first(&mut self, _: &SelectFirst, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(ix) = self.items.iter().position(|item| item.is_selectable()) {
            self.select_index(ix, window, cx);
        }
        cx.notify();
    }

    pub fn select_last(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<usize> {
        for (ix, item) in self.items.iter().enumerate().rev() {
            if item.is_selectable() {
                return self.select_index(ix, window, cx);
            }
        }
        None
    }

    pub(crate) fn handle_select_last(
        &mut self,
        _: &SelectLast,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.select_last(window, cx).is_some() {
            cx.notify();
        }
    }

    pub fn select_next(&mut self, _: &SelectNext, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(ix) = self.selected_index {
            let next_index = ix + 1;
            if self.items.len() <= next_index {
                self.select_first(&SelectFirst, window, cx);
                return;
            } else {
                for (ix, item) in self.items.iter().enumerate().skip(next_index) {
                    if item.is_selectable() {
                        self.select_index(ix, window, cx);
                        cx.notify();
                        return;
                    }
                }
            }
        }
        self.select_first(&SelectFirst, window, cx);
    }

    pub fn select_previous(
        &mut self,
        _: &SelectPrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(ix) = self.selected_index {
            for (ix, item) in self.items.iter().enumerate().take(ix).rev() {
                if item.is_selectable() {
                    self.select_index(ix, window, cx);
                    cx.notify();
                    return;
                }
            }
        }
        self.handle_select_last(&SelectLast, window, cx);
    }

    pub fn select_submenu_child(
        &mut self,
        _: &SelectChild,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ix) = self.selected_index else {
            return;
        };

        let Some(ContextMenuItem::Submenu { builder, .. }) = self.items.get(ix) else {
            return;
        };

        self.open_submenu(
            ix,
            builder.clone(),
            SubmenuOpenTrigger::Keyboard,
            window,
            cx,
        );

        if let SubmenuState::Open(open_submenu) = &self.submenu_state {
            let focus_handle = open_submenu.entity.read(cx).focus_handle.clone();
            window.focus(&focus_handle, cx);
            open_submenu.entity.update(cx, |submenu, cx| {
                submenu.select_first(&SelectFirst, window, cx);
            });
        }

        cx.notify();
    }

    pub fn select_submenu_parent(
        &mut self,
        _: &SelectParent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.main_menu.is_none() {
            return;
        }

        if let Some(parent) = &self.main_menu {
            let parent_clone = parent.clone();

            let parent_focus = parent.read(cx).focus_handle.clone();
            window.focus(&parent_focus, cx);

            cx.emit(DismissEvent);

            parent_clone.update(cx, |parent, cx| {
                if let SubmenuState::Open(open_submenu) = &parent.submenu_state {
                    let trigger_index = open_submenu.item_index;
                    parent.close_submenu(false, cx);
                    let _ = parent.select_index(trigger_index, window, cx);
                    cx.notify();
                }
            });

            return;
        }

        cx.emit(DismissEvent);
    }

    fn select_index(
        &mut self,
        ix: usize,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        self.documentation_aside = None;
        let item = self.items.get(ix)?;
        if item.is_selectable() {
            self.selected_index = Some(ix);
            match item {
                ContextMenuItem::Entry(entry) => {
                    if let Some(callback) = &entry.documentation_aside {
                        self.documentation_aside = Some((ix, callback.clone()));
                    }
                }
                ContextMenuItem::CustomEntry {
                    documentation_aside: Some(callback),
                    ..
                } => {
                    self.documentation_aside = Some((ix, callback.clone()));
                }
                ContextMenuItem::Submenu { .. } => {}
                _ => (),
            }
        }
        Some(ix)
    }

    fn create_submenu(
        builder: Rc<dyn Fn(ContextMenu, &mut Window, &mut Context<ContextMenu>) -> ContextMenu>,
        parent_entity: Entity<ContextMenu>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Entity<ContextMenu>, Subscription) {
        let submenu = Self::build_submenu(builder, parent_entity, window, cx);

        let dismiss_subscription = cx.subscribe(&submenu, |this, submenu, _: &DismissEvent, cx| {
            let should_dismiss_parent = submenu.read(cx).clicked;

            this.close_submenu(false, cx);

            if should_dismiss_parent {
                cx.emit(DismissEvent);
            }
        });

        (submenu, dismiss_subscription)
    }

    fn build_submenu(
        builder: Rc<dyn Fn(ContextMenu, &mut Window, &mut Context<ContextMenu>) -> ContextMenu>,
        parent_entity: Entity<ContextMenu>,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<ContextMenu> {
        cx.new(|cx| {
            let focus_handle = cx.focus_handle();

            let _on_blur_subscription = cx.on_blur(
                &focus_handle,
                window,
                |_this: &mut ContextMenu, _window, _cx| {},
            );

            let mut menu = ContextMenu {
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
                documentation_aside: None,
                aside_trigger_bounds: Rc::new(RefCell::new(HashMap::default())),
                main_menu: Some(parent_entity),
                main_menu_observed_bounds: Rc::new(Cell::new(None)),
                submenu_state: SubmenuState::Closed,
                hover_target: HoverTarget::MainMenu,
                submenu_safety_threshold_x: None,
                submenu_trigger_bounds: Rc::new(Cell::new(None)),
                submenu_trigger_mouse_down: false,
                ignore_blur_until: None,
            };

            menu = (builder)(menu, window, cx);
            menu
        })
    }

    pub(crate) fn close_submenu(&mut self, clear_selection: bool, cx: &mut Context<Self>) {
        self.submenu_state = SubmenuState::Closed;
        self.hover_target = HoverTarget::MainMenu;
        self.submenu_safety_threshold_x = None;
        self.main_menu_observed_bounds.set(None);
        self.submenu_trigger_bounds.set(None);

        if clear_selection {
            self.selected_index = None;
        }

        cx.notify();
    }

    pub(crate) fn open_submenu(
        &mut self,
        item_index: usize,
        builder: Rc<dyn Fn(ContextMenu, &mut Window, &mut Context<ContextMenu>) -> ContextMenu>,
        reason: SubmenuOpenTrigger,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // If the submenu is already open for this item, don't recreate it.
        if matches!(
            &self.submenu_state,
            SubmenuState::Open(open_submenu) if open_submenu.item_index == item_index
        ) {
            return;
        }

        let (submenu, dismiss_subscription) =
            Self::create_submenu(builder, cx.entity(), window, cx);

        let flip_left = self
            .main_menu_observed_bounds
            .get()
            .is_some_and(|bounds| bounds.right() + px(200.0) > window.viewport_size().width);

        // If we're switching from one submenu item to another, throw away any previously-captured
        // offset so we don't reuse a stale position.
        self.main_menu_observed_bounds.set(None);
        self.submenu_trigger_bounds.set(None);

        self.submenu_safety_threshold_x = None;
        self.hover_target = HoverTarget::MainMenu;

        // When opening a submenu via keyboard, there is a brief moment where focus/hover can
        // transition in a way that triggers the parent menu's `on_blur` dismissal.
        if matches!(reason, SubmenuOpenTrigger::Keyboard) {
            self.ignore_blur_until = Some(Instant::now() + Duration::from_millis(150));
        }

        let trigger_bounds = self.submenu_trigger_bounds.get();

        self.submenu_state = SubmenuState::Open(OpenSubmenu {
            item_index,
            entity: submenu,
            trigger_bounds,
            offset: None,
            flip_left,
            _dismiss_subscription: dismiss_subscription,
        });

        cx.notify();
    }

    pub fn on_action_dispatch(
        &mut self,
        dispatched: &dyn Action,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.clicked {
            cx.propagate();
            return;
        }

        if let Some(ix) = self.items.iter().position(|item| {
            if let ContextMenuItem::Entry(ContextMenuEntry {
                action: Some(action),
                disabled: false,
                ..
            }) = item
            {
                action.partial_eq(dispatched)
            } else {
                false
            }
        }) {
            self.select_index(ix, window, cx);
            self.delayed = true;
            cx.notify();
            let action = dispatched.boxed_clone();
            cx.spawn_in(window, async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(50))
                    .await;
                cx.update(|window, cx| {
                    this.update(cx, |this, cx| {
                        this.cancel(&menu::Cancel, window, cx);
                        window.dispatch_action(action, cx);
                    })
                })
            })
            .detach_and_log_err(cx);
        } else {
            cx.propagate()
        }
    }

    pub fn on_blur_subscription(mut self, new_subscription: Subscription) -> Self {
        self._on_blur_subscription = new_subscription;
        self
    }
}
