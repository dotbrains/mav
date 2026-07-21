use super::*;

pub(crate) fn render_buffer_header(
    editor: &Entity<Editor>,
    for_excerpt: &ExcerptBoundaryInfo,
    is_folded: bool,
    is_selected: bool,
    is_sticky: bool,
    jump_data: JumpData,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let editor_read = editor.read(cx);
    let multi_buffer = editor_read.buffer.read(cx);
    let is_read_only = editor_read.read_only(cx);
    let editor_handle: &dyn ItemHandle = editor;
    let multibuffer_snapshot = multi_buffer.snapshot(cx);
    let buffer = for_excerpt.buffer(&multibuffer_snapshot);

    let breadcrumbs = if is_selected {
        editor_read.breadcrumbs_inner(cx)
    } else {
        None
    };

    let buffer_id = for_excerpt.buffer_id();
    let file_status = multi_buffer
        .all_diff_hunks_expanded()
        .then(|| editor_read.status_for_buffer_id(buffer_id, cx))
        .flatten();
    let indicator = multi_buffer.buffer(buffer_id).and_then(|buffer| {
        let buffer = buffer.read(cx);
        let indicator_color = match (buffer.has_conflict(), buffer.is_dirty()) {
            (true, _) => Some(Color::Warning),
            (_, true) => Some(Color::Accent),
            (false, false) => None,
        };
        indicator_color.map(|indicator_color| Indicator::dot().color(indicator_color))
    });

    let include_root = editor_read
        .project
        .as_ref()
        .map(|project| project.read(cx).visible_worktrees(cx).count() > 1)
        .unwrap_or_default();
    let file = buffer.file();
    let can_open_excerpts = file.is_none_or(|file| file.can_open());
    let path_style = file.map(|file| file.path_style(cx));
    let relative_path = buffer.resolve_file_path(include_root, cx);
    let (parent_path, filename) = if let Some(path) = &relative_path {
        if let Some(path_style) = path_style {
            let (dir, file_name) = path_style.split(path);
            (dir.map(|dir| dir.to_owned()), Some(file_name.to_owned()))
        } else {
            (None, Some(path.clone()))
        }
    } else {
        (None, None)
    };
    let focus_handle = editor_read.focus_handle(cx);
    let colors = cx.theme().colors();
    let opaque_window =
        cx.theme().window_background_appearance() == WindowBackgroundAppearance::Opaque;

    let header = div()
        .id(("buffer-header", buffer_id.to_proto()))
        .p(BUFFER_HEADER_PADDING)
        .w_full()
        .h(FILE_HEADER_HEIGHT as f32 * window.line_height())
        .child(
            h_flex()
                .group("buffer-header-group")
                .size_full()
                .flex_basis(Length::Definite(DefiniteLength::Fraction(0.667)))
                .pl_1()
                .pr_2()
                .rounded_sm()
                .gap_1p5()
                .when(is_sticky && opaque_window, |el| el.shadow_md())
                .border_1()
                .map(|border| {
                    let border_color =
                        if is_selected && is_folded && focus_handle.contains_focused(window, cx) {
                            colors.border_focused
                        } else {
                            colors.border
                        };
                    border.border_color(border_color)
                })
                .when(opaque_window, |el| {
                    el.bg(colors.editor_subheader_background)
                })
                .hover(|style| style.bg(colors.element_hover))
                .map(|header| {
                    let editor = editor.clone();
                    let buffer_id = for_excerpt.buffer_id();
                    let toggle_chevron_icon =
                        FileIcons::get_chevron_icon(!is_folded, cx).map(Icon::from_path);
                    let button_size = rems_from_px(28.);

                    header.child(
                        div()
                            .hover(|style| style.bg(colors.element_selected))
                            .rounded_xs()
                            .child(
                                ButtonLike::new("toggle-buffer-fold")
                                    .style(ButtonStyle::Transparent)
                                    .height(button_size.into())
                                    .width(button_size)
                                    .children(toggle_chevron_icon)
                                    .tooltip({
                                        let focus_handle = focus_handle.clone();
                                        let is_folded_for_tooltip = is_folded;
                                        move |_window, cx| {
                                            Tooltip::with_meta_in(
                                                if is_folded_for_tooltip {
                                                    "Unfold Excerpt"
                                                } else {
                                                    "Fold Excerpt"
                                                },
                                                Some(&ToggleFold),
                                                format!(
                                                    "{} to toggle all",
                                                    text_for_keystroke(
                                                        &Modifiers::alt(),
                                                        "click",
                                                        cx,
                                                    )
                                                ),
                                                &focus_handle,
                                                cx,
                                            )
                                        }
                                    })
                                    .on_click(move |event, window, cx| {
                                        if event.modifiers().alt {
                                            editor.update(cx, |editor, cx| {
                                                editor.toggle_fold_all(&ToggleFoldAll, window, cx);
                                            });
                                        } else if is_folded {
                                            editor.update(cx, |editor, cx| {
                                                editor.unfold_buffer(buffer_id, cx);
                                            });
                                        } else {
                                            editor.update(cx, |editor, cx| {
                                                editor.fold_buffer(buffer_id, cx);
                                            });
                                        }
                                    }),
                            ),
                    )
                })
                .children(
                    editor_read
                        .addons
                        .values()
                        .filter_map(|addon| {
                            addon.render_buffer_header_controls(for_excerpt, buffer, window, cx)
                        })
                        .take(1),
                )
                .when(!is_read_only, |this| {
                    this.child(
                        h_flex()
                            .size_3()
                            .justify_center()
                            .flex_shrink_0()
                            .children(indicator),
                    )
                })
                .child(
                    h_flex()
                        .cursor_pointer()
                        .id("path_header_block")
                        .min_w_0()
                        .size_full()
                        .gap_1()
                        .justify_between()
                        .overflow_hidden()
                        .child(h_flex().min_w_0().flex_1().gap_0p5().overflow_hidden().map(
                            |path_header| {
                                let filename = filename
                                    .map(SharedString::from)
                                    .unwrap_or_else(|| "untitled".into());

                                let full_path = match parent_path.as_deref() {
                                    Some(parent) if !parent.is_empty() => {
                                        format!("{}{}", parent, filename.as_str())
                                    }
                                    _ => filename.as_str().to_string(),
                                };

                                path_header
                                    .child(
                                        ButtonLike::new("filename-button")
                                            .when(ItemSettings::get_global(cx).file_icons, |this| {
                                                let path = std::path::Path::new(filename.as_str());
                                                let icon = FileIcons::get_icon(path, cx)
                                                    .unwrap_or_default();

                                                this.child(
                                                    Icon::from_path(icon).color(Color::Muted),
                                                )
                                            })
                                            .child(
                                                Label::new(filename)
                                                    .single_line()
                                                    .color(file_status_label_color(file_status))
                                                    .buffer_font(cx)
                                                    .when(
                                                        file_status.is_some_and(|s| s.is_deleted()),
                                                        |label| label.strikethrough(),
                                                    ),
                                            )
                                            .tooltip(move |_, cx| {
                                                Tooltip::with_meta(
                                                    "Open File",
                                                    None,
                                                    full_path.clone(),
                                                    cx,
                                                )
                                            })
                                            .on_click(window.listener_for(editor, {
                                                let jump_data = jump_data.clone();
                                                move |editor, e: &ClickEvent, window, cx| {
                                                    editor.open_excerpts_common(
                                                        Some(jump_data.clone()),
                                                        e.modifiers().secondary(),
                                                        window,
                                                        cx,
                                                    );
                                                }
                                            })),
                                    )
                                    .when_some(parent_path, |then, path| {
                                        then.child(
                                            Label::new(path)
                                                .buffer_font(cx)
                                                .truncate_start()
                                                .color(
                                                    if file_status
                                                        .is_some_and(FileStatus::is_deleted)
                                                    {
                                                        Color::Custom(colors.text_disabled)
                                                    } else {
                                                        Color::Custom(colors.text_muted)
                                                    },
                                                ),
                                        )
                                    })
                                    .when(!buffer.capability.editable(), |el| {
                                        el.child(Icon::new(IconName::FileLock).color(Color::Muted))
                                    })
                                    .when_some(breadcrumbs, |then, breadcrumbs| {
                                        let font = theme_settings::ThemeSettings::get_global(cx)
                                            .buffer_font
                                            .clone();
                                        then.child(render_breadcrumb_text(
                                            breadcrumbs,
                                            Some(font),
                                            None,
                                            editor_handle,
                                            true,
                                            window,
                                            cx,
                                        ))
                                    })
                            },
                        ))
                        .when(can_open_excerpts && relative_path.is_some(), |this| {
                            this.child(
                                div()
                                    .when(!is_selected, |this| {
                                        this.visible_on_hover("buffer-header-group")
                                    })
                                    .child(
                                        Button::new("open-file-button", "Open File")
                                            .style(ButtonStyle::OutlinedGhost)
                                            .when(is_selected, |this| {
                                                this.key_binding(KeyBinding::for_action_in(
                                                    &OpenExcerpts,
                                                    &focus_handle,
                                                    cx,
                                                ))
                                            })
                                            .on_click(window.listener_for(editor, {
                                                let jump_data = jump_data.clone();
                                                move |editor, e: &ClickEvent, window, cx| {
                                                    editor.open_excerpts_common(
                                                        Some(jump_data.clone()),
                                                        e.modifiers().secondary(),
                                                        window,
                                                        cx,
                                                    );
                                                }
                                            })),
                                    ),
                            )
                        })
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(window.listener_for(editor, {
                            let buffer_id = for_excerpt.buffer_id();
                            move |editor, e: &ClickEvent, window, cx| {
                                if e.modifiers().alt {
                                    editor.open_excerpts_common(
                                        Some(jump_data.clone()),
                                        e.modifiers().secondary(),
                                        window,
                                        cx,
                                    );
                                    return;
                                }

                                if is_folded {
                                    editor.unfold_buffer(buffer_id, cx);
                                } else {
                                    editor.fold_buffer(buffer_id, cx);
                                }
                            }
                        })),
                ),
        );

    let file = buffer.file().cloned();
    let editor = editor.clone();
    let buffer_snapshot = buffer.clone();

    right_click_menu("buffer-header-context-menu")
        .trigger(move |_, _, _| header)
        .menu(move |window, cx| {
            let menu_context = focus_handle.clone();
            let editor = editor.clone();
            let file = file.clone();
            let buffer_snapshot = buffer_snapshot.clone();
            ContextMenu::build(window, cx, move |mut menu, window, cx| {
                if let Some(file) = file
                    && let Some(project) = editor.read(cx).project()
                    && let Some(worktree) =
                        project.read(cx).worktree_for_id(file.worktree_id(cx), cx)
                {
                    let path_style = file.path_style(cx);
                    let worktree = worktree.read(cx);
                    let relative_path = file.path();
                    let entry_for_path = worktree.entry_for_path(relative_path);
                    let abs_path = entry_for_path.map(|e| {
                        e.canonical_path
                            .as_deref()
                            .map_or_else(|| worktree.absolutize(relative_path), Path::to_path_buf)
                    });
                    let has_relative_path = worktree.root_entry().is_some_and(Entry::is_dir);

                    let parent_abs_path = abs_path
                        .as_ref()
                        .and_then(|abs_path| Some(abs_path.parent()?.to_path_buf()));
                    let relative_path = has_relative_path
                        .then_some(relative_path)
                        .map(ToOwned::to_owned);

                    let visible_in_project_panel = relative_path.is_some() && worktree.is_visible();
                    let reveal_in_project_panel = entry_for_path
                        .filter(|_| visible_in_project_panel)
                        .map(|entry| entry.id);
                    menu = menu
                        .when_some(abs_path, |menu, abs_path| {
                            menu.entry(
                                "Copy Path",
                                Some(Box::new(mav_actions::workspace::CopyPath)),
                                window.handler_for(&editor, move |_, _, cx| {
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
                                window.handler_for(&editor, move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        relative_path.display(path_style).to_string(),
                                    ));
                                }),
                            )
                        })
                        .when(
                            reveal_in_project_panel.is_some() || parent_abs_path.is_some(),
                            |menu| menu.separator(),
                        )
                        .when_some(reveal_in_project_panel, |menu, entry_id| {
                            menu.entry(
                                "Reveal In Project Panel",
                                Some(Box::new(RevealInProjectPanel::default())),
                                window.handler_for(&editor, move |editor, _, cx| {
                                    if let Some(project) = &mut editor.project {
                                        project.update(cx, |_, cx| {
                                            cx.emit(project::Event::RevealInProjectPanel(entry_id))
                                        });
                                    }
                                }),
                            )
                        })
                        .when_some(parent_abs_path, |menu, parent_abs_path| {
                            menu.entry(
                                "Open in Terminal",
                                Some(Box::new(OpenInTerminal)),
                                window.handler_for(&editor, move |_, window, cx| {
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
                }

                menu = editor.update(cx, |editor, cx| {
                    let mut menu = menu;
                    for addon in editor.addons.values() {
                        menu = addon.extend_buffer_header_context_menu(
                            menu,
                            &buffer_snapshot,
                            window,
                            cx,
                        );
                    }
                    menu
                });

                menu.context(menu_context)
            })
        })
}

pub(crate) fn file_status_label_color(file_status: Option<FileStatus>) -> Color {
    file_status.map_or(Color::Default, |status| {
        if status.is_conflicted() {
            Color::Conflict
        } else if status.is_modified() {
            Color::Modified
        } else if status.is_deleted() {
            Color::Disabled
        } else if status.is_created() {
            Color::Created
        } else {
            Color::Default
        }
    })
}
