use super::*;

fn files_not_created_on_launch(errors: HashMap<io::ErrorKind, Vec<&Path>>) {
    let message = "Mav failed to launch";
    let error_details = errors
        .into_iter()
        .flat_map(|(kind, paths)| {
            #[allow(unused_mut)] // for non-unix platforms
            let mut error_kind_details = match paths.len() {
                0 => return None,
                1 => format!(
                    "{kind} when creating directory {:?}",
                    paths.first().expect("match arm checks for a single entry")
                ),
                _many => format!("{kind} when creating directories {paths:?}"),
            };

            #[cfg(unix)]
            {
                if kind == io::ErrorKind::PermissionDenied {
                    error_kind_details.push_str("\n\nConsider using chown and chmod tools for altering the directories permissions if your user has corresponding rights.\
                        \nFor example, `sudo chown $(whoami):staff ~/.config` and `chmod +uwrx ~/.config`");
                }
            }

            Some(error_kind_details)
        })
        .collect::<Vec<_>>().join("\n\n");

    eprintln!("{message}: {error_details}");
    build_application()
        .with_quit_mode(QuitMode::Explicit)
        .run(move |cx| {
            if let Ok(window) = cx.open_window(gpui::WindowOptions::default(), |_, cx| {
                cx.new(|_| gpui::Empty)
            }) {
                window
                    .update(cx, |_, window, cx| {
                        let response = window.prompt(
                            gpui::PromptLevel::Critical,
                            message,
                            Some(&error_details),
                            &["Exit"],
                            cx,
                        );

                        cx.spawn_in(window, async move |_, cx| {
                            response.await?;
                            cx.update(|_, cx| cx.quit())
                        })
                        .detach_and_log_err(cx);
                    })
                    .log_err();
            } else {
                fail_to_open_window(anyhow::anyhow!("{message}: {error_details}"), cx)
            }
        })
}

fn fail_to_open_window_async(e: anyhow::Error, cx: &mut AsyncApp) {
    cx.update(|cx| fail_to_open_window(e, cx));
}

fn fail_to_open_window(e: anyhow::Error, _cx: &mut App) {
    eprintln!(
        "Mav failed to open a window: {e:?}. See https://mav.dev/docs/linux for troubleshooting steps."
    );
    #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
    {
        process::exit(1);
    }

    // Maybe unify this with gpui::platform::linux::platform::ResultExt::notify_err(..)?
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    {
        use ashpd::desktop::notification::{Notification, NotificationProxy, Priority};
        _cx.spawn(async move |_cx| {
            let Ok(proxy) = NotificationProxy::new().await else {
                process::exit(1);
            };

            let notification_id = "dev.mav.Oops";
            proxy
                .add_notification(
                    notification_id,
                    Notification::new("Mav failed to launch")
                        .body(Some(
                            format!(
                                "{e:?}. See https://mav.dev/docs/linux for troubleshooting steps."
                            )
                            .as_str(),
                        ))
                        .priority(Priority::High)
                        .icon(ashpd::desktop::Icon::with_names(&[
                            "dialog-question-symbolic",
                        ])),
                )
                .await
                .ok();

            process::exit(1);
        })
        .detach();
    }
}

fn dump_all_gpui_actions() {
    #[derive(Debug, serde::Serialize)]
    struct ActionDef {
        name: &'static str,
        human_name: String,
        schema: Option<serde_json::Value>,
        deprecated_aliases: &'static [&'static str],
        deprecation_message: Option<&'static str>,
        documentation: Option<&'static str>,
    }
    let mut generator = settings::KeymapFile::action_schema_generator();
    let mut actions = gpui::generate_list_of_all_registered_actions()
        .map(|action| {
            let schema = (action.json_schema)(&mut generator)
                .map(|s| serde_json::to_value(s).expect("Failed to serialize action schema"));
            ActionDef {
                name: action.name,
                human_name: command_palette::humanize_action_name(action.name),
                schema,
                deprecated_aliases: action.deprecated_aliases,
                deprecation_message: action.deprecation_message,
                documentation: action.documentation,
            }
        })
        .collect::<Vec<ActionDef>>();

    actions.sort_by_key(|a| a.name);

    let schema_definitions = serde_json::to_value(generator.definitions())
        .expect("Failed to serialize schema definitions");

    let output = serde_json::json!({
        "actions": actions,
        "schema_definitions": schema_definitions,
    });

    io::Write::write(
        &mut std::io::stdout(),
        serde_json::to_string_pretty(&output).unwrap().as_bytes(),
    )
    .unwrap();
}

#[cfg(target_os = "windows")]
fn check_for_conpty_dll() {
    use windows::{
        Win32::{Foundation::FreeLibrary, System::LibraryLoader::LoadLibraryW},
        core::w,
    };

    if let Ok(hmodule) = unsafe { LoadLibraryW(w!("conpty.dll")) } {
        unsafe {
            FreeLibrary(hmodule)
                .context("Failed to free conpty.dll")
                .log_err();
        }
    } else {
        log::warn!("Failed to load conpty.dll. Terminal will work with reduced functionality.");
    }
}
