use super::*;

impl Render for ProjectPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let has_worktree = !self.state.visible_entries.is_empty();
        let project = self.project.read(cx);
        let panel_settings = ProjectPanelSettings::get_global(cx);
        let indent_size = panel_settings.indent_size;
        let show_indent_guides = panel_settings.indent_guides.show == ShowIndentGuides::Always;
        let horizontal_scroll = panel_settings.scrollbar.horizontal_scroll;
        let show_sticky_entries = self.should_show_sticky_entries(panel_settings);

        let is_local = project.is_local();

        if has_worktree {
            let item_count = self.visible_entry_count();

            h_flex()
                .id("project-panel")
                .group("project-panel")
                .when(panel_settings.drag_and_drop, |this| {
                    this.on_drag_move(cx.listener(Self::handle_panel_drag_move::<ExternalPaths>))
                        .on_drag_move(cx.listener(Self::handle_panel_drag_move::<DraggedSelection>))
                })
                .size_full()
                .bg(cx.theme().colors().editor_background)
                .relative()
                .on_modifiers_changed(cx.listener(
                    |this, event: &ModifiersChangedEvent, window, cx| {
                        this.refresh_drag_cursor_style(&event.modifiers, window, cx);
                    },
                ))
                .key_context(self.dispatch_context(window, cx))
                .on_action(cx.listener(Self::scroll_up))
                .on_action(cx.listener(Self::scroll_down))
                .on_action(cx.listener(Self::scroll_cursor_center))
                .on_action(cx.listener(Self::scroll_cursor_top))
                .on_action(cx.listener(Self::scroll_cursor_bottom))
                .on_action(cx.listener(Self::select_next))
                .on_action(cx.listener(Self::select_previous))
                .on_action(cx.listener(Self::select_first))
                .on_action(cx.listener(Self::select_last))
                .on_action(cx.listener(Self::select_parent))
                .on_action(cx.listener(Self::select_next_git_entry))
                .on_action(cx.listener(Self::select_prev_git_entry))
                .on_action(cx.listener(Self::select_next_diagnostic))
                .on_action(cx.listener(Self::select_prev_diagnostic))
                .on_action(cx.listener(Self::select_next_directory))
                .on_action(cx.listener(Self::select_prev_directory))
                .on_action(cx.listener(Self::expand_selected_entry))
                .on_action(cx.listener(Self::collapse_selected_entry))
                .on_action(cx.listener(Self::collapse_all_entries))
                .on_action(cx.listener(Self::expand_all_entries))
                .on_action(cx.listener(Self::collapse_selected_entry_and_children))
                .on_action(cx.listener(Self::expand_selected_entry_and_children))
                .on_action(cx.listener(Self::open))
                .on_action(cx.listener(Self::open_permanent))
                .on_action(cx.listener(Self::open_split_vertical))
                .on_action(cx.listener(Self::open_split_horizontal))
                .on_action(cx.listener(Self::open_markdown_preview))
                .on_action(cx.listener(Self::confirm))
                .on_action(cx.listener(Self::cancel))
                .on_action(cx.listener(Self::copy_path))
                .on_action(cx.listener(Self::copy_relative_path))
                .on_action(cx.listener(Self::new_search_in_directory))
                .on_action(cx.listener(Self::unfold_directory))
                .on_action(cx.listener(Self::fold_directory))
                .on_action(cx.listener(Self::remove_from_project))
                .on_action(cx.listener(Self::compare_marked_files))
                .when(cx.has_flag::<ProjectPanelUndoRedoFeatureFlag>(), |el| {
                    el.on_action(cx.listener(Self::undo))
                        .on_action(cx.listener(Self::redo))
                })
                .when(!project.is_read_only(cx), |el| {
                    el.on_action(cx.listener(Self::new_file))
                        .on_action(cx.listener(Self::new_directory))
                        .on_action(cx.listener(Self::rename))
                        .on_action(cx.listener(Self::delete))
                        .on_action(cx.listener(Self::cut))
                        .on_action(cx.listener(Self::copy))
                        .on_action(cx.listener(Self::paste))
                        .on_action(cx.listener(Self::duplicate))
                        .on_action(cx.listener(Self::restore_file))
                        .on_action(cx.listener(Self::add_to_gitignore))
                        .on_action(cx.listener(Self::add_to_git_info_exclude))
                        .when(!project.is_remote(), |el| {
                            el.on_action(cx.listener(Self::trash))
                        })
                })
                .when(
                    project.is_local() || project.is_via_wsl_with_host_interop(cx),
                    |el| {
                        el.on_action(cx.listener(Self::reveal_in_finder))
                            .on_action(cx.listener(Self::open_system))
                            .on_action(cx.listener(Self::open_in_terminal))
                    },
                )
                .when(project.is_via_remote_server(), |el| {
                    el.on_action(cx.listener(Self::open_in_terminal))
                        .on_action(cx.listener(Self::download_from_remote))
                })
                .track_focus(&self.focus_handle(cx))
                .child(
                    v_flex()
                        .child(
                            uniform_list("entries", item_count, {
                                cx.processor(|this, range: Range<usize>, window, cx| {
                                    this.rendered_entries_len = range.end - range.start;
                                    let mut items = Vec::with_capacity(this.rendered_entries_len);
                                    let marked_selections: Arc<[SelectedEntry]> =
                                        Arc::from(this.marked_entries.clone());
                                    this.for_each_visible_entry(
                                        range,
                                        window,
                                        cx,
                                        &mut |id, details, window, cx| {
                                            items.push(this.render_entry(
                                                id,
                                                details,
                                                Arc::clone(&marked_selections),
                                                window,
                                                cx,
                                            ));
                                        },
                                    );
                                    items
                                })
                            })
                            .when(show_indent_guides, |list| {
                                list.with_decoration(
                                    ui::indent_guides(
                                        px(indent_size),
                                        IndentGuideColors::panel(cx),
                                    )
                                    .with_compute_indents_fn(
                                        cx.entity(),
                                        |this, range, window, cx| {
                                            let mut items =
                                                SmallVec::with_capacity(range.end - range.start);
                                            this.iter_visible_entries(
                                                range,
                                                window,
                                                cx,
                                                &mut |entry, _, entries, _, _| {
                                                    let (depth, _) =
                                                        Self::calculate_depth_and_difference(
                                                            entry, entries,
                                                        );
                                                    items.push(depth);
                                                },
                                            );
                                            items
                                        },
                                    )
                                    .on_click(cx.listener(
                                        |this,
                                         active_indent_guide: &IndentGuideLayout,
                                         window,
                                         cx| {
                                            if window.modifiers().secondary() {
                                                let ix = active_indent_guide.offset.y;
                                                let Some((target_entry, worktree)) = maybe!({
                                                    let (worktree_id, entry) =
                                                        this.entry_at_index(ix)?;
                                                    let worktree = this
                                                        .project
                                                        .read(cx)
                                                        .worktree_for_id(worktree_id, cx)?;
                                                    let target_entry = worktree
                                                        .read(cx)
                                                        .entry_for_path(&entry.path.parent()?)?;
                                                    Some((target_entry, worktree))
                                                }) else {
                                                    return;
                                                };

                                                this.collapse_entry(
                                                    target_entry.clone(),
                                                    worktree,
                                                    window,
                                                    cx,
                                                );
                                            }
                                        },
                                    ))
                                    .with_render_fn(
                                        cx.entity(),
                                        move |this, params, _, cx| {
                                            const LEFT_OFFSET: Pixels = px(14.);
                                            const PADDING_Y: Pixels = px(4.);
                                            const HITBOX_OVERDRAW: Pixels = px(3.);

                                            let active_indent_guide_index = this
                                                .find_active_indent_guide(
                                                    &params.indent_guides,
                                                    cx,
                                                );

                                            let indent_size = params.indent_size;
                                            let item_height = params.item_height;

                                            params
                                                .indent_guides
                                                .into_iter()
                                                .enumerate()
                                                .map(|(idx, layout)| {
                                                    let offset = if layout.continues_offscreen {
                                                        px(0.)
                                                    } else {
                                                        PADDING_Y
                                                    };
                                                    let bounds = Bounds::new(
                                                        point(
                                                            layout.offset.x * indent_size
                                                                + LEFT_OFFSET,
                                                            layout.offset.y * item_height + offset,
                                                        ),
                                                        size(
                                                            px(1.),
                                                            layout.length * item_height
                                                                - offset * 2.,
                                                        ),
                                                    );
                                                    ui::RenderedIndentGuide {
                                                        bounds,
                                                        layout,
                                                        is_active: Some(idx)
                                                            == active_indent_guide_index,
                                                        hitbox: Some(Bounds::new(
                                                            point(
                                                                bounds.origin.x - HITBOX_OVERDRAW,
                                                                bounds.origin.y,
                                                            ),
                                                            size(
                                                                bounds.size.width
                                                                    + HITBOX_OVERDRAW * 2.,
                                                                bounds.size.height,
                                                            ),
                                                        )),
                                                    }
                                                })
                                                .collect()
                                        },
                                    ),
                                )
                            })
                            .when(show_sticky_entries, |list| {
                                let sticky_items = ui::sticky_items(
                                    cx.entity(),
                                    |this, range, window, cx| {
                                        let mut items =
                                            SmallVec::with_capacity(range.end - range.start);
                                        this.iter_visible_entries(
                                            range,
                                            window,
                                            cx,
                                            &mut |entry, index, entries, _, _| {
                                                let (depth, _) =
                                                    Self::calculate_depth_and_difference(
                                                        entry, entries,
                                                    );
                                                let candidate =
                                                    StickyProjectPanelCandidate { index, depth };
                                                items.push(candidate);
                                            },
                                        );
                                        items
                                    },
                                    |this, marker_entry, window, cx| {
                                        let sticky_entries =
                                            this.render_sticky_entries(marker_entry, window, cx);
                                        this.sticky_items_count = sticky_entries.len();
                                        sticky_entries
                                    },
                                );
                                list.with_decoration(if show_indent_guides {
                                    sticky_items.with_decoration(
                                        ui::indent_guides(
                                            px(indent_size),
                                            IndentGuideColors::panel(cx),
                                        )
                                        .with_render_fn(
                                            cx.entity(),
                                            move |_, params, _, _| {
                                                const LEFT_OFFSET: Pixels = px(14.);

                                                let indent_size = params.indent_size;
                                                let item_height = params.item_height;

                                                params
                                                    .indent_guides
                                                    .into_iter()
                                                    .map(|layout| {
                                                        let bounds = Bounds::new(
                                                            point(
                                                                layout.offset.x * indent_size
                                                                    + LEFT_OFFSET,
                                                                layout.offset.y * item_height,
                                                            ),
                                                            size(
                                                                px(1.),
                                                                layout.length * item_height,
                                                            ),
                                                        );
                                                        ui::RenderedIndentGuide {
                                                            bounds,
                                                            layout,
                                                            is_active: false,
                                                            hitbox: None,
                                                        }
                                                    })
                                                    .collect()
                                            },
                                        ),
                                    )
                                } else {
                                    sticky_items
                                })
                            })
                            .with_sizing_behavior(ListSizingBehavior::Infer)
                            .with_horizontal_sizing_behavior(if horizontal_scroll {
                                ListHorizontalSizingBehavior::Unconstrained
                            } else {
                                ListHorizontalSizingBehavior::FitList
                            })
                            .when(horizontal_scroll, |list| {
                                list.with_width_from_item(self.state.max_width_item_index)
                            })
                            .track_scroll(&self.scroll_handle),
                        )
                        .child(
                            div()
                                .id("project-panel-blank-area")
                                .block_mouse_except_scroll()
                                .flex_grow_1()
                                .on_scroll_wheel({
                                    let scroll_handle = self.scroll_handle.clone();
                                    let entity_id = cx.entity().entity_id();
                                    move |event, window, cx| {
                                        let state = scroll_handle.0.borrow();
                                        let base_handle = &state.base_handle;
                                        let current_offset = base_handle.offset();
                                        let max_offset = base_handle.max_offset();
                                        let delta = event.delta.pixel_delta(window.line_height());
                                        let new_offset = (current_offset + delta)
                                            .clamp(&max_offset.neg(), &Point::default());

                                        if new_offset != current_offset {
                                            base_handle.set_offset(new_offset);
                                            cx.notify(entity_id);
                                        }
                                    }
                                })
                                .when(
                                    self.drag_target_entry.as_ref().is_some_and(
                                        |entry| match entry {
                                            DragTarget::Background => true,
                                            DragTarget::Entry {
                                                highlight_entry_id, ..
                                            } => self.state.last_worktree_root_id.is_some_and(
                                                |root_id| *highlight_entry_id == root_id,
                                            ),
                                        },
                                    ),
                                    |div| div.bg(cx.theme().colors().drop_target_background),
                                )
                                .on_drag_move::<ExternalPaths>(cx.listener(
                                    move |this, event: &DragMoveEvent<ExternalPaths>, _, _| {
                                        let Some(_last_root_id) = this.state.last_worktree_root_id
                                        else {
                                            return;
                                        };
                                        if event.bounds.contains(&event.event.position) {
                                            this.drag_target_entry = Some(DragTarget::Background);
                                        } else {
                                            if this.drag_target_entry.as_ref().is_some_and(|e| {
                                                matches!(e, DragTarget::Background)
                                            }) {
                                                this.drag_target_entry = None;
                                            }
                                        }
                                    },
                                ))
                                .on_drag_move::<DraggedSelection>(cx.listener(
                                    move |this, event: &DragMoveEvent<DraggedSelection>, _, cx| {
                                        let Some(last_root_id) = this.state.last_worktree_root_id
                                        else {
                                            return;
                                        };
                                        if event.bounds.contains(&event.event.position) {
                                            let drag_state = event.drag(cx);
                                            if this.should_highlight_background_for_selection_drag(
                                                &drag_state,
                                                last_root_id,
                                                cx,
                                            ) {
                                                this.drag_target_entry =
                                                    Some(DragTarget::Background);
                                            }
                                        } else {
                                            if this.drag_target_entry.as_ref().is_some_and(|e| {
                                                matches!(e, DragTarget::Background)
                                            }) {
                                                this.drag_target_entry = None;
                                            }
                                        }
                                    },
                                ))
                                .on_drop(cx.listener(
                                    move |this, external_paths: &ExternalPaths, window, cx| {
                                        this.drag_target_entry = None;
                                        this.hover_scroll_task.take();
                                        if let Some(entry_id) = this.state.last_worktree_root_id {
                                            this.drop_external_files(
                                                external_paths.paths(),
                                                entry_id,
                                                window,
                                                cx,
                                            );
                                        }
                                        cx.stop_propagation();
                                    },
                                ))
                                .on_drop(cx.listener(
                                    move |this, selections: &DraggedSelection, window, cx| {
                                        this.drag_target_entry = None;
                                        this.hover_scroll_task.take();
                                        if let Some(entry_id) = this.state.last_worktree_root_id {
                                            this.drag_onto(selections, entry_id, false, window, cx);
                                        }
                                        cx.stop_propagation();
                                    },
                                ))
                                .on_click(cx.listener(|this, event, window, cx| {
                                    if matches!(event, gpui::ClickEvent::Keyboard(_)) {
                                        return;
                                    }
                                    cx.stop_propagation();
                                    this.selection = None;
                                    this.marked_entries.clear();
                                    this.focus_handle(cx).focus(window, cx);
                                }))
                                .on_mouse_down(
                                    MouseButton::Right,
                                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                        // When deploying the context menu anywhere below the last project entry,
                                        // act as if the user clicked the root of the last worktree.
                                        if let Some(entry_id) = this.state.last_worktree_root_id {
                                            this.deploy_context_menu(
                                                event.position,
                                                entry_id,
                                                window,
                                                cx,
                                            );
                                        }
                                    }),
                                )
                                .when(!project.is_read_only(cx), |el| {
                                    el.on_click(cx.listener(
                                        |this, event: &gpui::ClickEvent, window, cx| {
                                            if event.click_count() > 1
                                                && let Some(entry_id) =
                                                    this.state.last_worktree_root_id
                                            {
                                                let project = this.project.read(cx);

                                                let worktree_id = if let Some(worktree) =
                                                    project.worktree_for_entry(entry_id, cx)
                                                {
                                                    worktree.read(cx).id()
                                                } else {
                                                    return;
                                                };

                                                this.selection = Some(SelectedEntry {
                                                    worktree_id,
                                                    entry_id,
                                                });

                                                this.new_file(&NewFile, window, cx);
                                            }
                                        },
                                    ))
                                }),
                        )
                        .size_full(),
                )
                .custom_scrollbars(self.panel_scrollbars(horizontal_scroll, cx), window, cx)
                .children(self.context_menu.as_ref().map(|(menu, position, _)| {
                    Self::render_context_menu_layer(menu.clone(), *position)
                }))
        } else {
            self.render_empty_project_panel(is_local, panel_settings.drag_and_drop, cx)
        }
    }
}
