use super::*;

impl ProjectPanel {
    pub(super) fn render_entry(
        &self,
        entry_id: ProjectEntryId,
        details: EntryDetails,
        marked_selections: Arc<[SelectedEntry]>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        const GROUP_NAME: &str = "project_entry";

        let kind = details.kind;
        let is_sticky = details.sticky.is_some();
        let sticky_index = details.sticky.as_ref().map(|this| this.sticky_index);
        let settings = ProjectPanelSettings::get_global(cx);
        let show_editor = details.is_editing && !details.is_processing;

        let selection = SelectedEntry {
            worktree_id: details.worktree_id,
            entry_id,
        };

        let is_marked = self.marked_entries.contains(&selection);
        let is_active = self
            .selection
            .is_some_and(|selection| selection.entry_id == entry_id);

        let file_name = details.filename.clone();

        let mut icon = details.icon.clone();
        if settings.file_icons && show_editor && details.kind.is_file() {
            let filename = self.filename_editor.read(cx).text(cx);
            if filename.len() > 2 {
                icon = FileIcons::get_icon(Path::new(&filename), cx);
            }
        }

        let filename_text_color = details.filename_text_color;
        let diagnostic_severity = details.diagnostic_severity;
        let diagnostic_count = details.diagnostic_count;
        let item_colors = get_item_color(is_sticky, cx);

        let canonical_path = details.canonical_path.clone();
        let path_style = self.project.read(cx).path_style(cx);
        let path = details.path.clone();

        let depth = details.depth;
        let worktree_id = details.worktree_id;

        let bg_color = if is_marked {
            item_colors.marked
        } else {
            item_colors.default
        };

        let bg_hover_color = if is_marked {
            item_colors.marked
        } else {
            item_colors.hover
        };

        let validation_color_and_message = if show_editor {
            match self
                .state
                .edit_state
                .as_ref()
                .map_or(ValidationState::None, |e| e.validation_state.clone())
            {
                ValidationState::Error(msg) => Some((Color::Error.color(cx), msg)),
                ValidationState::Warning(msg) => Some((Color::Warning.color(cx), msg)),
                ValidationState::None => None,
            }
        } else {
            None
        };

        let border_color = self.rendered_entry_border_color(
            is_active,
            bg_color,
            item_colors.focused,
            validation_color_and_message.clone(),
            window,
            cx,
        );
        let border_hover_color = self.rendered_entry_border_color(
            is_active,
            bg_hover_color,
            item_colors.focused,
            validation_color_and_message.clone(),
            window,
            cx,
        );

        let folded_directory_drag_target = self.folded_directory_drag_target;
        let is_highlighted = self.is_entry_highlighted(entry_id, worktree_id, &path, cx);
        let git_indicator = Self::rendered_entry_git_indicator(settings, details.git_status);

        let id = Self::rendered_entry_id(entry_id, is_sticky);

        div()
            .id(id.clone())
            .relative()
            .group(GROUP_NAME)
            .cursor_pointer()
            .rounded_none()
            .bg(bg_color)
            .border_1()
            .border_r_2()
            .border_color(border_color)
            .hover(|style| style.bg(bg_hover_color).border_color(border_hover_color))
            .when(is_sticky, |this| this.block_mouse_except_scroll())
            .when(!is_sticky, |this| {
                this.when(
                    is_highlighted && folded_directory_drag_target.is_none(),
                    |this| {
                        this.border_color(transparent_white())
                            .bg(item_colors.drag_over)
                    },
                )
                .when(settings.drag_and_drop, |this| {
                    let path_for_external_paths = path.clone();
                    let path_for_dragged_selection = path.clone();
                    let source_pane = self.workspace.upgrade().and_then(|workspace| {
                        workspace
                            .read(cx)
                            .panel_pane_for_kind(PaneKind::Project, cx)
                            .map(|pane| pane.downgrade())
                    });
                    let dragged_selection = DraggedSelection {
                        active_selection: selection,
                        marked_selections: marked_selections.clone(),
                        source_pane,
                        active_selection_is_file: kind.is_file(),
                    };

                    this.on_drag_move::<ExternalPaths>(cx.listener(
                        move |this, event: &DragMoveEvent<ExternalPaths>, _, cx| {
                            let is_current_target =
                                this.drag_target_entry
                                    .as_ref()
                                    .and_then(|entry| match entry {
                                        DragTarget::Entry {
                                            entry_id: target_id,
                                            ..
                                        } => Some(*target_id),
                                        DragTarget::Background { .. } => None,
                                    })
                                    == Some(entry_id);

                            if !event.bounds.contains(&event.event.position) {
                                // Entry responsible for setting drag target is also responsible to
                                // clear it up after drag is out of bounds
                                if is_current_target {
                                    this.drag_target_entry = None;
                                }
                                return;
                            }

                            if is_current_target {
                                return;
                            }

                            this.marked_entries.clear();

                            let Some((entry_id, highlight_entry_id)) = maybe!({
                                let target_worktree = this
                                    .project
                                    .read(cx)
                                    .worktree_for_id(selection.worktree_id, cx)?
                                    .read(cx);
                                let target_entry =
                                    target_worktree.entry_for_path(&path_for_external_paths)?;
                                let highlight_entry_id = this.highlight_entry_for_external_drag(
                                    target_entry,
                                    target_worktree,
                                )?;
                                Some((target_entry.id, highlight_entry_id))
                            }) else {
                                return;
                            };

                            this.drag_target_entry = Some(DragTarget::Entry {
                                entry_id,
                                highlight_entry_id,
                            });
                        },
                    ))
                    .on_drop(cx.listener(
                        move |this, external_paths: &ExternalPaths, window, cx| {
                            this.drag_target_entry = None;
                            this.hover_scroll_task.take();
                            this.drop_external_files(external_paths.paths(), entry_id, window, cx);
                            cx.stop_propagation();
                        },
                    ))
                    .on_drag_move::<DraggedSelection>(cx.listener(
                        move |this, event: &DragMoveEvent<DraggedSelection>, window, cx| {
                            let is_current_target =
                                this.drag_target_entry
                                    .as_ref()
                                    .and_then(|entry| match entry {
                                        DragTarget::Entry {
                                            entry_id: target_id,
                                            ..
                                        } => Some(*target_id),
                                        DragTarget::Background { .. } => None,
                                    })
                                    == Some(entry_id);

                            if !event.bounds.contains(&event.event.position) {
                                // Entry responsible for setting drag target is also responsible to
                                // clear it up after drag is out of bounds
                                if is_current_target {
                                    this.drag_target_entry = None;
                                }
                                return;
                            }

                            if is_current_target {
                                return;
                            }

                            let drag_state = event.drag(cx);

                            if drag_state.items().count() == 1 {
                                this.marked_entries.clear();
                                this.marked_entries.push(drag_state.active_selection);
                            }

                            let Some((entry_id, highlight_entry_id)) = maybe!({
                                let target_worktree = this
                                    .project
                                    .read(cx)
                                    .worktree_for_id(selection.worktree_id, cx)?
                                    .read(cx);
                                let target_entry =
                                    target_worktree.entry_for_path(&path_for_dragged_selection)?;
                                let highlight_entry_id = this.highlight_entry_for_selection_drag(
                                    target_entry,
                                    target_worktree,
                                    drag_state,
                                    cx,
                                )?;
                                Some((target_entry.id, highlight_entry_id))
                            }) else {
                                return;
                            };

                            this.drag_target_entry = Some(DragTarget::Entry {
                                entry_id,
                                highlight_entry_id,
                            });

                            this.hover_expand_task.take();

                            if !kind.is_dir()
                                || this
                                    .state
                                    .expanded_dir_ids
                                    .get(&details.worktree_id)
                                    .is_some_and(|ids| ids.binary_search(&entry_id).is_ok())
                            {
                                return;
                            }

                            let bounds = event.bounds;
                            this.hover_expand_task =
                                Some(cx.spawn_in(window, async move |this, cx| {
                                    cx.background_executor()
                                        .timer(Duration::from_millis(500))
                                        .await;
                                    this.update_in(cx, |this, window, cx| {
                                        this.hover_expand_task.take();
                                        if this.drag_target_entry.as_ref().and_then(|entry| {
                                            match entry {
                                                DragTarget::Entry {
                                                    entry_id: target_id,
                                                    ..
                                                } => Some(*target_id),
                                                DragTarget::Background { .. } => None,
                                            }
                                        }) == Some(entry_id)
                                            && bounds.contains(&window.mouse_position())
                                        {
                                            this.expand_entry(worktree_id, entry_id, cx);
                                            this.update_visible_entries(
                                                Some((worktree_id, entry_id)),
                                                false,
                                                false,
                                                window,
                                                cx,
                                            );
                                            cx.notify();
                                        }
                                    })
                                    .ok();
                                }));
                        },
                    ))
                    .on_drag(dragged_selection, {
                        let active_component =
                            self.state.ancestors.get(&entry_id).and_then(|ancestors| {
                                ancestors.active_component(&details.filename)
                            });
                        move |selection, click_offset, _window, cx| {
                            let filename = active_component
                                .as_ref()
                                .unwrap_or_else(|| &details.filename);
                            cx.new(|_| DraggedProjectEntryView {
                                icon: details.icon.clone(),
                                filename: filename.clone(),
                                click_offset,
                                selection: selection.active_selection,
                                selections: selection.marked_selections.clone(),
                            })
                        }
                    })
                    .on_drop(cx.listener(
                        move |this, selections: &DraggedSelection, window, cx| {
                            this.drag_target_entry = None;
                            this.hover_scroll_task.take();
                            this.hover_expand_task.take();
                            if folded_directory_drag_target.is_some() {
                                return;
                            }
                            this.drag_onto(selections, entry_id, kind.is_file(), window, cx);
                        },
                    ))
                })
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    this.mouse_down = true;
                    cx.propagate();
                }),
            )
            .on_click(
                cx.listener(move |project_panel, event: &gpui::ClickEvent, window, cx| {
                    project_panel.handle_rendered_entry_click(
                        event,
                        entry_id,
                        worktree_id,
                        kind,
                        is_sticky,
                        sticky_index,
                        show_editor,
                        window,
                        cx,
                    );
                }),
            )
            .child(
                ListItem::new(id)
                    .indent_level(depth)
                    .indent_step_size(px(settings.indent_size))
                    .spacing(match settings.entry_spacing {
                        ProjectPanelEntrySpacing::Comfortable => ListItemSpacing::Dense,
                        ProjectPanelEntrySpacing::Standard => ListItemSpacing::ExtraDense,
                    })
                    .selectable(false)
                    .when(
                        canonical_path.is_some()
                            || diagnostic_count.is_some()
                            || git_indicator.is_some(),
                        |this| {
                            this.end_slot::<AnyElement>(Self::render_entry_end_slot(
                                canonical_path,
                                diagnostic_count,
                                git_indicator,
                                kind,
                                filename_text_color,
                                cx,
                            ))
                        },
                    )
                    .child(if let Some(icon) = &icon {
                        if let Some((_, decoration_color)) =
                            entry_diagnostic_aware_icon_decoration_and_color(diagnostic_severity)
                        {
                            let is_warning = diagnostic_severity
                                .map(|severity| matches!(severity, DiagnosticSeverity::WARNING))
                                .unwrap_or(false);
                            div().child(
                                DecoratedIcon::new(
                                    Icon::from_path(icon.clone()).color(Color::Muted),
                                    Some(
                                        IconDecoration::new(
                                            if kind.is_file() {
                                                if is_warning {
                                                    IconDecorationKind::Triangle
                                                } else {
                                                    IconDecorationKind::X
                                                }
                                            } else {
                                                IconDecorationKind::Dot
                                            },
                                            bg_color,
                                            cx,
                                        )
                                        .group_name(Some(GROUP_NAME.into()))
                                        .knockout_hover_color(bg_hover_color)
                                        .color(decoration_color.color(cx))
                                        .position(Point {
                                            x: px(-2.),
                                            y: px(-2.),
                                        }),
                                    ),
                                )
                                .into_any_element(),
                            )
                        } else {
                            h_flex().child(Icon::from_path(icon.to_string()).color(Color::Muted))
                        }
                    } else if let Some((icon_name, color)) =
                        entry_diagnostic_aware_icon_name_and_color(diagnostic_severity)
                    {
                        h_flex()
                            .size(IconSize::default().rems())
                            .child(Icon::new(icon_name).color(color).size(IconSize::Small))
                    } else {
                        h_flex()
                            .size(IconSize::default().rems())
                            .invisible()
                            .flex_none()
                    })
                    .child(if show_editor {
                        h_flex().h_6().w_full().child(self.filename_editor.clone())
                    } else {
                        h_flex()
                            .h_6()
                            .map(|this| match self.state.ancestors.get(&entry_id) {
                                Some(folded_ancestors) => {
                                    this.children(self.render_folder_elements(
                                        folded_ancestors,
                                        entry_id,
                                        file_name,
                                        path_style,
                                        is_sticky,
                                        kind.is_file(),
                                        is_active || is_marked,
                                        settings.drag_and_drop,
                                        settings.bold_folder_labels,
                                        item_colors.drag_over,
                                        folded_directory_drag_target,
                                        filename_text_color,
                                        cx,
                                    ))
                                }

                                None => this.child(
                                    Label::new(file_name)
                                        .single_line()
                                        .color(filename_text_color)
                                        .when(
                                            settings.bold_folder_labels && kind.is_dir(),
                                            |this| this.weight(FontWeight::SEMIBOLD),
                                        )
                                        .into_any_element(),
                                ),
                            })
                    })
                    .on_secondary_mouse_down(cx.listener(
                        move |this, event: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            if !this.marked_entries.contains(&selection) {
                                this.marked_entries.clear();
                            }
                            this.deploy_context_menu(event.position, entry_id, window, cx);
                        },
                    ))
                    .overflow_x(),
            )
            .when_some(validation_color_and_message, |this, (color, message)| {
                this.relative().child(deferred(
                    div()
                        .occlude()
                        .absolute()
                        .top_full()
                        .left(px(-1.)) // Used px over rem so that it doesn't change with font size
                        .right(px(-0.5))
                        .py_1()
                        .px_2()
                        .border_1()
                        .border_color(color)
                        .bg(cx.theme().colors().background)
                        .child(
                            Label::new(message)
                                .color(Color::from(color))
                                .size(LabelSize::Small),
                        ),
                ))
            })
    }
}
