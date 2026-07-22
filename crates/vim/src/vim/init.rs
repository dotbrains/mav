use super::*;

pub fn init(cx: &mut App) {
    VimGlobals::register(cx);

    cx.observe_new(Vim::register).detach();

    cx.observe_new(|workspace: &mut Workspace, _, _| {
        workspace.register_action(|workspace, _: &ToggleVimMode, _, cx| {
            let fs = workspace.app_state().fs.clone();
            let currently_enabled = VimModeSetting::get_global(cx).0;
            update_settings_file(fs, cx, move |setting, _| {
                setting.vim_mode = Some(!currently_enabled);
                if let Some(helix_mode) = &mut setting.helix_mode {
                    *helix_mode = false;
                }
            })
        });

        workspace.register_action(|workspace, _: &ToggleHelixMode, _, cx| {
            let fs = workspace.app_state().fs.clone();
            let currently_enabled = HelixModeSetting::get_global(cx).0;
            update_settings_file(fs, cx, move |setting, _| {
                setting.helix_mode = Some(!currently_enabled);
                if let Some(vim_mode) = &mut setting.vim_mode {
                    *vim_mode = false;
                }
            })
        });

        workspace.register_action(|_, _: &MenuSelectNext, window, cx| {
            let count = Vim::take_count(cx).unwrap_or(1);

            for _ in 0..count {
                window.dispatch_action(menu::SelectNext.boxed_clone(), cx);
            }
        });

        workspace.register_action(|_, _: &MenuSelectPrevious, window, cx| {
            let count = Vim::take_count(cx).unwrap_or(1);

            for _ in 0..count {
                window.dispatch_action(menu::SelectPrevious.boxed_clone(), cx);
            }
        });

        workspace.register_action(|_, _: &ToggleProjectPanelFocus, window, cx| {
            if Vim::take_count(cx).is_none() {
                window.dispatch_action(mav_actions::project_panel::ToggleFocus.boxed_clone(), cx);
            }
        });

        workspace.register_action(|workspace, n: &Number, window, cx| {
            let vim = workspace
                .focused_pane(window, cx)
                .read(cx)
                .active_item()
                .and_then(|item| item.act_as::<Editor>(cx))
                .and_then(|editor| editor.read(cx).addon::<VimAddon>().cloned());
            if let Some(vim) = vim {
                let digit = n.0;
                vim.entity.update(cx, |_, cx| {
                    cx.defer_in(window, move |vim, window, cx| {
                        vim.push_count_digit(digit, window, cx)
                    })
                });
            } else {
                let count = Vim::globals(cx).pre_count.unwrap_or(0);
                Vim::globals(cx).pre_count = Some(
                    count
                        .checked_mul(10)
                        .and_then(|c| c.checked_add(n.0))
                        .unwrap_or(count),
                );
            };
        });

        workspace.register_action(|_, _: &mav_actions::vim::OpenDefaultKeymap, _, cx| {
            cx.emit(workspace::Event::OpenBundledFile {
                text: settings::vim_keymap(),
                title: "Default Vim Bindings",
                language: "JSON",
            });
        });

        workspace.register_action(|workspace, _: &ResetPaneSizes, window, cx| {
            workspace.reset_pane_sizes(window, cx);
        });

        workspace.register_action(|workspace, _: &MaximizePane, window, cx| {
            let pane = workspace.active_pane();
            let Some(size) = workspace.bounding_box_for_pane(pane) else {
                return;
            };

            let theme = ThemeSettings::get_global(cx);
            let height = theme.buffer_font_size(cx) * theme.buffer_line_height.value();

            let desired_size = if let Some(count) = Vim::take_count(cx) {
                height * count
            } else {
                px(10000.)
            };
            workspace.resize_pane(Axis::Vertical, desired_size - size.size.height, window, cx)
        });

        workspace.register_action(|workspace, _: &ResizePaneRight, window, cx| {
            let count = Vim::take_count(cx).unwrap_or(1) as f32;
            Vim::take_forced_motion(cx);
            let theme = ThemeSettings::get_global(cx);
            let font_id = window.text_system().resolve_font(&theme.buffer_font);
            let Ok(width) = window
                .text_system()
                .advance(font_id, theme.buffer_font_size(cx), 'm')
            else {
                return;
            };
            workspace.resize_pane(Axis::Horizontal, width.width * count, window, cx);
        });

        workspace.register_action(|workspace, _: &ResizePaneLeft, window, cx| {
            let count = Vim::take_count(cx).unwrap_or(1) as f32;
            Vim::take_forced_motion(cx);
            let theme = ThemeSettings::get_global(cx);
            let font_id = window.text_system().resolve_font(&theme.buffer_font);
            let Ok(width) = window
                .text_system()
                .advance(font_id, theme.buffer_font_size(cx), 'm')
            else {
                return;
            };
            workspace.resize_pane(Axis::Horizontal, -width.width * count, window, cx);
        });

        workspace.register_action(|workspace, _: &ResizePaneUp, window, cx| {
            let count = Vim::take_count(cx).unwrap_or(1) as f32;
            Vim::take_forced_motion(cx);
            let theme = ThemeSettings::get_global(cx);
            let height = theme.buffer_font_size(cx) * theme.buffer_line_height.value();
            workspace.resize_pane(Axis::Vertical, height * count, window, cx);
        });

        workspace.register_action(|workspace, _: &ResizePaneDown, window, cx| {
            let count = Vim::take_count(cx).unwrap_or(1) as f32;
            Vim::take_forced_motion(cx);
            let theme = ThemeSettings::get_global(cx);
            let height = theme.buffer_font_size(cx) * theme.buffer_line_height.value();
            workspace.resize_pane(Axis::Vertical, -height * count, window, cx);
        });

        workspace.register_action(|workspace, _: &SearchSubmit, window, cx| {
            let vim = workspace
                .focused_pane(window, cx)
                .read(cx)
                .active_item()
                .and_then(|item| item.act_as::<Editor>(cx))
                .and_then(|editor| editor.read(cx).addon::<VimAddon>().cloned());
            let Some(vim) = vim else { return };
            vim.entity.update(cx, |vim, cx| {
                if !vim.search.cmd_f_search {
                    cx.defer_in(window, |vim, window, cx| vim.search_submit(window, cx))
                } else {
                    cx.propagate()
                }
            })
        });
        workspace.register_action(|_, _: &GoToTab, window, cx| {
            let count = Vim::take_count(cx);
            Vim::take_forced_motion(cx);

            if let Some(tab_index) = count {
                // <count>gt goes to tab <count> (1-based).
                let zero_based_index = tab_index.saturating_sub(1);
                window.dispatch_action(
                    workspace::pane::ActivateItem(zero_based_index).boxed_clone(),
                    cx,
                );
            } else {
                // If no count is provided, go to the next tab.
                window.dispatch_action(
                    workspace::pane::ActivateNextItem::default().boxed_clone(),
                    cx,
                );
            }
        });

        workspace.register_action(|workspace, _: &GoToPreviousTab, window, cx| {
            let count = Vim::take_count(cx);
            Vim::take_forced_motion(cx);

            if let Some(count) = count {
                // gT with count goes back that many tabs with wraparound (not the same as gt!).
                let pane = workspace.active_pane().read(cx);
                let item_count = pane.items().count();
                if item_count > 0 {
                    let current_index = pane.active_item_index();
                    let target_index = (current_index as isize - count as isize)
                        .rem_euclid(item_count as isize)
                        as usize;
                    window.dispatch_action(
                        workspace::pane::ActivateItem(target_index).boxed_clone(),
                        cx,
                    );
                }
            } else {
                // No count provided, go to the previous tab.
                window.dispatch_action(
                    workspace::pane::ActivatePreviousItem::default().boxed_clone(),
                    cx,
                );
            }
        });
    })
    .detach();
}

#[derive(Clone)]
pub(crate) struct VimAddon {
    pub(crate) entity: Entity<Vim>,
}
