use super::*;

pub fn init(cx: &mut App) {
    renderer_registration::init_renderers(cx);
    let queue = ProjectSettingsUpdateQueue::new(cx);
    cx.set_global(queue);

    cx.on_action(|_: &OpenSettings, cx| {
        open_settings_editor(None, None, None, cx);
    });
    cx.on_action(|_: &mav_actions::assistant::OpenSkillCreator, cx| {
        open_skill_creator(pages::SkillCreatorOpenMode::Form, None, cx);
    });
    cx.on_action(|_: &mav_actions::assistant::CreateSkillFromUrl, cx| {
        let initial_url = pages::skill_url_from_clipboard(cx);
        open_skill_creator(pages::SkillCreatorOpenMode::Url { initial_url }, None, cx);
    });

    cx.observe_new(|workspace: &mut workspace::Workspace, _, _| {
        workspace
            .register_action(|_, action: &OpenSettingsAt, window, cx| {
                let window_handle = window.window_handle().downcast::<MultiWorkspace>();
                open_settings_editor_at_target(
                    Some(&action.path),
                    action.target.as_ref().map(SettingsFileTarget::from),
                    window_handle,
                    cx,
                );
            })
            .register_action(|_, _: &OpenSettings, window, cx| {
                let window_handle = window.window_handle().downcast::<MultiWorkspace>();
                open_settings_editor(None, None, window_handle, cx);
            })
            .register_action(|workspace, _: &OpenProjectSettings, window, cx| {
                let window_handle = window.window_handle().downcast::<MultiWorkspace>();
                let target_worktree_id = workspace
                    .project()
                    .read(cx)
                    .visible_worktrees(cx)
                    .find_map(|tree| {
                        tree.read(cx)
                            .root_entry()?
                            .is_dir()
                            .then_some(tree.read(cx).id())
                    });
                open_settings_editor(None, target_worktree_id, window_handle, cx);
            })
            .register_action(
                |_, _: &mav_actions::assistant::OpenSkillCreator, window, cx| {
                    let window_handle = window.window_handle().downcast::<MultiWorkspace>();
                    open_skill_creator(pages::SkillCreatorOpenMode::Form, window_handle, cx);
                },
            )
            .register_action(
                |_, _: &mav_actions::assistant::CreateSkillFromUrl, window, cx| {
                    let window_handle = window.window_handle().downcast::<MultiWorkspace>();
                    let initial_url = pages::skill_url_from_clipboard(cx);
                    open_skill_creator(
                        pages::SkillCreatorOpenMode::Url { initial_url },
                        window_handle,
                        cx,
                    );
                },
            );
    })
    .detach();
}

pub fn open_settings_editor(
    path: Option<&str>,
    target_worktree_id: Option<WorktreeId>,
    workspace_handle: Option<WindowHandle<MultiWorkspace>>,
    cx: &mut App,
) {
    open_settings_editor_at_target(
        path,
        target_worktree_id.map(SettingsFileTarget::Project),
        workspace_handle,
        cx,
    );
}

fn open_settings_editor_at_target(
    path: Option<&str>,
    target_file: Option<SettingsFileTarget>,
    workspace_handle: Option<WindowHandle<MultiWorkspace>>,
    cx: &mut App,
) {
    fn select_target_file(
        target_file: SettingsFileTarget,
        settings_window: &mut SettingsWindow,
        window: &mut Window,
        cx: &mut Context<SettingsWindow>,
    ) {
        let file_index = settings_window
            .files
            .iter()
            .position(|(file, _)| match target_file {
                SettingsFileTarget::User => matches!(file, SettingsUiFile::User),
                SettingsFileTarget::Project(worktree_id) => file.worktree_id() == Some(worktree_id),
            });
        if let Some(file_index) = file_index {
            settings_window.change_file(file_index, window, cx);
        }
    }

    /// Assumes a settings GUI window is already open
    fn open_path(
        path: &str,
        settings_window: &mut SettingsWindow,
        window: &mut Window,
        cx: &mut Context<SettingsWindow>,
    ) {
        if path.starts_with("languages.$(language)") {
            log::error!("language-specific settings links are not currently supported");
            return;
        }

        let query = format!("#{path}");
        let indices = settings_window.filter_by_json_path(&query);

        settings_window.opening_link = true;
        settings_window.search_bar.update(cx, |editor, cx| {
            editor.set_text(query.clone(), window, cx);
        });
        settings_window.apply_match_indices(indices.iter().copied(), &query);

        if indices.len() == 1
            && let Some(search_index) = settings_window.search_index.as_ref()
        {
            let SearchKeyLUTEntry {
                page_index,
                item_index,
                header_index,
                ..
            } = search_index.key_lut[indices[0]];
            let page = &settings_window.pages[page_index];
            let item = &page.items[item_index];

            if settings_window.filter_table[page_index][item_index]
                && let SettingsPageItem::SubPageLink(link) = item
                && let SettingsPageItem::SectionHeader(header) = page.items[header_index]
            {
                settings_window.push_sub_page(link.clone(), SharedString::from(header), window, cx);
            }
        }

        cx.notify();
    }

    let path = path.map(ToOwned::to_owned);
    open_settings_editor_with(workspace_handle, cx, move |settings_window, window, cx| {
        if let Some(target_file) = target_file {
            select_target_file(target_file, settings_window, window, cx);
        }
        if let Some(path) = path {
            open_path(&path, settings_window, window, cx);
        } else if target_file.is_some() {
            cx.notify();
        }
    });
}

pub fn open_skill_creator(
    open_mode: pages::SkillCreatorOpenMode,
    workspace_handle: Option<WindowHandle<MultiWorkspace>>,
    cx: &mut App,
) {
    open_settings_editor_with(workspace_handle, cx, |settings_window, window, cx| {
        settings_window.navigate_to_skill_creator(open_mode, window, cx);
    });
}

fn open_settings_editor_with(
    workspace_handle: Option<WindowHandle<MultiWorkspace>>,
    cx: &mut App,
    callback: impl FnOnce(&mut SettingsWindow, &mut Window, &mut Context<SettingsWindow>) + 'static,
) {
    telemetry::event!("Settings Viewed");

    let existing_window = cx
        .windows()
        .into_iter()
        .find_map(|window| window.downcast::<SettingsWindow>());

    if let Some(existing_window) = existing_window {
        existing_window
            .update(cx, |settings_window, window, cx| {
                settings_window.original_window = workspace_handle;

                window.activate_window();
                callback(settings_window, window, cx);
            })
            .ok();
        return;
    }

    // We have to defer this to get the workspace off the stack.
    cx.defer(move |cx| {
        let current_rem_size: f32 = theme_settings::ThemeSettings::get_global(cx)
            .ui_font_size(cx)
            .into();

        let default_bounds = DEFAULT_ADDITIONAL_WINDOW_SIZE;
        let default_rem_size = 16.0;
        let scale_factor = current_rem_size / default_rem_size;
        let scaled_bounds: gpui::Size<Pixels> = default_bounds.map(|axis| axis * scale_factor);

        let app_id = ReleaseChannel::global(cx).app_id();
        let window_decorations = match std::env::var("MAV_WINDOW_DECORATIONS") {
            Ok(val) if val == "server" => gpui::WindowDecorations::Server,
            Ok(val) if val == "client" => gpui::WindowDecorations::Client,
            _ => match WorkspaceSettings::get_global(cx).window_decorations {
                settings::WindowDecorations::Server => gpui::WindowDecorations::Server,
                settings::WindowDecorations::Client => gpui::WindowDecorations::Client,
            },
        };

        cx.open_window(
            WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some("Mav — Settings".into()),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(12.0), px(12.0))),
                }),
                focus: true,
                show: true,
                is_movable: true,
                kind: gpui::WindowKind::Normal,
                window_background: cx.theme().window_background_appearance(),
                app_id: Some(app_id.to_owned()),
                window_decorations: Some(window_decorations),
                window_min_size: Some(gpui::Size {
                    // Don't make the settings window thinner than this,
                    // otherwise, it gets unusable. Users with smaller res monitors
                    // can customize the height, but not the width.
                    width: px(900.0),
                    height: px(240.0),
                }),
                window_bounds: Some(WindowBounds::centered(scaled_bounds, cx)),
                ..Default::default()
            },
            |window, cx| {
                let settings_window =
                    cx.new(|cx| SettingsWindow::new(workspace_handle, window, cx));
                settings_window.update(cx, |settings_window, cx| {
                    callback(settings_window, window, cx);
                });

                settings_window
            },
        )
        .log_err();
    });
}
