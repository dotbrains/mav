use super::*;

impl DebugPanel {
    pub(crate) fn deploy_context_menu(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(running_state) = self
            .active_session
            .as_ref()
            .map(|session| session.read(cx).running_state().clone())
        {
            let pane_items_status = running_state.read(cx).pane_items_status(cx);
            let this = cx.weak_entity();

            let context_menu = ContextMenu::build(window, cx, |mut menu, _window, _cx| {
                for (item_kind, is_visible) in pane_items_status.into_iter() {
                    menu = menu.toggleable_entry(item_kind, is_visible, IconPosition::End, None, {
                        let this = this.clone();
                        move |window, cx| {
                            this.update(cx, |this, cx| {
                                if let Some(running_state) = this
                                    .active_session
                                    .as_ref()
                                    .map(|session| session.read(cx).running_state().clone())
                                {
                                    running_state.update(cx, |state, cx| {
                                        if is_visible {
                                            state.remove_pane_item(item_kind, window, cx);
                                        } else {
                                            state.add_pane_item(item_kind, position, window, cx);
                                        }
                                    })
                                }
                            })
                            .ok();
                        }
                    });
                }

                menu
            });

            window.focus(&context_menu.focus_handle(cx), cx);
            let subscription = cx.subscribe(&context_menu, |this, _, _: &DismissEvent, cx| {
                this.context_menu.take();
                cx.notify();
            });
            self.context_menu = Some((context_menu, position, subscription));
        }
    }

    pub(super) fn copy_debug_adapter_arguments(
        &mut self,
        _: &CopyDebugAdapterArguments,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let content = maybe!({
            let mut session = self.active_session()?.read(cx).session(cx);
            while let Some(parent) = session.read(cx).parent_session().cloned() {
                session = parent;
            }
            let binary = session.read(cx).binary()?;
            let content = serde_json::to_string_pretty(&binary).ok()?;
            Some(content)
        });
        if let Some(content) = content {
            cx.write_to_clipboard(ClipboardItem::new_string(content));
        }
    }
}
