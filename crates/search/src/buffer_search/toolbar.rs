use super::*;

impl Focusable for BufferSearchBar {
    fn focus_handle(&self, cx: &App) -> gpui::FocusHandle {
        self.query_editor.focus_handle(cx)
    }
}

impl ToolbarItemView for BufferSearchBar {
    fn contribute_context(&self, context: &mut KeyContext, _cx: &App) {
        if !self.dismissed {
            context.add("buffer_search_deployed");
        }
    }

    fn set_active_pane_item(
        &mut self,
        item: Option<&dyn ItemHandle>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ToolbarItemLocation {
        cx.notify();
        self.active_searchable_item_subscriptions.take();
        self.active_searchable_item.take();
        self.splittable_editor = None;
        self._splittable_editor_subscription = None;

        self.pending_search.take();

        if let Some(splittable_editor) = item
            .and_then(|item| item.act_as_type(TypeId::of::<SplittableEditor>(), cx))
            .and_then(|entity| entity.downcast::<SplittableEditor>().ok())
        {
            self._splittable_editor_subscription =
                Some(cx.observe(&splittable_editor, |_, _, cx| cx.notify()));
            self.splittable_editor = Some(splittable_editor.downgrade());
        }

        if let Some(searchable_item_handle) =
            item.and_then(|item| item.to_searchable_item_handle(cx))
        {
            let this = cx.entity().downgrade();

            let search_event_subscription = searchable_item_handle.subscribe_to_search_events(
                window,
                cx,
                Box::new(move |search_event, window, cx| {
                    if let Some(this) = this.upgrade() {
                        this.update(cx, |this, cx| {
                            this.on_active_searchable_item_event(search_event, window, cx)
                        });
                    }
                }),
            );

            #[cfg(target_os = "macos")]
            {
                let item_focus_handle = searchable_item_handle.item_focus_handle(cx);

                self.active_searchable_item_subscriptions = Some([
                    search_event_subscription,
                    cx.on_focus(&item_focus_handle, window, |this, window, cx| {
                        if this.query_editor_focused || this.replacement_editor_focused {
                            // no need to read pasteboard since focus came from toolbar
                            return;
                        }

                        cx.defer_in(window, |this, window, cx| {
                            let Some(item) = cx.read_from_find_pasteboard() else {
                                return;
                            };
                            let Some(text) = item.text() else {
                                return;
                            };

                            if this.query(cx) == text {
                                return;
                            }

                            let search_options = item
                                .metadata()
                                .and_then(|m| m.parse().ok())
                                .and_then(SearchOptions::from_bits)
                                .unwrap_or(this.search_options);

                            if this.dismissed {
                                this.pending_external_query = Some((text, search_options));
                            } else {
                                drop(this.search(&text, Some(search_options), true, window, cx));
                            }
                        });
                    }),
                ]);
            }
            #[cfg(not(target_os = "macos"))]
            {
                self.active_searchable_item_subscriptions = Some(search_event_subscription);
            }

            let is_project_search = searchable_item_handle.supported_options(cx).find_in_results;
            self.active_searchable_item = Some(searchable_item_handle);
            drop(self.update_matches(true, false, window, cx));
            if self.needs_expand_collapse_option(cx) {
                return ToolbarItemLocation::PrimaryLeft;
            } else if !self.is_dismissed() {
                if is_project_search {
                    self.dismiss(&Default::default(), window, cx);
                } else {
                    return ToolbarItemLocation::Secondary;
                }
            }
        }
        ToolbarItemLocation::Hidden
    }
}
