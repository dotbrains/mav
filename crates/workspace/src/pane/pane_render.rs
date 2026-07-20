use super::*;

pub(super) fn default_render_tab_bar_buttons(
    pane: &mut Pane,
    window: &mut Window,
    cx: &mut Context<Pane>,
) -> (Option<AnyElement>, Option<AnyElement>) {
    if !pane.has_focus(window, cx) && !pane.context_menu_focused(window, cx) {
        return (None, None);
    }
    let (can_clone, can_split_move) = match pane.active_item() {
        Some(active_item) if active_item.can_split(cx) => (true, false),
        Some(_) => (false, pane.items_len() > 1),
        None => (false, false),
    };
    // Ideally we would return a vec of elements here to pass directly to the [TabBar]'s
    // `end_slot`, but due to needing a view here that isn't possible.
    let right_children = h_flex()
        // Instead we need to replicate the spacing from the [TabBar]'s `end_slot` here.
        .gap(DynamicSpacing::Base04.rems(cx))
        .child(
            PopoverMenu::new("pane-tab-bar-popover-menu")
                .trigger_with_tooltip(
                    IconButton::new("plus", IconName::Plus).icon_size(IconSize::Small),
                    Tooltip::text("New..."),
                )
                .anchor(Anchor::TopRight)
                .with_handle(pane.new_item_context_menu_handle.clone())
                .menu(move |window, cx| {
                    Some(ContextMenu::build(window, cx, |menu, _, _| {
                        menu.action("New File", NewFile.boxed_clone())
                            .action("Open File", ToggleFileFinder::default().boxed_clone())
                            .separator()
                            .action("Search Project", DeploySearch::default().boxed_clone())
                            .action("Search Symbols", ToggleProjectSymbols.boxed_clone())
                            .separator()
                            .action("New Terminal", NewTerminal::default().boxed_clone())
                            .action(
                                "New Center Terminal",
                                NewCenterTerminal::default().boxed_clone(),
                            )
                    }))
                }),
        )
        .child(
            PopoverMenu::new("pane-tab-bar-split")
                .trigger_with_tooltip(
                    IconButton::new("split", IconName::Split)
                        .icon_size(IconSize::Small)
                        .disabled(!can_clone && !can_split_move),
                    Tooltip::text("Split Pane"),
                )
                .anchor(Anchor::TopRight)
                .with_handle(pane.split_item_context_menu_handle.clone())
                .menu(move |window, cx| {
                    ContextMenu::build(window, cx, |menu, _, _| {
                        let mode = SplitMode::MovePane;
                        if can_split_move {
                            menu.action("Split Right", SplitRight { mode }.boxed_clone())
                                .action("Split Left", SplitLeft { mode }.boxed_clone())
                                .action("Split Up", SplitUp { mode }.boxed_clone())
                                .action("Split Down", SplitDown { mode }.boxed_clone())
                        } else {
                            menu.action("Split Right", SplitRight::default().boxed_clone())
                                .action("Split Left", SplitLeft::default().boxed_clone())
                                .action("Split Up", SplitUp::default().boxed_clone())
                                .action("Split Down", SplitDown::default().boxed_clone())
                        }
                    })
                    .into()
                }),
        )
        .child(render_toggle_zoom_button(pane, cx))
        .into_any_element()
        .into();
    (None, right_children)
}

impl Focusable for Pane {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Pane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut key_context = KeyContext::new_with_defaults();
        key_context.add("Pane");
        if self.active_item().is_none() {
            key_context.add("EmptyPane");
        }

        self.toolbar
            .read(cx)
            .contribute_context(&mut key_context, cx);

        let should_display_tab_bar = self.should_display_tab_bar.clone();
        let display_tab_bar = should_display_tab_bar(window, cx);
        let Some(project) = self.project.upgrade() else {
            return div().track_focus(&self.focus_handle(cx));
        };
        let is_local = project.read(cx).is_local();

        v_flex()
            .key_context(key_context)
            .track_focus(&self.focus_handle(cx))
            .relative()
            .size_full()
            .flex_none()
            .overflow_hidden()
            .on_action(cx.listener(|pane, split: &SplitLeft, window, cx| {
                pane.split(SplitDirection::Left, split.mode, window, cx)
            }))
            .on_action(cx.listener(|pane, split: &SplitUp, window, cx| {
                pane.split(SplitDirection::Up, split.mode, window, cx)
            }))
            .on_action(cx.listener(|pane, split: &SplitHorizontal, window, cx| {
                pane.split(SplitDirection::horizontal(cx), split.mode, window, cx)
            }))
            .on_action(cx.listener(|pane, split: &SplitVertical, window, cx| {
                pane.split(SplitDirection::vertical(cx), split.mode, window, cx)
            }))
            .on_action(cx.listener(|pane, split: &SplitRight, window, cx| {
                pane.split(SplitDirection::Right, split.mode, window, cx)
            }))
            .on_action(cx.listener(|pane, split: &SplitDown, window, cx| {
                pane.split(SplitDirection::Down, split.mode, window, cx)
            }))
            .on_action(cx.listener(|pane, _: &SplitAndMoveUp, window, cx| {
                pane.split(SplitDirection::Up, SplitMode::MovePane, window, cx)
            }))
            .on_action(cx.listener(|pane, _: &SplitAndMoveDown, window, cx| {
                pane.split(SplitDirection::Down, SplitMode::MovePane, window, cx)
            }))
            .on_action(cx.listener(|pane, _: &SplitAndMoveLeft, window, cx| {
                pane.split(SplitDirection::Left, SplitMode::MovePane, window, cx)
            }))
            .on_action(cx.listener(|pane, _: &SplitAndMoveRight, window, cx| {
                pane.split(SplitDirection::Right, SplitMode::MovePane, window, cx)
            }))
            .on_action(cx.listener(|_, _: &JoinIntoNext, _, cx| {
                cx.emit(Event::JoinIntoNext);
            }))
            .on_action(cx.listener(|_, _: &JoinAll, _, cx| {
                cx.emit(Event::JoinAll);
            }))
            .on_action(cx.listener(Pane::toggle_zoom))
            .on_action(cx.listener(Pane::zoom_in))
            .on_action(cx.listener(Pane::zoom_out))
            .on_action(cx.listener(Self::navigate_backward))
            .on_action(cx.listener(Self::navigate_forward))
            .on_action(cx.listener(Self::go_to_older_tag))
            .on_action(cx.listener(Self::go_to_newer_tag))
            .on_action(
                cx.listener(|pane: &mut Pane, action: &ActivateItem, window, cx| {
                    pane.activate_item(
                        action.0.min(pane.items.len().saturating_sub(1)),
                        true,
                        true,
                        window,
                        cx,
                    );
                }),
            )
            .on_action(cx.listener(Self::alternate_file))
            .on_action(cx.listener(Self::activate_last_item))
            .on_action(cx.listener(Self::activate_previous_item))
            .on_action(cx.listener(Self::activate_next_item))
            .on_action(cx.listener(Self::swap_item_left))
            .on_action(cx.listener(Self::swap_item_right))
            .on_action(cx.listener(Self::toggle_pin_tab))
            .on_action(cx.listener(Self::unpin_all_tabs))
            .when(PreviewTabsSettings::get_global(cx).enabled, |this| {
                this.on_action(
                    cx.listener(|pane: &mut Pane, _: &TogglePreviewTab, window, cx| {
                        if let Some(active_item_id) = pane.active_item().map(|i| i.item_id()) {
                            if pane.is_active_preview_item(active_item_id) {
                                pane.unpreview_item_if_preview(active_item_id);
                            } else {
                                pane.replace_preview_item_id(active_item_id, window, cx);
                            }
                        }
                    }),
                )
            })
            .on_action(
                cx.listener(|pane: &mut Self, action: &CloseActiveItem, window, cx| {
                    pane.close_active_item(action, window, cx)
                        .detach_and_log_err(cx)
                }),
            )
            .on_action(
                cx.listener(|pane: &mut Self, action: &CloseOtherItems, window, cx| {
                    pane.close_other_items(action, None, window, cx)
                        .detach_and_log_err(cx);
                }),
            )
            .on_action(
                cx.listener(|pane: &mut Self, action: &CloseCleanItems, window, cx| {
                    pane.close_clean_items(action, window, cx)
                        .detach_and_log_err(cx)
                }),
            )
            .on_action(cx.listener(
                |pane: &mut Self, action: &CloseItemsToTheLeft, window, cx| {
                    pane.close_items_to_the_left_by_id(None, action, window, cx)
                        .detach_and_log_err(cx)
                },
            ))
            .on_action(cx.listener(
                |pane: &mut Self, action: &CloseItemsToTheRight, window, cx| {
                    pane.close_items_to_the_right_by_id(None, action, window, cx)
                        .detach_and_log_err(cx)
                },
            ))
            .on_action(
                cx.listener(|pane: &mut Self, action: &CloseAllItems, window, cx| {
                    pane.close_all_items(action, window, cx)
                        .detach_and_log_err(cx)
                }),
            )
            .on_action(cx.listener(
                |pane: &mut Self, action: &CloseMultibufferItems, window, cx| {
                    pane.close_multibuffer_items(action, window, cx)
                        .detach_and_log_err(cx)
                },
            ))
            .on_action(cx.listener(
                |pane: &mut Self, action: &RevealInProjectPanel, _window, cx| {
                    let active_item = pane.active_item();
                    let entry_id = active_item.as_ref().and_then(|item| {
                        action
                            .entry_id
                            .map(ProjectEntryId::from_proto)
                            .or_else(|| item.project_entry_ids(cx).first().copied())
                    });

                    pane.project
                        .update(cx, |project, cx| {
                            if let Some(entry_id) = entry_id
                                && project
                                    .worktree_for_entry(entry_id, cx)
                                    .is_some_and(|worktree| worktree.read(cx).is_visible())
                            {
                                return cx.emit(project::Event::RevealInProjectPanel(entry_id));
                            }

                            // When no entry is found, which is the case when
                            // working with an unsaved buffer, or the worktree
                            // is not visible, for example, a file that doesn't
                            // belong to an open project, we can't reveal the
                            // entry but we still want to activate the project
                            // panel.
                            cx.emit(project::Event::ActivateProjectPanel);
                        })
                        .log_err();
                },
            ))
            .on_action(cx.listener(|_, _: &menu::Cancel, window, cx| {
                if cx.stop_active_drag(window) {
                } else {
                    cx.propagate();
                }
            }))
            .when(self.active_item().is_some() && display_tab_bar, |element| {
                let header = (self.render_tab_bar.clone())(self, window, cx);
                element.child(self.render_header_with_traffic_light_spacer(header, window, cx))
            })
            .child({
                let has_worktrees = project.read(cx).visible_worktrees(cx).next().is_some();
                let body_accepts_dragged_selection = self.pane_kind != PaneKind::Project;
                // main content
                div()
                    .flex_1()
                    .relative()
                    .group("")
                    .overflow_hidden()
                    .on_drag_move::<DraggedTab>(cx.listener(Self::handle_drag_move))
                    .on_drag_move::<DraggedSelection>(cx.listener(Self::handle_drag_move))
                    .on_drag_move::<DraggedPane>(cx.listener(Self::handle_drag_move))
                    .when(is_local, |div| {
                        div.on_drag_move::<ExternalPaths>(cx.listener(Self::handle_drag_move))
                    })
                    .map(|div| {
                        if let Some(item) = self.active_item() {
                            div.id("pane_placeholder")
                                .v_flex()
                                .size_full()
                                .overflow_hidden()
                                .child(self.toolbar.clone())
                                .child(item.to_any_view())
                        } else {
                            let placeholder = div
                                .id("pane_placeholder")
                                .h_flex()
                                .size_full()
                                .justify_center()
                                .on_click(cx.listener(
                                    move |this, event: &ClickEvent, window, cx| {
                                        if event.click_count() == 2 {
                                            window.dispatch_action(
                                                this.double_click_dispatch_action.boxed_clone(),
                                                cx,
                                            );
                                        }
                                    },
                                ));
                            if has_worktrees || !self.should_display_welcome_page {
                                placeholder
                            } else {
                                if self.welcome_page.is_none() {
                                    let workspace = self.workspace.clone();
                                    self.welcome_page = Some(cx.new(|cx| {
                                        crate::welcome::WelcomePage::new(
                                            workspace, true, window, cx,
                                        )
                                    }));
                                }
                                placeholder.child(self.welcome_page.clone().unwrap())
                            }
                        }
                        .focus_follows_mouse(self.focus_follows_mouse, cx)
                    })
                    .child(
                        // drag target
                        div()
                            .invisible()
                            .when(self.drag_tab_target && cx.has_active_drag(), |div| {
                                div.visible()
                                    .bg(cx.theme().colors().drop_target_background)
                                    .border_2()
                                    .border_color(cx.theme().colors().drop_target_border)
                                    .rounded_lg()
                            })
                            .absolute()
                            .top_0()
                            .right_0()
                            .bottom_0()
                            .left_0()
                            .on_drop(cx.listener(move |this, dragged_tab, window, cx| {
                                let target_ix = if this.drag_tab_target {
                                    this.items.len()
                                } else {
                                    this.active_item_index()
                                };
                                this.handle_tab_drop(dragged_tab, target_ix, true, window, cx)
                            }))
                            .when(body_accepts_dragged_selection, |div| {
                                div.on_drop(cx.listener(
                                    move |this, selection: &DraggedSelection, window, cx| {
                                        let target_ix = if this.drag_tab_target {
                                            Some(this.items.len())
                                        } else {
                                            None
                                        };
                                        this.handle_dragged_selection_drop(
                                            selection, target_ix, window, cx,
                                        )
                                    },
                                ))
                            })
                            .on_drop(cx.listener(
                                move |this, dragged_pane: &DraggedPane, window, cx| {
                                    this.handle_pane_drop(dragged_pane, window, cx)
                                },
                            ))
                            .on_drop(cx.listener(move |this, paths, window, cx| {
                                this.handle_external_paths_drop(paths, window, cx)
                            }))
                            .map(|div| {
                                let size = DefiniteLength::Fraction(0.5);
                                match self.drag_split_direction {
                                    None => div.top_0().right_0().bottom_0().left_0(),
                                    Some(SplitDirection::Up) => {
                                        div.top_0().left_0().right_0().h(size)
                                    }
                                    Some(SplitDirection::Down) => {
                                        div.left_0().bottom_0().right_0().h(size)
                                    }
                                    Some(SplitDirection::Left) => {
                                        div.top_0().left_0().bottom_0().w(size)
                                    }
                                    Some(SplitDirection::Right) => {
                                        div.top_0().bottom_0().right_0().w(size)
                                    }
                                }
                            }),
                    )
            })
            .when_some(
                self.drag_split_direction.filter(|_| cx.has_active_drag()),
                |this, direction| {
                    this.child(Self::render_split_drop_overlay(
                        direction,
                        self.pane_kind != PaneKind::Project,
                        cx,
                    ))
                },
            )
            .when(self.drag_swap_target && cx.has_active_drag(), |this| {
                this.child(Self::render_swap_drop_overlay(cx))
            })
            .child(self.render_pane_drag_handle(cx))
            .on_mouse_down(
                MouseButton::Navigate(NavigationDirection::Back),
                cx.listener(|pane, _, window, cx| {
                    if let Some(workspace) = pane.workspace.upgrade() {
                        let pane = cx.entity().downgrade();
                        window.defer(cx, move |window, cx| {
                            workspace.update(cx, |workspace, cx| {
                                workspace.go_back(pane, window, cx).detach_and_log_err(cx)
                            })
                        })
                    }
                }),
            )
            .on_mouse_down(
                MouseButton::Navigate(NavigationDirection::Forward),
                cx.listener(|pane, _, window, cx| {
                    if let Some(workspace) = pane.workspace.upgrade() {
                        let pane = cx.entity().downgrade();
                        window.defer(cx, move |window, cx| {
                            workspace.update(cx, |workspace, cx| {
                                workspace
                                    .go_forward(pane, window, cx)
                                    .detach_and_log_err(cx)
                            })
                        })
                    }
                }),
            )
    }
}
