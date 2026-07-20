use super::*;

pub(super) struct TabContextMenuParams {
    pub(super) pane: WeakEntity<Pane>,
    pub(super) menu_context: FocusHandle,
    pub(super) item_handle: Box<dyn ItemHandle>,
    pub(super) item_id: EntityId,
    pub(super) ix: usize,
    pub(super) single_entry_to_resolve: Option<ProjectEntryId>,
    pub(super) total_items: usize,
    pub(super) has_multibuffer_items: bool,
    pub(super) has_items_to_left: bool,
    pub(super) has_items_to_right: bool,
    pub(super) has_clean_items: bool,
    pub(super) is_pinned: bool,
    pub(super) capability: Capability,
}

pub(super) fn render_tab_context_menu(
    tab: impl IntoElement + 'static,
    params: TabContextMenuParams,
) -> impl IntoElement {
    let TabContextMenuParams {
        pane,
        menu_context,
        item_handle,
        item_id,
        ix,
        single_entry_to_resolve,
        total_items,
        has_multibuffer_items,
        has_items_to_left,
        has_items_to_right,
        has_clean_items,
        is_pinned,
        capability,
    } = params;

    right_click_menu(ix)
        .trigger(|_, _, _| tab)
        .menu(move |window, cx| {
            let pane = pane.clone();
            let menu_context = menu_context.clone();
            let extra_actions = item_handle.tab_extra_context_menu_actions(window, cx);
            ContextMenu::build(window, cx, move |mut menu, window, cx| {
                let close_active_item_action = CloseActiveItem {
                    save_intent: None,
                    close_pinned: true,
                };
                let close_inactive_items_action = CloseOtherItems {
                    save_intent: None,
                    close_pinned: false,
                };
                let close_multibuffers_action = CloseMultibufferItems {
                    save_intent: None,
                    close_pinned: false,
                };
                let close_items_to_the_left_action = CloseItemsToTheLeft {
                    close_pinned: false,
                };
                let close_items_to_the_right_action = CloseItemsToTheRight {
                    close_pinned: false,
                };
                let close_clean_items_action = CloseCleanItems {
                    close_pinned: false,
                };
                let close_all_items_action = CloseAllItems {
                    save_intent: None,
                    close_pinned: false,
                };
                if let Some(pane) = pane.upgrade() {
                    menu = menu
                        .entry(
                            "Close",
                            Some(Box::new(close_active_item_action)),
                            window.handler_for(&pane, move |pane, window, cx| {
                                pane.close_item_by_id(item_id, SaveIntent::Close, window, cx)
                                    .detach_and_log_err(cx);
                            }),
                        )
                        .item(ContextMenuItem::Entry(
                            ContextMenuEntry::new("Close Others")
                                .action(Box::new(close_inactive_items_action.clone()))
                                .disabled(total_items == 1)
                                .handler(window.handler_for(&pane, move |pane, window, cx| {
                                    pane.close_other_items(
                                        &close_inactive_items_action,
                                        Some(item_id),
                                        window,
                                        cx,
                                    )
                                    .detach_and_log_err(cx);
                                })),
                        ))
                        // We make this optional, instead of using disabled as to not overwhelm the context menu unnecessarily
                        .extend(has_multibuffer_items.then(|| {
                            ContextMenuItem::Entry(
                                ContextMenuEntry::new("Close Multibuffers")
                                    .action(Box::new(close_multibuffers_action.clone()))
                                    .handler(window.handler_for(&pane, move |pane, window, cx| {
                                        pane.close_multibuffer_items(
                                            &close_multibuffers_action,
                                            window,
                                            cx,
                                        )
                                        .detach_and_log_err(cx);
                                    })),
                            )
                        }))
                        .separator()
                        .item(ContextMenuItem::Entry(
                            ContextMenuEntry::new("Close Left")
                                .action(Box::new(close_items_to_the_left_action.clone()))
                                .disabled(!has_items_to_left)
                                .handler(window.handler_for(&pane, move |pane, window, cx| {
                                    pane.close_items_to_the_left_by_id(
                                        Some(item_id),
                                        &close_items_to_the_left_action,
                                        window,
                                        cx,
                                    )
                                    .detach_and_log_err(cx);
                                })),
                        ))
                        .item(ContextMenuItem::Entry(
                            ContextMenuEntry::new("Close Right")
                                .action(Box::new(close_items_to_the_right_action.clone()))
                                .disabled(!has_items_to_right)
                                .handler(window.handler_for(&pane, move |pane, window, cx| {
                                    pane.close_items_to_the_right_by_id(
                                        Some(item_id),
                                        &close_items_to_the_right_action,
                                        window,
                                        cx,
                                    )
                                    .detach_and_log_err(cx);
                                })),
                        ))
                        .separator()
                        .item(ContextMenuItem::Entry(
                            ContextMenuEntry::new("Close Clean")
                                .action(Box::new(close_clean_items_action.clone()))
                                .disabled(!has_clean_items)
                                .handler(window.handler_for(&pane, move |pane, window, cx| {
                                    pane.close_clean_items(&close_clean_items_action, window, cx)
                                        .detach_and_log_err(cx)
                                })),
                        ))
                        .entry(
                            "Close All",
                            Some(Box::new(close_all_items_action.clone())),
                            window.handler_for(&pane, move |pane, window, cx| {
                                pane.close_all_items(&close_all_items_action, window, cx)
                                    .detach_and_log_err(cx)
                            }),
                        );

                    let pin_tab_entries = |menu: ContextMenu| {
                        menu.separator().map(|this| {
                            if is_pinned {
                                this.entry(
                                    "Unpin Tab",
                                    Some(TogglePinTab.boxed_clone()),
                                    window.handler_for(&pane, move |pane, window, cx| {
                                        pane.unpin_tab_at(ix, window, cx);
                                    }),
                                )
                            } else {
                                this.entry(
                                    "Pin Tab",
                                    Some(TogglePinTab.boxed_clone()),
                                    window.handler_for(&pane, move |pane, window, cx| {
                                        pane.pin_tab_at(ix, window, cx);
                                    }),
                                )
                            }
                        })
                    };

                    if capability != Capability::ReadOnly {
                        let read_only_label = if capability.editable() {
                            "Make File Read-Only"
                        } else {
                            "Make File Editable"
                        };
                        menu = menu.separator().entry(
                            read_only_label,
                            None,
                            window.handler_for(&pane, move |pane, window, cx| {
                                if let Some(item) = pane.item_for_index(ix) {
                                    item.toggle_read_only(window, cx);
                                }
                            }),
                        );
                    }

                    if let Some(entry) = single_entry_to_resolve {
                        let project_path = pane
                            .read(cx)
                            .item_for_entry(entry, cx)
                            .and_then(|item| item.project_path(cx));
                        let worktree = project_path.as_ref().and_then(|project_path| {
                            pane.read(cx)
                                .project
                                .upgrade()?
                                .read(cx)
                                .worktree_for_id(project_path.worktree_id, cx)
                        });
                        let has_relative_path = worktree.as_ref().is_some_and(|worktree| {
                            worktree
                                .read(cx)
                                .root_entry()
                                .is_some_and(|entry| entry.is_dir())
                        });

                        let entry_abs_path = pane.read(cx).entry_abs_path(entry, cx);
                        let reveal_path = entry_abs_path.clone();
                        let parent_abs_path = entry_abs_path
                            .as_deref()
                            .and_then(|abs_path| Some(abs_path.parent()?.to_path_buf()));
                        let relative_path = project_path
                            .map(|project_path| project_path.path)
                            .filter(|_| has_relative_path);

                        let visible_in_project_panel = relative_path.is_some()
                            && worktree.is_some_and(|worktree| worktree.read(cx).is_visible());
                        let is_local = pane.read(cx).project.upgrade().is_some_and(|project| {
                            let project = project.read(cx);
                            project.is_local() || project.is_via_wsl_with_host_interop(cx)
                        });
                        let is_remote = pane
                            .read(cx)
                            .project
                            .upgrade()
                            .is_some_and(|project| project.read(cx).is_remote());

                        let entry_id = entry.to_proto();

                        menu = menu
                            .separator()
                            .when_some(entry_abs_path, |menu, abs_path| {
                                menu.entry(
                                    "Copy Path",
                                    Some(Box::new(mav_actions::workspace::CopyPath)),
                                    window.handler_for(&pane, move |_, _, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            abs_path.to_string_lossy().into_owned(),
                                        ));
                                    }),
                                )
                            })
                            .when_some(relative_path, |menu, relative_path| {
                                menu.entry(
                                    "Copy Relative Path",
                                    Some(Box::new(mav_actions::workspace::CopyRelativePath)),
                                    window.handler_for(&pane, move |this, _, cx| {
                                        let Some(project) = this.project.upgrade() else {
                                            return;
                                        };
                                        let path_style = project
                                            .update(cx, |project, cx| project.path_style(cx));
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            relative_path.display(path_style).to_string(),
                                        ));
                                    }),
                                )
                            })
                            .when(is_local, |menu| {
                                menu.when_some(reveal_path, |menu, reveal_path| {
                                    menu.separator().entry(
                                        ui::utils::reveal_in_file_manager_label(is_remote),
                                        Some(Box::new(mav_actions::editor::RevealInFileManager)),
                                        window.handler_for(&pane, move |pane, _, cx| {
                                            if let Some(project) = pane.project.upgrade() {
                                                project.update(cx, |project, cx| {
                                                    project.reveal_path(&reveal_path, cx);
                                                });
                                            } else {
                                                cx.reveal_path(&reveal_path);
                                            }
                                        }),
                                    )
                                })
                            })
                            .map(pin_tab_entries)
                            .when(visible_in_project_panel, |menu| {
                                menu.entry(
                                    "Reveal In Project Panel",
                                    Some(Box::new(RevealInProjectPanel::default())),
                                    window.handler_for(&pane, move |pane, _, cx| {
                                        pane.project
                                            .update(cx, |_, cx| {
                                                cx.emit(project::Event::RevealInProjectPanel(
                                                    ProjectEntryId::from_proto(entry_id),
                                                ))
                                            })
                                            .ok();
                                    }),
                                )
                            })
                            .when_some(parent_abs_path, |menu, parent_abs_path| {
                                menu.entry(
                                    "Open in Terminal",
                                    Some(Box::new(OpenInTerminal)),
                                    window.handler_for(&pane, move |_, window, cx| {
                                        window.dispatch_action(
                                            OpenTerminal {
                                                working_directory: parent_abs_path.clone(),
                                                local: false,
                                            }
                                            .boxed_clone(),
                                            cx,
                                        );
                                    }),
                                )
                            });
                    } else {
                        menu = menu.map(pin_tab_entries);
                    }
                };

                // Add custom item-specific actions
                if !extra_actions.is_empty() {
                    menu = menu.separator();
                    for (label, action) in extra_actions {
                        menu = menu.action(label, action);
                    }
                }

                menu.context(menu_context)
            })
        })
}
