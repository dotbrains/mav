use super::*;

impl ProjectPanel {
    pub(super) fn render_folder_elements(
        &self,
        folded_ancestors: &FoldedAncestors,
        entry_id: ProjectEntryId,
        file_name: String,
        path_style: PathStyle,
        is_sticky: bool,
        is_file: bool,
        is_active_or_marked: bool,
        drag_and_drop_enabled: bool,
        bold_folder_labels: bool,
        drag_over_color: Hsla,
        folded_directory_drag_target: Option<FoldedDirectoryDragTarget>,
        filename_text_color: Color,
        cx: &Context<Self>,
    ) -> impl Iterator<Item = AnyElement> {
        let components = Path::new(&file_name)
            .components()
            .map(|comp| comp.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        let active_index = folded_ancestors.active_index();
        let components_len = components.len();
        let delimiter = SharedString::new(path_style.primary_separator());

        let path_component_elements =
            components
                .into_iter()
                .enumerate()
                .map(move |(index, component)| {
                    div()
                        .id(SharedString::from(format!(
                            "project_panel_path_component_{}_{index}",
                            entry_id.to_usize()
                        )))
                        .when(index == 0, |this| this.ml_neg_0p5())
                        .px_0p5()
                        .rounded_xs()
                        .hover(|style| style.bg(cx.theme().colors().element_active))
                        .when(!is_sticky, |div| {
                            div.when(index != components_len - 1, |div| {
                                let target_entry_id = folded_ancestors
                                    .ancestors
                                    .get(components_len - 1 - index)
                                    .cloned();
                                div.when(drag_and_drop_enabled, |div| {
                                    div.on_drag_move(cx.listener(
                                        move |this,
                                              event: &DragMoveEvent<DraggedSelection>,
                                              _,
                                              _| {
                                            if event.bounds.contains(&event.event.position) {
                                                this.folded_directory_drag_target =
                                                    Some(FoldedDirectoryDragTarget {
                                                        entry_id,
                                                        index,
                                                        is_delimiter_target: false,
                                                    });
                                            } else {
                                                let is_current_target = this
                                                    .folded_directory_drag_target
                                                    .as_ref()
                                                    .is_some_and(|target| {
                                                        target.entry_id == entry_id
                                                            && target.index == index
                                                            && !target.is_delimiter_target
                                                    });
                                                if is_current_target {
                                                    this.folded_directory_drag_target = None;
                                                }
                                            }
                                        },
                                    ))
                                    .on_drop(cx.listener(
                                        move |this, selections: &DraggedSelection, window, cx| {
                                            this.hover_scroll_task.take();
                                            this.drag_target_entry = None;
                                            this.folded_directory_drag_target = None;
                                            if let Some(target_entry_id) = target_entry_id {
                                                this.drag_onto(
                                                    selections,
                                                    target_entry_id,
                                                    is_file,
                                                    window,
                                                    cx,
                                                );
                                            }
                                        },
                                    ))
                                    .when(
                                        folded_directory_drag_target.is_some_and(|target| {
                                            target.entry_id == entry_id && target.index == index
                                        }),
                                        |this| this.bg(drag_over_color),
                                    )
                                })
                            })
                        })
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, _, cx| {
                                if let Some(folds) = this.state.ancestors.get_mut(&entry_id) {
                                    if folds.set_active_index(index) {
                                        cx.notify();
                                    }
                                }
                            }),
                        )
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |this, _, _, cx| {
                                if let Some(folds) = this.state.ancestors.get_mut(&entry_id) {
                                    if folds.set_active_index(index) {
                                        cx.notify();
                                    }
                                }
                            }),
                        )
                        .child(
                            Label::new(component)
                                .single_line()
                                .color(filename_text_color)
                                .when(bold_folder_labels && !is_file, |this| {
                                    this.weight(FontWeight::SEMIBOLD)
                                })
                                .when(index == active_index && is_active_or_marked, |this| {
                                    this.underline()
                                }),
                        )
                        .into_any()
                });

        let mut separator_index = 0;
        itertools::intersperse_with(path_component_elements, move || {
            separator_index += 1;
            self.render_entry_path_separator(
                entry_id,
                separator_index,
                components_len,
                is_sticky,
                is_file,
                drag_and_drop_enabled,
                filename_text_color,
                &delimiter,
                folded_ancestors,
                cx,
            )
            .into_any()
        })
    }
    pub(super) fn render_entry_path_separator(
        &self,
        entry_id: ProjectEntryId,
        index: usize,
        components_len: usize,
        is_sticky: bool,
        is_file: bool,
        drag_and_drop_enabled: bool,
        filename_text_color: Color,
        delimiter: &SharedString,
        folded_ancestors: &FoldedAncestors,
        cx: &Context<Self>,
    ) -> Div {
        let delimiter_target_index = index - 1;
        let target_entry_id = folded_ancestors
            .ancestors
            .get(components_len - 1 - delimiter_target_index)
            .cloned();
        div()
            .when(!is_sticky, |div| {
                div.when(drag_and_drop_enabled, |div| {
                    div.on_drop(cx.listener(
                        move |this, selections: &DraggedSelection, window, cx| {
                            this.hover_scroll_task.take();
                            this.drag_target_entry = None;
                            this.folded_directory_drag_target = None;
                            if let Some(target_entry_id) = target_entry_id {
                                this.drag_onto(selections, target_entry_id, is_file, window, cx);
                            }
                        },
                    ))
                    .on_drag_move(cx.listener(
                        move |this, event: &DragMoveEvent<DraggedSelection>, _, _| {
                            if event.bounds.contains(&event.event.position) {
                                this.folded_directory_drag_target =
                                    Some(FoldedDirectoryDragTarget {
                                        entry_id,
                                        index: delimiter_target_index,
                                        is_delimiter_target: true,
                                    });
                            } else {
                                let is_current_target =
                                    this.folded_directory_drag_target.is_some_and(|target| {
                                        target.entry_id == entry_id
                                            && target.index == delimiter_target_index
                                            && target.is_delimiter_target
                                    });
                                if is_current_target {
                                    this.folded_directory_drag_target = None;
                                }
                            }
                        },
                    ))
                })
            })
            .child(
                Label::new(delimiter.clone())
                    .single_line()
                    .color(filename_text_color),
            )
    }
}
