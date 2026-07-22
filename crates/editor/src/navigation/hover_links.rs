use super::*;

impl Editor {
    pub fn navigate_to_hover_links(
        &mut self,
        kind: Option<GotoDefinitionKind>,
        definitions: Vec<HoverLink>,
        origin: Option<NavigationEntry>,
        split: bool,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) -> Task<Result<Navigated>> {
        // Separate out url and file links, we can only handle one of them at most or an arbitrary number of locations
        let mut first_url_or_file = None;
        let definitions: Vec<_> = definitions
            .into_iter()
            .filter_map(|def| match def {
                HoverLink::Text(link) => Some(Task::ready(anyhow::Ok(Some(link.target)))),
                HoverLink::LspLocation(lsp_location, server_id) => {
                    let computation =
                        self.compute_target_location(lsp_location, server_id, window, cx);
                    Some(cx.background_spawn(computation))
                }
                HoverLink::Url(url) => {
                    first_url_or_file = Some(Either::Left(url));
                    None
                }
                HoverLink::File(file_target) => {
                    first_url_or_file = Some(Either::Right(file_target));
                    None
                }
            })
            .collect();

        let workspace = self.workspace();

        let excerpt_context_lines = multi_buffer::excerpt_context_lines(cx);
        cx.spawn_in(window, async move |editor, cx| {
            let locations: Vec<Location> = future::join_all(definitions)
                .await
                .into_iter()
                .filter_map(|location| location.transpose())
                .collect::<Result<_>>()
                .context("location tasks")?;
            let mut locations = cx.update(|_, cx| {
                locations
                    .into_iter()
                    .map(|location| {
                        let buffer = location.buffer.read(cx);
                        (location.buffer, location.range.to_point(buffer))
                    })
                    .into_group_map()
            })?;
            let mut num_locations = 0;
            for ranges in locations.values_mut() {
                ranges.sort_unstable_by_key(|range| (range.start, Reverse(range.end)));
                ranges.dedup();
                // Merge overlapping or contained ranges. After sorting by
                // (start, Reverse(end)), we can merge in a single pass:
                // if the next range starts before the current one ends,
                // extend the current range's end if needed.
                let mut i = 0;
                while i + 1 < ranges.len() {
                    if ranges[i + 1].start <= ranges[i].end {
                        let merged_end = ranges[i].end.max(ranges[i + 1].end);
                        ranges[i].end = merged_end;
                        ranges.remove(i + 1);
                    } else {
                        i += 1;
                    }
                }
                let fits_in_one_excerpt = ranges
                    .iter()
                    .tuple_windows()
                    .all(|(a, b)| b.start.row - a.end.row <= 2 * excerpt_context_lines);
                num_locations += if fits_in_one_excerpt { 1 } else { ranges.len() };
            }

            if num_locations > 1 {
                let tab_kind = match kind {
                    Some(GotoDefinitionKind::Implementation) => "Implementations",
                    Some(GotoDefinitionKind::Symbol) | None => "Definitions",
                    Some(GotoDefinitionKind::Declaration) => "Declarations",
                    Some(GotoDefinitionKind::Type) => "Types",
                };
                let title = editor
                    .update_in(cx, |_, _, cx| {
                        let target = locations
                            .iter()
                            .flat_map(|(k, v)| iter::repeat(k.clone()).zip(v))
                            .map(|(buffer, location)| {
                                buffer
                                    .read(cx)
                                    .text_for_range(location.clone())
                                    .collect::<String>()
                            })
                            .filter(|text| !text.contains('\n'))
                            .unique()
                            .take(3)
                            .join(", ");
                        if target.is_empty() {
                            tab_kind.to_owned()
                        } else {
                            format!("{tab_kind} for {target}")
                        }
                    })
                    .context("buffer title")?;

                let Some(workspace) = workspace else {
                    return Ok(Navigated::No);
                };

                let opened = workspace
                    .update_in(cx, |workspace, window, cx| {
                        let allow_preview = PreviewTabsSettings::get_global(cx)
                            .enable_preview_multibuffer_from_code_navigation;
                        if let Some((target_editor, target_pane)) =
                            Self::open_locations_in_multibuffer(
                                workspace,
                                locations,
                                title,
                                split,
                                allow_preview,
                                MultibufferSelectionMode::First,
                                window,
                                cx,
                            )
                        {
                            // We create our own nav history instead of using
                            // `target_editor.nav_history` because `nav_history`
                            // seems to be populated asynchronously when an item
                            // is added to a pane
                            let mut nav_history = target_pane
                                .update(cx, |pane, _| pane.nav_history_for_item(&target_editor));
                            target_editor.update(cx, |editor, cx| {
                                let nav_data = editor
                                    .navigation_data(editor.selections.newest_anchor().head(), cx);
                                let target =
                                    Some(nav_history.navigation_entry(Some(
                                        Arc::new(nav_data) as Arc<dyn Any + Send + Sync>
                                    )));
                                nav_history.push_tag(origin, target);
                            })
                        }
                    })
                    .is_ok();

                anyhow::Ok(Navigated::from_bool(opened))
            } else if num_locations == 0 {
                // If there is one url or file, open it directly
                match first_url_or_file {
                    Some(Either::Left(url)) => {
                        cx.update(|window, cx| {
                            if parse_mav_link(&url, cx).is_some() {
                                window.dispatch_action(
                                    Box::new(mav_actions::OpenMavUrl { url: url.into() }),
                                    cx,
                                );
                            } else {
                                cx.open_url(&url);
                            }
                        })?;
                        Ok(Navigated::Yes)
                    }
                    Some(Either::Right(file_target)) => {
                        // TODO(andrew): respect preview tab settings
                        //               `enable_keep_preview_on_code_navigation` and
                        //               `enable_preview_file_from_code_navigation`
                        let Some(workspace) = workspace else {
                            return Ok(Navigated::No);
                        };
                        let item = workspace
                            .update_in(cx, |workspace, window, cx| {
                                workspace.open_resolved_path(
                                    file_target.resolved_path.clone(),
                                    window,
                                    cx,
                                )
                            })?
                            .await?;

                        file_target.navigate_item_to_position(item, cx);

                        Ok(Navigated::Yes)
                    }
                    None => Ok(Navigated::No),
                }
            } else {
                let Some((target_buffer, target_ranges)) = locations.into_iter().next() else {
                    return Ok(Navigated::No);
                };

                editor.update_in(cx, |editor, window, cx| {
                    let target_ranges = target_ranges
                        .into_iter()
                        .map(|r| editor.range_for_match(&r))
                        .map(collapse_multiline_range)
                        .collect::<Vec<_>>();
                    if !split
                        && Some(&target_buffer) == editor.buffer.read(cx).as_singleton().as_ref()
                    {
                        let multibuffer = editor.buffer.read(cx);
                        let target_ranges = target_ranges
                            .into_iter()
                            .filter_map(|r| {
                                let start = multibuffer.buffer_point_to_anchor(
                                    &target_buffer,
                                    r.start,
                                    cx,
                                )?;
                                let end = multibuffer.buffer_point_to_anchor(
                                    &target_buffer,
                                    r.end,
                                    cx,
                                )?;
                                Some(start..end)
                            })
                            .collect::<Vec<_>>();
                        if target_ranges.is_empty() {
                            return Navigated::No;
                        }

                        editor.change_selections(
                            SelectionEffects::scroll(Autoscroll::for_go_to_definition(
                                editor.cursor_top_offset(cx),
                                cx,
                            ))
                            .nav_history(true),
                            window,
                            cx,
                            |s| s.select_anchor_ranges(target_ranges),
                        );

                        let target =
                            editor.navigation_entry(editor.selections.newest_anchor().head(), cx);
                        if let Some(mut nav_history) = editor.nav_history.clone() {
                            nav_history.push_tag(origin, target);
                        }
                    } else {
                        let Some(workspace) = workspace else {
                            return Navigated::No;
                        };
                        let pane = workspace.read(cx).active_pane().clone();
                        let offset = editor.cursor_top_offset(cx);

                        window.defer(cx, move |window, cx| {
                            let (target_editor, target_pane): (Entity<Self>, Entity<Pane>) =
                                workspace.update(cx, |workspace, cx| {
                                    let pane = if split {
                                        workspace.adjacent_pane(window, cx)
                                    } else {
                                        workspace.active_pane().clone()
                                    };

                                    let preview_tabs_settings = PreviewTabsSettings::get_global(cx);
                                    let keep_old_preview = preview_tabs_settings
                                        .enable_keep_preview_on_code_navigation;
                                    let allow_new_preview = preview_tabs_settings
                                        .enable_preview_file_from_code_navigation;

                                    let editor = workspace.open_project_item(
                                        pane.clone(),
                                        target_buffer.clone(),
                                        true,
                                        true,
                                        keep_old_preview,
                                        allow_new_preview,
                                        window,
                                        cx,
                                    );
                                    (editor, pane)
                                });
                            // We create our own nav history instead of using
                            // `target_editor.nav_history` because `nav_history`
                            // seems to be populated asynchronously when an item
                            // is added to a pane
                            let mut nav_history = target_pane
                                .update(cx, |pane, _| pane.nav_history_for_item(&target_editor));
                            target_editor.update(cx, |target_editor, cx| {
                                // When selecting a definition in a different buffer, disable the nav history
                                // to avoid creating a history entry at the previous cursor location.
                                pane.update(cx, |pane, _| pane.disable_history());

                                let multibuffer = target_editor.buffer.read(cx);
                                let Some(target_buffer) = multibuffer.as_singleton() else {
                                    return Navigated::No;
                                };
                                let target_ranges = target_ranges
                                    .into_iter()
                                    .filter_map(|r| {
                                        let start = multibuffer.buffer_point_to_anchor(
                                            &target_buffer,
                                            r.start,
                                            cx,
                                        )?;
                                        let end = multibuffer.buffer_point_to_anchor(
                                            &target_buffer,
                                            r.end,
                                            cx,
                                        )?;
                                        Some(start..end)
                                    })
                                    .collect::<Vec<_>>();
                                if target_ranges.is_empty() {
                                    return Navigated::No;
                                }

                                target_editor.change_selections(
                                    SelectionEffects::scroll(Autoscroll::for_go_to_definition(
                                        offset, cx,
                                    ))
                                    .nav_history(true),
                                    window,
                                    cx,
                                    |s| s.select_anchor_ranges(target_ranges),
                                );

                                let nav_data = target_editor.navigation_data(
                                    target_editor.selections.newest_anchor().head(),
                                    cx,
                                );
                                let target =
                                    Some(nav_history.navigation_entry(Some(
                                        Arc::new(nav_data) as Arc<dyn Any + Send + Sync>
                                    )));
                                nav_history.push_tag(origin, target);
                                pane.update(cx, |pane, _| pane.enable_history());
                                Navigated::Yes
                            });
                        });
                    }
                    Navigated::Yes
                })
            }
        })
    }
}
