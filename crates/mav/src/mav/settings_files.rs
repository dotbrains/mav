use super::*;

fn notify_settings_errors(result: settings::SettingsParseResult, is_user: bool, cx: &mut App) {
    if let settings::ParseStatus::Failed { error: err } = &result.parse_status {
        let settings_type = if is_user { "user" } else { "global" };
        log::error!("Failed to load {} settings: {err}", settings_type);
    }

    let error = match result.parse_status {
        settings::ParseStatus::Failed { error } => Some(anyhow::format_err!(error)),
        settings::ParseStatus::Success => None,
        settings::ParseStatus::Unchanged => return,
    };
    let id = NotificationId::Named(format!("failed-to-parse-settings-{is_user}").into());

    let showed_parse_error = match error {
        Some(error) => {
            if let Some(InvalidSettingsError::LocalSettings { .. }) =
                error.downcast_ref::<InvalidSettingsError>()
            {
                false
                // Local settings errors are displayed by the projects
            } else {
                show_app_notification(id, cx, move |cx| {
                    cx.new(|cx| {
                        MessageNotification::new(format!("Invalid user settings file\n{error}"), cx)
                            .primary_message("Open Settings File")
                            .primary_icon(IconName::Settings)
                            .primary_on_click(|window, cx| {
                                window.dispatch_action(
                                    mav_actions::OpenSettingsFile.boxed_clone(),
                                    cx,
                                );
                                cx.emit(DismissEvent);
                            })
                    })
                });
                true
            }
        }
        None => {
            dismiss_app_notification(&id, cx);
            false
        }
    };
    let id = NotificationId::Named(format!("failed-to-migrate-settings-{is_user}").into());

    match result.migration_status {
        settings::MigrationStatus::Succeeded | settings::MigrationStatus::NotNeeded => {
            dismiss_app_notification(&id, cx);
        }
        settings::MigrationStatus::Failed { error: err } => {
            if !showed_parse_error {
                show_app_notification(id, cx, move |cx| {
                    cx.new(|cx| {
                        MessageNotification::new(
                            format!(
                                "Failed to migrate settings\n\
                                {err}"
                            ),
                            cx,
                        )
                        .primary_message("Open Settings File")
                        .primary_icon(IconName::Settings)
                        .primary_on_click(|window, cx| {
                            window.dispatch_action(mav_actions::OpenSettingsFile.boxed_clone(), cx);
                            cx.emit(DismissEvent);
                        })
                    })
                });
            }
        }
    };
}

/// Starts watching `~/.config/mav/AGENTS.md` (or the platform equivalent) and
/// surfaces any read errors using the same notification UI as settings errors.
///
/// The file itself is loaded into [`agent_settings::UserAgentsMd`] for inclusion
/// in prompts.
pub fn watch_user_agents_md(fs: Arc<dyn fs::Fs>, cx: &mut App) {
    struct UserAgentsMdParseError;
    let notification_id = NotificationId::unique::<UserAgentsMdParseError>();

    init_user_agents_md(fs, cx, move |state, cx| match state {
        UserAgentsMdState::Loaded(_) | UserAgentsMdState::Empty => {
            dismiss_app_notification(&notification_id, cx);
        }
        UserAgentsMdState::Error(message) => {
            let path = paths::agents_file().display().to_string();
            log::error!("Failed to load user AGENTS.md from {path}: {message}");
            let body = format!("Failed to load {path}\n{message}");
            let notification_id = notification_id.clone();
            show_app_notification(notification_id, cx, move |cx| {
                let body = body.clone();
                cx.new(|cx| MessageNotification::new(body, cx))
            });
        }
    });
}

pub fn watch_settings_files(fs: Arc<dyn fs::Fs>, cx: &mut App) {
    MigrationNotification::set_global(cx.new(|_| MigrationNotification), cx);

    SettingsStore::update_global(cx, move |store, cx| {
        store.watch_settings_files(fs, cx, |settings_file, result, cx| {
            let is_user = matches!(settings_file, SettingsFile::User);
            let migrating_in_memory =
                matches!(&result.migration_status, MigrationStatus::Succeeded);
            notify_settings_errors(result, is_user, cx);
            if let Some(notifier) = MigrationNotification::try_global(cx) {
                notifier.update(cx, |_, cx| {
                    cx.emit(MigrationEvent::ContentChanged {
                        migration_type: MigrationType::Settings,
                        migrating_in_memory,
                    });
                });
            }
        });
    });
}

pub fn handle_keymap_file_changes(
    mut user_keymap_file_rx: mpsc::UnboundedReceiver<String>,
    user_keymap_watcher: gpui::Task<()>,
    cx: &mut App,
) {
    let (base_keymap_tx, mut base_keymap_rx) = mpsc::unbounded();
    let (keyboard_layout_tx, mut keyboard_layout_rx) = mpsc::unbounded();
    let mut old_base_keymap = *BaseKeymap::get_global(cx);
    let mut old_vim_enabled = VimModeSetting::get_global(cx).0;
    let mut old_helix_enabled = vim_mode_setting::HelixModeSetting::get_global(cx).0;

    cx.observe_global::<SettingsStore>(move |cx| {
        let new_base_keymap = *BaseKeymap::get_global(cx);
        let new_vim_enabled = VimModeSetting::get_global(cx).0;
        let new_helix_enabled = vim_mode_setting::HelixModeSetting::get_global(cx).0;

        if new_base_keymap != old_base_keymap
            || new_vim_enabled != old_vim_enabled
            || new_helix_enabled != old_helix_enabled
        {
            old_base_keymap = new_base_keymap;
            old_vim_enabled = new_vim_enabled;
            old_helix_enabled = new_helix_enabled;

            base_keymap_tx.unbounded_send(()).unwrap();
        }
    })
    .detach();

    #[cfg(target_os = "windows")]
    {
        let mut current_layout_id = cx.keyboard_layout().id().to_string();
        cx.on_keyboard_layout_change(move |cx| {
            let next_layout_id = cx.keyboard_layout().id();
            if next_layout_id != current_layout_id {
                current_layout_id = next_layout_id.to_string();
                keyboard_layout_tx.unbounded_send(()).ok();
            }
        })
        .detach();
    }

    #[cfg(not(target_os = "windows"))]
    {
        let mut current_mapping = cx.keyboard_mapper().get_key_equivalents().cloned();
        cx.on_keyboard_layout_change(move |cx| {
            let next_mapping = cx.keyboard_mapper().get_key_equivalents();
            if current_mapping.as_ref() != next_mapping {
                current_mapping = next_mapping.cloned();
                keyboard_layout_tx.unbounded_send(()).ok();
            }
        })
        .detach();
    }

    load_default_keymap(cx);

    struct KeymapParseErrorNotification;
    let notification_id = NotificationId::unique::<KeymapParseErrorNotification>();

    cx.spawn(async move |cx| {
        let _user_keymap_watcher = user_keymap_watcher;
        let mut user_keymap_content = String::new();
        let mut migrating_in_memory = false;
        loop {
            select_biased! {
                _ = base_keymap_rx.next() => {},
                _ = keyboard_layout_rx.next() => {},
                content = user_keymap_file_rx.next() => {
                    if let Some(content) = content {
                        if let Ok(Some(migrated_content)) = migrate_keymap(&content) {
                            user_keymap_content = migrated_content;
                            migrating_in_memory = true;
                        } else {
                            user_keymap_content = content;
                            migrating_in_memory = false;
                        }
                    }
                }
            };
            cx.update(|cx| {
                if let Some(notifier) = MigrationNotification::try_global(cx) {
                    notifier.update(cx, |_, cx| {
                        cx.emit(MigrationEvent::ContentChanged {
                            migration_type: MigrationType::Keymap,
                            migrating_in_memory,
                        });
                    });
                }
                let load_result = KeymapFile::load(&user_keymap_content, cx);
                match load_result {
                    KeymapFileLoadResult::Success { key_bindings } => {
                        reload_keymaps(cx, key_bindings);
                        dismiss_app_notification(&notification_id.clone(), cx);
                    }
                    KeymapFileLoadResult::SomeFailedToLoad {
                        key_bindings,
                        error_message,
                    } => {
                        if !key_bindings.is_empty() {
                            reload_keymaps(cx, key_bindings);
                        }
                        show_keymap_file_load_error(notification_id.clone(), error_message, cx);
                    }
                    KeymapFileLoadResult::JsonParseFailure { error } => {
                        show_keymap_file_json_error(notification_id.clone(), &error, cx)
                    }
                }
            });
        }
    })
    .detach();
}

fn show_keymap_file_json_error(
    notification_id: NotificationId,
    error: &anyhow::Error,
    cx: &mut App,
) {
    let message: SharedString =
        format!("JSON parse error in keymap file. Bindings not reloaded.\n\n{error}").into();
    show_app_notification(notification_id, cx, move |cx| {
        cx.new(|cx| {
            MessageNotification::new(message.clone(), cx)
                .primary_message("Open Keymap File")
                .primary_icon(IconName::Settings)
                .primary_on_click(|window, cx| {
                    window.dispatch_action(mav_actions::OpenKeymapFile.boxed_clone(), cx);
                    cx.emit(DismissEvent);
                })
        })
    });
}

fn show_keymap_file_load_error(
    notification_id: NotificationId,
    error_message: MarkdownString,
    cx: &mut App,
) {
    show_markdown_app_notification(
        notification_id,
        error_message,
        "Open Keymap File".into(),
        |window, cx| {
            window.dispatch_action(mav_actions::OpenKeymapFile.boxed_clone(), cx);
            cx.emit(DismissEvent);
        },
        cx,
    )
}

fn show_markdown_app_notification<F>(
    notification_id: NotificationId,
    message: MarkdownString,
    primary_button_message: SharedString,
    primary_button_on_click: F,
    cx: &mut App,
) where
    F: 'static + Send + Sync + Fn(&mut Window, &mut Context<MessageNotification>),
{
    let markdown = cx.new(|cx| Markdown::new(message.0.into(), None, None, cx));
    let primary_button_on_click = Arc::new(primary_button_on_click);

    show_app_notification(notification_id, cx, move |cx| {
        let markdown = markdown.clone();
        let primary_button_message = primary_button_message.clone();
        let primary_button_on_click = primary_button_on_click.clone();

        cx.new(move |cx| {
            MessageNotification::new_from_builder(cx, move |window, cx| {
                image_cache(retain_all("notification-cache"))
                    .child(div().text_ui(cx).child(MarkdownElement::new(
                        markdown.clone(),
                        MarkdownStyle::themed(MarkdownFont::Editor, window, cx),
                    )))
                    .into_any()
            })
            .primary_message(primary_button_message)
            .primary_icon(IconName::Settings)
            .primary_on_click_arc(primary_button_on_click)
        })
    })
}

fn reload_keymaps(cx: &mut App, mut user_key_bindings: Vec<KeyBinding>) {
    cx.clear_key_bindings();
    load_default_keymap(cx);

    for key_binding in &mut user_key_bindings {
        key_binding.set_meta(KeybindSource::User.meta());
    }
    cx.bind_keys(user_key_bindings);

    let menus = app_menus(cx);
    cx.set_menus(menus);
    // On Windows, this is set in the `update_jump_list` method of the `HistoryManager`.
    #[cfg(not(target_os = "windows"))]
    cx.set_dock_menu(vec![gpui::MenuItem::action(
        "New Window",
        workspace::NewWindow,
    )]);
    // todo: nicer api here?
    keymap_editor::KeymapEventChannel::trigger_keymap_changed(cx);
}

pub fn load_default_keymap(cx: &mut App) {
    let base_keymap = *BaseKeymap::get_global(cx);
    if base_keymap == BaseKeymap::None {
        return;
    }

    cx.bind_keys(
        KeymapFile::load_asset(DEFAULT_KEYMAP_PATH, Some(KeybindSource::Default), cx).unwrap(),
    );

    if let Some(asset_path) = base_keymap.asset_path() {
        cx.bind_keys(KeymapFile::load_asset(asset_path, Some(KeybindSource::Base), cx).unwrap());
    }

    if VimModeSetting::get_global(cx).0 || vim_mode_setting::HelixModeSetting::get_global(cx).0 {
        cx.bind_keys(
            KeymapFile::load_asset(VIM_KEYMAP_PATH, Some(KeybindSource::Vim), cx).unwrap(),
        );
    }

    cx.bind_keys(
        KeymapFile::load_asset(
            SPECIFIC_OVERRIDES_KEYMAP_PATH,
            Some(KeybindSource::Default),
            cx,
        )
        .unwrap(),
    );
}
