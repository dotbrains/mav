use super::*;

pub(crate) fn new_debugger_pane(
    workspace: WeakEntity<Workspace>,
    project: Entity<Project>,
    window: &mut Window,
    cx: &mut Context<RunningState>,
) -> Entity<Pane> {
    let weak_running = cx.weak_entity();

    cx.new(move |cx| {
        let can_drop_predicate: Arc<dyn Fn(&dyn Any, &mut Window, &mut App) -> bool> =
            Arc::new(|any, _window, _cx| {
                any.downcast_ref::<DraggedTab>()
                    .is_some_and(|dragged_tab| dragged_tab.item.downcast::<SubView>().is_some())
            });
        let mut pane = Pane::new(
            workspace.clone(),
            project.clone(),
            Default::default(),
            Some(can_drop_predicate),
            NoAction.boxed_clone(),
            true,
            window,
            cx,
        );
        let focus_handle = pane.focus_handle(cx);
        pane.set_can_split(Some(Arc::new({
            let weak_running = weak_running.clone();
            move |pane, dragged_item, _window, cx| {
                if let Some(tab) = dragged_item.downcast_ref::<DraggedTab>() {
                    let is_current_pane = tab.pane == cx.entity();
                    let Some(can_drag_away) = weak_running
                        .read_with(cx, |running_state, _| {
                            let current_panes = running_state.panes.panes();
                            !current_panes.contains(&&tab.pane)
                                || current_panes.len() > 1
                                || (!is_current_pane || pane.items_len() > 1)
                        })
                        .ok()
                    else {
                        return false;
                    };
                    if can_drag_away {
                        let item = if is_current_pane {
                            pane.item_for_index(tab.ix)
                        } else {
                            tab.pane.read(cx).item_for_index(tab.ix)
                        };
                        if let Some(item) = item {
                            return item.downcast::<SubView>().is_some();
                        }
                    }
                }
                false
            }
        })));
        pane.set_can_toggle_zoom(false, cx);
        pane.display_nav_history_buttons(None);
        pane.set_should_display_tab_bar(|_, _| true);
        pane.set_render_tab_bar_buttons(cx, |_, _, _| (None, None));
        pane.set_render_tab_bar(cx, {
            move |pane, window, cx| {
                let active_pane_item = pane.active_item();
                let pane_group_id: SharedString =
                    format!("pane-zoom-button-hover-{}", cx.entity_id()).into();
                let as_subview = active_pane_item
                    .as_ref()
                    .and_then(|item| item.downcast::<SubView>());
                let is_hovered = as_subview
                    .as_ref()
                    .is_some_and(|item| item.read(cx).hovered);

                h_flex()
                    .track_focus(&focus_handle)
                    .group(pane_group_id.clone())
                    .pl_1p5()
                    .pr_1()
                    .justify_between()
                    .border_b_1()
                    .border_color(cx.theme().colors().border)
                    .bg(cx.theme().colors().tab_bar_background)
                    .on_action(|_: &menu::Cancel, window, cx| {
                        if cx.stop_active_drag(window) {
                        } else {
                            cx.propagate();
                        }
                    })
                    .child(
                        h_flex()
                            .w_full()
                            .gap_1()
                            .h(Tab::container_height(cx))
                            .drag_over::<DraggedTab>(|bar, _, _, cx| {
                                bar.bg(cx.theme().colors().drop_target_background)
                            })
                            .on_drop(cx.listener(
                                move |this, dragged_tab: &DraggedTab, window, cx| {
                                    if dragged_tab.item.downcast::<SubView>().is_none() {
                                        return;
                                    }
                                    this.drag_split_direction = None;
                                    this.handle_tab_drop(
                                        dragged_tab,
                                        this.items_len(),
                                        false,
                                        window,
                                        cx,
                                    )
                                },
                            ))
                            .children(pane.items().enumerate().map(|(ix, item)| {
                                let selected = active_pane_item
                                    .as_ref()
                                    .is_some_and(|active| active.item_id() == item.item_id());
                                let deemphasized = !pane.has_focus(window, cx);
                                let item_ = item.boxed_clone();
                                div()
                                    .id(format!("debugger_tab_{}", item.item_id().as_u64()))
                                    .p_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .when_some(item.tab_tooltip_text(cx), |this, tooltip| {
                                        this.tooltip(Tooltip::text(tooltip))
                                    })
                                    .map(|this| {
                                        let theme = cx.theme();
                                        if selected {
                                            let color = theme.colors().tab_active_background;
                                            let color = if deemphasized {
                                                color.opacity(0.5)
                                            } else {
                                                color
                                            };
                                            this.bg(color)
                                        } else {
                                            let hover_color = theme.colors().element_hover;
                                            this.hover(|style| style.bg(hover_color))
                                        }
                                    })
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        let index = this.index_for_item(&*item_);
                                        if let Some(index) = index {
                                            this.activate_item(index, true, true, window, cx);
                                        }
                                    }))
                                    .child(item.tab_content(
                                        TabContentParams {
                                            selected,
                                            deemphasized,
                                            ..Default::default()
                                        },
                                        window,
                                        cx,
                                    ))
                                    .on_drop(cx.listener(
                                        move |this, dragged_tab: &DraggedTab, window, cx| {
                                            if dragged_tab.item.downcast::<SubView>().is_none() {
                                                return;
                                            }
                                            this.drag_split_direction = None;
                                            this.handle_tab_drop(dragged_tab, ix, false, window, cx)
                                        },
                                    ))
                                    .on_drag(
                                        DraggedTab {
                                            item: item.boxed_clone(),
                                            pane: cx.entity(),
                                            detail: 0,
                                            is_active: selected,
                                            ix,
                                        },
                                        |tab, _, _, cx| cx.new(|_| tab.clone()),
                                    )
                            })),
                    )
                    .child({
                        let zoomed = pane.is_zoomed();

                        h_flex()
                            .visible_on_hover(pane_group_id)
                            .when(is_hovered, |this| this.visible())
                            .when_some(as_subview.as_ref(), |this, subview| {
                                subview.update(cx, |view, cx| {
                                    let Some(additional_actions) = view.actions.as_mut() else {
                                        return this;
                                    };
                                    this.child(additional_actions(window, cx))
                                })
                            })
                            .child(
                                IconButton::new(
                                    SharedString::from(format!(
                                        "debug-toggle-zoom-{}",
                                        cx.entity_id()
                                    )),
                                    if zoomed {
                                        IconName::Minimize
                                    } else {
                                        IconName::Maximize
                                    },
                                )
                                .icon_size(IconSize::Small)
                                .on_click(cx.listener(move |pane, _, _, cx| {
                                    let is_zoomed = pane.is_zoomed();
                                    pane.set_zoomed(!is_zoomed, cx);
                                    cx.notify();
                                }))
                                .tooltip({
                                    let focus_handle = focus_handle.clone();
                                    move |_window, cx| {
                                        let zoomed_text =
                                            if zoomed { "Minimize" } else { "Expand" };
                                        Tooltip::for_action_in(
                                            zoomed_text,
                                            &ToggleExpandItem,
                                            &focus_handle,
                                            cx,
                                        )
                                    }
                                }),
                            )
                    })
                    .into_any_element()
            }
        });
        pane
    })
}
