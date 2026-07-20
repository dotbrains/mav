use super::*;

impl Pane {
    pub(super) fn render_tab_bar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Pane>,
    ) -> AnyElement {
        if self.workspace.upgrade().is_none() {
            return gpui::Empty.into_any();
        }

        let focus_handle = self.focus_handle.clone();

        let navigate_backward = IconButton::new("navigate_backward", IconName::ArrowLeft)
            .icon_size(IconSize::Small)
            .on_click({
                let entity = cx.entity();
                move |_, window, cx| {
                    entity.update(cx, |pane, cx| {
                        pane.navigate_backward(&Default::default(), window, cx)
                    })
                }
            })
            .disabled(!self.can_navigate_backward())
            .tooltip({
                let focus_handle = focus_handle.clone();
                move |window, cx| {
                    Tooltip::for_action_in(
                        "Go Back",
                        &GoBack,
                        &window.focused(cx).unwrap_or_else(|| focus_handle.clone()),
                        cx,
                    )
                }
            });

        let navigate_forward = IconButton::new("navigate_forward", IconName::ArrowRight)
            .icon_size(IconSize::Small)
            .on_click({
                let entity = cx.entity();
                move |_, window, cx| {
                    entity.update(cx, |pane, cx| {
                        pane.navigate_forward(&Default::default(), window, cx)
                    })
                }
            })
            .disabled(!self.can_navigate_forward())
            .tooltip({
                let focus_handle = focus_handle.clone();
                move |window, cx| {
                    Tooltip::for_action_in(
                        "Go Forward",
                        &GoForward,
                        &window.focused(cx).unwrap_or_else(|| focus_handle.clone()),
                        cx,
                    )
                }
            });

        let mut tab_items = self
            .items
            .iter()
            .enumerate()
            .zip(tab_details(&self.items, window, cx))
            .map(|((ix, item), detail)| {
                self.render_tab(ix, &**item, detail, &focus_handle, window, cx)
                    .into_any_element()
            })
            .collect::<Vec<_>>();
        let tab_count = tab_items.len();
        if self.is_tab_pinned(tab_count) {
            log::warn!(
                "Pinned tab count ({}) exceeds actual tab count ({}). \
                This should not happen. If possible, add reproduction steps, \
                in a comment, to https://github.com/mav-industries/mav/issues/33342",
                self.pinned_tab_count,
                tab_count
            );
            self.pinned_tab_count = tab_count;
        }
        let unpinned_tabs = tab_items.split_off(self.pinned_tab_count);
        let pinned_tabs = tab_items;

        let tab_bar_settings = TabBarSettings::get_global(cx);
        let use_separate_rows = tab_bar_settings.show_pinned_tabs_in_separate_row;

        if use_separate_rows && !pinned_tabs.is_empty() && !unpinned_tabs.is_empty() {
            self.render_two_row_tab_bar(
                pinned_tabs,
                unpinned_tabs,
                tab_count,
                navigate_backward,
                navigate_forward,
                window,
                cx,
            )
        } else {
            self.render_single_row_tab_bar(
                pinned_tabs,
                unpinned_tabs,
                tab_count,
                navigate_backward,
                navigate_forward,
                window,
                cx,
            )
        }
    }

    fn configure_tab_bar_start(
        &mut self,
        tab_bar: TabBar,
        navigate_backward: IconButton,
        navigate_forward: IconButton,
        window: &mut Window,
        cx: &mut Context<Pane>,
    ) -> TabBar {
        tab_bar
            .when(
                self.display_nav_history_buttons.unwrap_or_default(),
                |tab_bar| {
                    tab_bar
                        .start_child(navigate_backward)
                        .start_child(navigate_forward)
                },
            )
            .map(|tab_bar| {
                if self.show_tab_bar_buttons {
                    let render_tab_buttons = self.render_tab_bar_buttons.clone();
                    let (left_children, right_children) = render_tab_buttons(self, window, cx);
                    tab_bar
                        .start_children(left_children)
                        .end_children(right_children)
                } else if self.zoomed {
                    tab_bar.end_child(render_toggle_zoom_button(self, cx))
                } else {
                    tab_bar
                }
            })
    }

    fn render_single_row_tab_bar(
        &mut self,
        pinned_tabs: Vec<AnyElement>,
        unpinned_tabs: Vec<AnyElement>,
        tab_count: usize,
        navigate_backward: IconButton,
        navigate_forward: IconButton,
        window: &mut Window,
        cx: &mut Context<Pane>,
    ) -> AnyElement {
        let tab_bar = self
            .configure_tab_bar_start(
                TabBar::new("tab_bar"),
                navigate_backward,
                navigate_forward,
                window,
                cx,
            )
            .children(pinned_tabs.len().ne(&0).then(|| {
                let max_scroll = self.tab_bar_scroll_handle.max_offset().x;
                // We need to check both because offset returns delta values even when the scroll handle is not scrollable
                let is_scrolled = self.tab_bar_scroll_handle.offset().x < px(0.);
                // Avoid flickering when max_offset is very small (< 2px).
                // The border adds 1-2px which can push max_offset back to 0, creating a loop.
                let is_scrollable = max_scroll > px(2.0);
                let has_active_unpinned_tab = self.active_item_index >= self.pinned_tab_count;
                h_flex()
                    .children(pinned_tabs)
                    .when(is_scrollable && is_scrolled, |this| {
                        this.when(has_active_unpinned_tab, |this| this.border_r_2())
                            .when(!has_active_unpinned_tab, |this| this.border_r_1())
                            .border_color(cx.theme().colors().border)
                    })
            }))
            .child(self.render_unpinned_tabs_container(unpinned_tabs, tab_count, cx));
        tab_bar.into_any_element()
    }

    fn render_two_row_tab_bar(
        &mut self,
        pinned_tabs: Vec<AnyElement>,
        unpinned_tabs: Vec<AnyElement>,
        tab_count: usize,
        navigate_backward: IconButton,
        navigate_forward: IconButton,
        window: &mut Window,
        cx: &mut Context<Pane>,
    ) -> AnyElement {
        let pinned_tab_bar = self
            .configure_tab_bar_start(
                TabBar::new("pinned_tab_bar"),
                navigate_backward,
                navigate_forward,
                window,
                cx,
            )
            .child(
                h_flex()
                    .id("pinned_tabs_row")
                    .debug_selector(|| "pinned_tabs_row".into())
                    .overflow_x_scroll()
                    .w_full()
                    .children(pinned_tabs)
                    .child(self.render_pinned_tab_bar_drop_target(cx)),
            );
        v_flex()
            .w_full()
            .flex_none()
            .child(pinned_tab_bar)
            .child(
                TabBar::new("unpinned_tab_bar").child(self.render_unpinned_tabs_container(
                    unpinned_tabs,
                    tab_count,
                    cx,
                )),
            )
            .into_any_element()
    }

    fn render_unpinned_tabs_container(
        &mut self,
        unpinned_tabs: Vec<AnyElement>,
        tab_count: usize,
        cx: &mut Context<Pane>,
    ) -> impl IntoElement {
        h_flex()
            .id("unpinned tabs")
            .overflow_x_scroll()
            .w_full()
            .track_scroll(&self.tab_bar_scroll_handle)
            .on_scroll_wheel(cx.listener(|this, _, _, _| {
                this.suppress_scroll = true;
            }))
            .children(unpinned_tabs)
            .child(self.render_tab_bar_drop_target(tab_count, cx))
    }

    fn render_tab_bar_drop_target(
        &self,
        tab_count: usize,
        cx: &mut Context<Pane>,
    ) -> impl IntoElement {
        div()
            .id("tab_bar_drop_target")
            .relative()
            .min_w_6()
            .h(Tab::container_height(cx))
            .flex_grow_1()
            // HACK: This empty child is currently necessary to force the drop target to appear
            // despite us setting a min width above.
            .child("")
            .when(
                (self.drag_tab_target
                    || self.drag_tab_insertion_target == Some(TabInsertionTarget::UnpinnedEnd))
                    && cx.has_active_drag(),
                |bar| bar.child(Self::render_tab_bar_insertion_indicator(cx)),
            )
            .on_drag_move::<DraggedTab>(cx.listener(
                move |this, event: &DragMoveEvent<DraggedTab>, _, cx| {
                    this.handle_dragged_tab_over_tab_bar_end(
                        TabInsertionTarget::UnpinnedEnd,
                        event,
                        cx,
                    );
                },
            ))
            .on_drag_move::<DraggedSelection>(cx.listener(
                move |this, event: &DragMoveEvent<DraggedSelection>, _, cx| {
                    this.handle_dragged_selection_over_tab_bar_end(
                        TabInsertionTarget::UnpinnedEnd,
                        event,
                        cx,
                    );
                },
            ))
            .on_drop(
                cx.listener(move |this, dragged_tab: &DraggedTab, window, cx| {
                    this.clear_drag_drop_target(cx);
                    this.handle_tab_drop(dragged_tab, this.items.len(), false, window, cx)
                }),
            )
            .on_drop(
                cx.listener(move |this, selection: &DraggedSelection, window, cx| {
                    this.clear_drag_drop_target(cx);
                    this.handle_dragged_selection_drop(selection, Some(tab_count), window, cx)
                }),
            )
            .on_drop(cx.listener(move |this, paths, window, cx| {
                this.clear_drag_drop_target(cx);
                this.handle_external_paths_drop(paths, window, cx)
            }))
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                if event.click_count() == 2 {
                    window.dispatch_action(this.double_click_dispatch_action.boxed_clone(), cx);
                }
            }))
    }

    fn render_tab_bar_insertion_indicator(cx: &mut Context<Pane>) -> impl IntoElement {
        div()
            .absolute()
            .top(px(4.))
            .bottom(px(4.))
            .left_0()
            .w(px(2.))
            .bg(cx.theme().colors().drop_target_border)
    }

    fn render_pinned_tab_bar_drop_target(&self, cx: &mut Context<Pane>) -> impl IntoElement {
        div()
            .id("pinned_tabs_border")
            .debug_selector(|| "pinned_tabs_border".into())
            .relative()
            .min_w_6()
            .h(Tab::container_height(cx))
            .flex_grow_1()
            .border_l_1()
            .border_color(cx.theme().colors().border)
            // HACK: This empty child is currently necessary to force the drop target to appear
            // despite us setting a min width above.
            .child("")
            .when(
                self.drag_tab_insertion_target == Some(TabInsertionTarget::PinnedEnd)
                    && cx.has_active_drag(),
                |bar| bar.child(Self::render_tab_bar_insertion_indicator(cx)),
            )
            .on_drag_move::<DraggedTab>(cx.listener(
                move |this, event: &DragMoveEvent<DraggedTab>, _, cx| {
                    this.handle_dragged_tab_over_tab_bar_end(
                        TabInsertionTarget::PinnedEnd,
                        event,
                        cx,
                    );
                },
            ))
            .on_drag_move::<DraggedSelection>(cx.listener(
                move |this, event: &DragMoveEvent<DraggedSelection>, _, cx| {
                    this.handle_dragged_selection_over_tab_bar_end(
                        TabInsertionTarget::PinnedEnd,
                        event,
                        cx,
                    );
                },
            ))
            .on_drop(
                cx.listener(move |this, dragged_tab: &DraggedTab, window, cx| {
                    this.clear_drag_drop_target(cx);
                    this.handle_pinned_tab_bar_drop(dragged_tab, window, cx)
                }),
            )
            .on_drop(
                cx.listener(move |this, selection: &DraggedSelection, window, cx| {
                    this.clear_drag_drop_target(cx);
                    this.handle_dragged_selection_drop(
                        selection,
                        Some(this.pinned_tab_count),
                        window,
                        cx,
                    )
                }),
            )
            .on_drop(cx.listener(move |this, paths, window, cx| {
                this.clear_drag_drop_target(cx);
                this.handle_external_paths_drop(paths, window, cx)
            }))
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                if event.click_count() == 2 {
                    window.dispatch_action(this.double_click_dispatch_action.boxed_clone(), cx);
                }
            }))
    }

    pub(super) fn render_split_drop_overlay(
        direction: SplitDirection,
        accepts_dragged_selection: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let size = DefiniteLength::Fraction(0.5);
        div()
            .absolute()
            .bg(cx.theme().colors().drop_target_background)
            .border_2()
            .border_color(cx.theme().colors().drop_target_border)
            .rounded_lg()
            .map(|div| match direction {
                SplitDirection::Up => div.top_0().left_0().right_0().h(size),
                SplitDirection::Down => div.left_0().bottom_0().right_0().h(size),
                SplitDirection::Left => div.top_0().left_0().bottom_0().w(size),
                SplitDirection::Right => div.top_0().bottom_0().right_0().w(size),
            })
            .on_drop(
                cx.listener(move |this, dragged_tab: &DraggedTab, window, cx| {
                    this.handle_tab_drop(dragged_tab, this.active_item_index(), true, window, cx)
                }),
            )
            .when(accepts_dragged_selection, |div| {
                div.on_drop(
                    cx.listener(move |this, selection: &DraggedSelection, window, cx| {
                        this.handle_dragged_selection_drop(selection, None, window, cx)
                    }),
                )
            })
            .on_drop(
                cx.listener(move |this, dragged_pane: &DraggedPane, window, cx| {
                    this.handle_pane_drop(dragged_pane, window, cx)
                }),
            )
            .on_drop(cx.listener(move |this, paths, window, cx| {
                this.handle_external_paths_drop(paths, window, cx)
            }))
    }

    pub(super) fn render_swap_drop_overlay(cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .absolute()
            .top_0()
            .right_0()
            .bottom_0()
            .left_0()
            .bg(cx.theme().colors().drop_target_background)
            .border_2()
            .border_color(cx.theme().colors().drop_target_border)
            .rounded_lg()
            .on_drop(
                cx.listener(move |this, dragged_pane: &DraggedPane, window, cx| {
                    this.handle_pane_drop(dragged_pane, window, cx)
                }),
            )
    }
}
