mod app_menus;
pub mod edit_prediction_registry;
mod file_actions;
mod lifecycle_actions;
#[cfg(target_os = "macos")]
pub(crate) mod mac_only_instance;
mod migrate;
#[cfg(target_os = "macos")]
pub(crate) mod move_to_applications;
mod open_listener;
mod open_url_modal;
mod quick_action_bar;
pub mod remote_debug;
mod settings_files;
pub mod telemetry_log;
mod theme_loading;
pub use settings_files::{handle_keymap_file_changes, watch_settings_files, watch_user_agents_md};
pub(crate) use theme_loading::eager_load_active_theme_and_icon_theme;
#[cfg(all(target_os = "macos", feature = "visual-tests"))]
pub mod visual_tests;
#[cfg(target_os = "windows")]
pub(crate) mod windows_only_instance;

use agent_settings::{UserAgentsMdState, init_user_agents_md};
use agent_ui::AgentDiffToolbar;
use anyhow::Context as _;
pub use app_menus::*;
use assets::Assets;

use breadcrumbs::Breadcrumbs;
use client::mav_urls;
use collections::VecDeque;
use debugger_ui::debugger_panel::DebugPanel;
use editor::{Editor, MultiBuffer};
use extension_host::ExtensionStore;
use feature_flags::{FeatureFlagAppExt as _, PanicFeatureFlag};
use file_actions::{
    open_bundled_file, open_project_debug_tasks_file, open_project_settings_file,
    open_project_tasks_file, open_settings_file,
};
use fs::Fs;
use futures::FutureExt as _;
use futures::{StreamExt, channel::mpsc, select_biased};
use git_ui::commit_view::CommitViewToolbar;
use git_ui::git_panel::GitPanel;
use git_ui::project_diff::{BranchDiffToolbar, ProjectDiffToolbar};
use git_ui::solo_diff_view::{SoloDiffGitToolbar, SoloDiffStyleToolbar};
use gpui::{
    Action, App, AppContext as _, AsyncWindowContext, ClipboardItem, Context, DismissEvent,
    Element, Entity, FocusHandle, Focusable, Image, ImageFormat, KeyBinding, ParentElement,
    PathPromptOptions, PromptLevel, ReadGlobal, SharedString, Size, Task, TaskExt, TitlebarOptions,
    UpdateGlobal, WeakEntity, Window, WindowBounds, WindowHandle, WindowKind, WindowOptions,
    actions, image_cache, img, point, px, retain_all,
};
use image_viewer::ImageInfo;
use language::Capability;
use language_onboarding::BasedPyrightBanner;
use language_tools::lsp_button::{self, LspButton};
use language_tools::lsp_log_view::LspLogToolbarItemView;
#[cfg(not(target_os = "windows"))]
use lifecycle_actions::install_cli;
use lifecycle_actions::{open_about_window, quit};
use markdown::{Markdown, MarkdownElement, MarkdownFont, MarkdownStyle};
use migrate::{MigrationBanner, MigrationEvent, MigrationNotification, MigrationType};
use migrator::migrate_keymap;
use onboarding::multibuffer_hint::MultibufferHint;
pub use open_listener::*;
use outline_panel::OutlinePanel;
use paths::{
    local_debug_file_relative_path, local_settings_file_relative_path,
    local_tasks_file_relative_path,
};
use project::{DirectoryLister, ProjectItem};
use project_panel::ProjectPanel;
use quick_action_bar::QuickActionBar;
use recent_projects::open_remote_project;
use release_channel::{AppCommitSha, AppVersion, ReleaseChannel};
use rope::Rope;
use search::project_search::ProjectSearchBar;
use settings::{
    BaseKeymap, DEFAULT_KEYMAP_PATH, DefaultOpenBehavior, InvalidSettingsError, KeybindSource,
    KeymapFile, KeymapFileLoadResult, MigrationStatus, SPECIFIC_OVERRIDES_KEYMAP_PATH, Settings,
    SettingsFile, SettingsStore, VIM_KEYMAP_PATH, initial_local_debug_tasks_content,
    initial_project_settings_content, initial_tasks_content, update_settings_file,
};
use sidebar::Sidebar;
#[cfg(debug_assertions)]
use workspace::workspace_error::{ErrorAction, ErrorSeverity, WorkspaceError};

use std::{
    borrow::Cow,
    path::{Path, PathBuf},
    sync::Arc,
    sync::atomic::{self, AtomicBool},
};
use theme::{ActiveTheme, SystemAppearance, ThemeRegistry, deserialize_icon_theme};
use theme_settings::{ThemeSettings, load_user_theme};
use ui::{Navigable, NavigableEntry, PopoverMenuHandle, TintColor, prelude::*};
use util::markdown::MarkdownString;
use util::rel_path::RelPath;
use util::{ResultExt, asset_str, maybe};
use uuid::Uuid;
use vim_mode_setting::VimModeSetting;
use workspace::notifications::{NotificationId, dismiss_app_notification, show_app_notification};

use mav_actions::{
    About, OpenAccountSettings, OpenBrowser, OpenDocs, OpenMavUrl, OpenServerSettings,
    OpenSettingsFile, OpenStatusPage, Quit,
};
use workspace::{
    AppState, MultiWorkspace, NewFile, NewWindow, OpenLog, Toast, Workspace, WorkspaceSettings,
    create_and_open_local_file, notifications::simple_message_notification::MessageNotification,
    open_new,
};
use workspace::{
    CloseIntent, CloseProject, CloseWindow, RestoreBanner, with_active_or_new_workspace,
};
use workspace::{Pane, notifications::DetachAndPromptErr};

const DOCS_URL: &str = "https://mav.dev/docs/";
const STATUS_URL: &str = "https://status.mav.dev";

pub struct CrashHandler(pub Arc<crashes::Client>);

impl gpui::Global for CrashHandler {}

actions!(
    mav,
    [
        /// Opens the element inspector for debugging UI.
        DebugElements,
        /// Hides the application window.
        Hide,
        /// Hides all other application windows.
        HideOthers,
        /// Minimizes the current window.
        Minimize,
        /// Opens the default settings file.
        OpenDefaultSettings,
        /// Opens project-specific settings file.
        OpenProjectSettingsFile,
        /// Opens the project tasks configuration.
        OpenProjectTasks,
        /// Opens the tasks panel.
        OpenTasks,
        /// Opens debug tasks configuration.
        OpenDebugTasks,
        /// Shows the default semantic token rules (read-only).
        ShowDefaultSemanticTokenRules,
        /// Resets the application database.
        ResetDatabase,
        /// Shows all hidden windows.
        ShowAll,
        /// Toggles fullscreen mode.
        ToggleFullScreen,
        /// Zooms the window.
        Zoom,
        /// Triggers a test panic for debugging.
        TestPanic,
        /// Triggers a hard crash for debugging.
        TestCrash,
    ]
);

actions!(
    dev,
    [
        /// Opens a prompt to enter a URL to open.
        OpenUrlPrompt,
    ]
);

#[cfg(debug_assertions)]
actions!(
    dev,
    [
        /// Show an error on the workspace level.
        ShowWorkspaceError
    ]
);

pub fn init(cx: &mut App) {
    #[cfg(target_os = "macos")]
    cx.on_action(|_: &Hide, cx| cx.hide());
    #[cfg(target_os = "macos")]
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    #[cfg(target_os = "macos")]
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
    cx.on_action(quit);

    cx.on_action(|_: &RestoreBanner, cx| title_bar::restore_banner(cx));

    cx.observe_flag::<PanicFeatureFlag, _>({
        let mut added = false;
        move |flag, cx| {
            if added || !*flag {
                return;
            }
            added = true;
            cx.on_action(|_: &TestPanic, _| panic!("Ran the TestPanic action"))
                .on_action(|_: &TestCrash, _| {
                    unsafe extern "C" {
                        fn puts(s: *const i8);
                    }
                    unsafe {
                        puts(0xabad1d3a as *const i8);
                    }
                });
        }
    })
    .detach();

    // When Mav logs to stdout rather than the log file, avoid registering
    // handlers for both `OpenLog` and `RevealLogInFileManager`, as the log file
    // does not exist in that scenario and these actions would error.
    if !crate::stdout_is_a_pty() {
        cx.on_action(|_: &OpenLog, cx| {
            with_active_or_new_workspace(cx, |workspace, window, cx| {
                open_log_file(workspace, window, cx);
            });
        })
        .on_action(|_: &workspace::RevealLogInFileManager, cx| {
            cx.reveal_path(paths::log_file().as_path());
        });
    }

    cx.on_action(|_: &mav_actions::OpenLicenses, cx| {
        with_active_or_new_workspace(cx, |workspace, window, cx| {
            open_bundled_file(
                workspace,
                asset_str::<Assets>("licenses.md"),
                "Open Source License Attribution",
                "Markdown",
                window,
                cx,
            );
        });
    })
    .on_action(|&mav_actions::OpenKeymapFile, cx| {
        with_active_or_new_workspace(cx, |_, window, cx| {
            open_settings_file(
                paths::keymap_file(),
                || settings::initial_keymap_content().as_ref().into(),
                window,
                cx,
            );
        });
    })
    .on_action(|_: &OpenSettingsFile, cx| {
        with_active_or_new_workspace(cx, |_, window, cx| {
            open_settings_file(
                paths::settings_file(),
                || settings::initial_user_settings_content().as_ref().into(),
                window,
                cx,
            );
        });
    })
    .on_action(|_: &OpenAccountSettings, cx| {
        with_active_or_new_workspace(cx, |_, _, cx| {
            cx.open_url(&mav_urls::account_url(cx));
        });
    })
    .on_action(|_: &OpenTasks, cx| {
        with_active_or_new_workspace(cx, |_, window, cx| {
            open_settings_file(
                paths::tasks_file(),
                || settings::initial_tasks_content().as_ref().into(),
                window,
                cx,
            );
        });
    })
    .on_action(|_: &OpenDebugTasks, cx| {
        with_active_or_new_workspace(cx, |_, window, cx| {
            open_settings_file(
                paths::debug_scenarios_file(),
                || settings::initial_debug_tasks_content().as_ref().into(),
                window,
                cx,
            );
        });
    })
    .on_action(|_: &ShowDefaultSemanticTokenRules, cx| {
        with_active_or_new_workspace(cx, |workspace, window, cx| {
            open_bundled_file(
                workspace,
                settings::default_semantic_token_rules(),
                "Default Semantic Token Rules",
                "JSONC",
                window,
                cx,
            );
        });
    })
    .on_action(|_: &OpenDefaultSettings, cx| {
        with_active_or_new_workspace(cx, |workspace, window, cx| {
            open_bundled_file(
                workspace,
                settings::default_settings(),
                "Default Settings",
                "JSON",
                window,
                cx,
            );
        });
    })
    .on_action(|_: &mav_actions::OpenDefaultKeymap, cx| {
        with_active_or_new_workspace(cx, |workspace, window, cx| {
            open_bundled_file(
                workspace,
                settings::default_keymap(),
                "Default Key Bindings",
                "JSON",
                window,
                cx,
            );
        });
    })
    .on_action(|_: &About, cx| {
        open_about_window(cx);
    });
}

fn bind_on_window_closed(cx: &mut App) -> Option<gpui::Subscription> {
    #[cfg(target_os = "macos")]
    {
        WorkspaceSettings::get_global(cx)
            .on_last_window_closed
            .is_quit_app()
            .then(|| {
                cx.on_window_closed(|cx, _window_id| {
                    if cx.windows().is_empty() {
                        cx.quit();
                    }
                })
            })
    }
    #[cfg(not(target_os = "macos"))]
    {
        Some(cx.on_window_closed(|cx, _window_id| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        }))
    }
}

pub fn build_window_options(display_uuid: Option<Uuid>, cx: &mut App) -> WindowOptions {
    let display = display_uuid.and_then(|uuid| {
        cx.displays()
            .into_iter()
            .find(|display| display.uuid().ok() == Some(uuid))
    });
    let app_id = ReleaseChannel::global(cx).app_id();
    let window_decorations = match std::env::var("MAV_WINDOW_DECORATIONS") {
        Ok(val) if val == "server" => gpui::WindowDecorations::Server,
        Ok(val) if val == "client" => gpui::WindowDecorations::Client,
        _ => match WorkspaceSettings::get_global(cx).window_decorations {
            settings::WindowDecorations::Server => gpui::WindowDecorations::Server,
            settings::WindowDecorations::Client => gpui::WindowDecorations::Client,
        },
    };

    let use_system_window_tabs = WorkspaceSettings::get_global(cx).use_system_window_tabs;

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    static APP_ICON: std::sync::LazyLock<Option<std::sync::Arc<image::RgbaImage>>> =
        std::sync::LazyLock::new(|| {
            // this shouldn't fail since decode is checked in build.rs
            const BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/app_icon.png"));
            util::maybe!({
                let image = image::ImageReader::new(std::io::Cursor::new(BYTES))
                    .with_guessed_format()?
                    .decode()?
                    .into();
                anyhow::Ok(Arc::new(image))
            })
            .log_err()
        });

    WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: None,
            appears_transparent: true,
            traffic_light_position: Some(point(px(9.0), px(9.0))),
        }),
        window_bounds: None,
        focus: false,
        show: false,
        kind: WindowKind::Normal,
        // On macOS, Mav handles window movement itself, so disable AppKit's titlebar dragging.
        // On other platforms, `is_movable` gates native window dragging (e.g. Windows'
        // `HTCAPTION` hit test), so it must remain `true`.
        is_movable: cfg!(not(target_os = "macos")),
        display_id: display.map(|display| display.id()),
        window_background: cx.theme().window_background_appearance(),
        app_id: Some(app_id.to_owned()),
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        icon: APP_ICON.as_ref().cloned(),
        window_decorations: Some(window_decorations),
        window_min_size: Some(gpui::Size {
            width: px(360.0),
            height: px(240.0),
        }),
        tabbing_identifier: if use_system_window_tabs {
            Some(String::from("mav"))
        } else {
            None
        },
        ..Default::default()
    }
}

pub fn initialize_workspace(app_state: Arc<AppState>, cx: &mut App) {
    let mut _on_close_subscription = bind_on_window_closed(cx);
    cx.observe_global::<SettingsStore>(move |cx| {
        // A 1.92 regression causes unused-assignment to trigger on this variable.
        _ = _on_close_subscription.is_some();
        _on_close_subscription = bind_on_window_closed(cx);
    })
    .detach();

    init_cursor_hide_mode(cx);

    cx.observe_new(|_multi_workspace: &mut MultiWorkspace, window, cx| {
        let Some(window) = window else {
            return;
        };

        #[cfg(feature = "track-project-leak")]
        {
            let multi_workspace_handle = cx.weak_entity();
            let workspace_handle = _multi_workspace.workspace().downgrade();
            let project_handle = _multi_workspace.workspace().read(cx).project().downgrade();
            let window_id_2 = window.window_handle().window_id();
            cx.on_window_closed(move |cx, window_id| {
                let multi_workspace_handle = multi_workspace_handle.clone();
                let workspace_handle = workspace_handle.clone();
                let project_handle = project_handle.clone();
                if window_id != window_id_2 {
                    return;
                }
                cx.spawn(async move |cx| {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(1500))
                        .await;

                    multi_workspace_handle.assert_released();
                    workspace_handle.assert_released();
                    project_handle.assert_released();
                })
                .detach();
            })
            .detach();
        }

        cx.spawn_in(window, async move |_this, cx| {
            const TELEMETRY_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5 * 60);
            loop {
                cx.background_executor().timer(TELEMETRY_INTERVAL).await;
                if cx
                    .update(|window, cx| {
                        input_latency_ui::report_input_latency_telemetry(window, cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();

        let multi_workspace_handle = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            multi_workspace_handle
                .update(cx, |multi_workspace, cx| {
                    // We'll handle closing asynchronously
                    multi_workspace.close_window(&CloseWindow, window, cx);
                    false
                })
                .unwrap_or(true)
        });

        let window_handle = window.window_handle();
        let multi_workspace_handle = cx.entity();
        cx.subscribe_in(
            &multi_workspace_handle,
            window,
            |this, _multi_workspace, event: &workspace::MultiWorkspaceEvent, window, cx| {
                let workspace::MultiWorkspaceEvent::ActiveWorkspaceChanged { source_workspace } =
                    event
                else {
                    return;
                };

                let active_workspace = this.workspace().clone();
                let source_workspace = source_workspace.clone();
                active_workspace.update(cx, |workspace, cx| {
                    if let Some(ref source) = source_workspace {
                        if let Some(panel) = workspace.panel::<agent_ui::AgentPanel>(cx) {
                            panel.update(cx, |panel, cx| {
                                panel.initialize_from_source_workspace_if_needed(
                                    source.clone(),
                                    window,
                                    cx,
                                );
                            });
                        }
                    }
                });
            },
        )
        .detach();

        cx.defer(move |cx| {
            window_handle
                .update(cx, |_, window, cx| {
                    let sidebar =
                        cx.new(|cx| Sidebar::new(multi_workspace_handle.clone(), window, cx));
                    multi_workspace_handle.update(cx, |multi_workspace, cx| {
                        multi_workspace.register_sidebar(sidebar, window, cx);
                    });
                })
                .ok();
        });
    })
    .detach();

    cx.observe_new(move |workspace: &mut Workspace, window, cx| {
        let Some(window) = window else {
            return;
        };

        let workspace_handle = cx.entity();
        let center_pane = workspace.active_pane().clone();
        initialize_pane(workspace, &center_pane, window, cx);

        cx.subscribe_in(&workspace_handle, window, {
            move |workspace, _, event, window, cx| match event {
                workspace::Event::PaneAdded(pane) => {
                    initialize_pane(workspace, pane, window, cx);
                }
                workspace::Event::OpenBundledFile {
                    text,
                    title,
                    language,
                } => open_bundled_file(workspace, text.clone(), title, language, window, cx),
                _ => {}
            }
        })
        .detach();

        #[cfg(not(any(test, target_os = "macos")))]
        initialize_file_watcher(window, cx);

        if let Some(specs) = window.gpu_specs() {
            log::info!("Using GPU: {:?}", specs);
            show_software_emulation_warning_if_needed(specs.clone(), window, cx);
            if let Some(crash_client) = cx.try_global::<CrashHandler>() {
                crashes::set_gpu_info(&crash_client.0, specs);
            }
        }

        let edit_prediction_menu_handle = PopoverMenuHandle::default();
        let edit_prediction_ui = cx.new(|cx| {
            edit_prediction_ui::EditPredictionButton::new(
                app_state.fs.clone(),
                app_state.user_store.clone(),
                edit_prediction_menu_handle.clone(),
                workspace.project().clone(),
                cx,
            )
        });
        workspace.register_action({
            move |_, _: &edit_prediction_ui::ToggleMenu, window, cx| {
                edit_prediction_menu_handle.toggle(window, cx);
            }
        });

        let search_button = cx.new(|_| search::search_status_button::SearchButton::new());
        let diagnostic_summary =
            cx.new(|cx| diagnostics::items::DiagnosticIndicator::new(workspace, cx));
        let active_file_name = cx.new(|_| workspace::active_file_name::ActiveFileName::new());
        let activity_indicator = activity_indicator::ActivityIndicator::new(
            workspace,
            workspace.project().read(cx).languages().clone(),
            window,
            cx,
        );
        let active_buffer_encoding =
            cx.new(|_| encoding_selector::ActiveBufferEncoding::new(workspace));
        let active_buffer_language =
            cx.new(|_| language_selector::ActiveBufferLanguage::new(workspace));
        let active_toolchain_language =
            cx.new(|cx| toolchain_selector::ActiveToolchain::new(workspace, window, cx));
        let vim_mode_indicator = cx.new(|cx| vim::ModeIndicator::new(window, cx));
        let image_info = cx.new(|_cx| ImageInfo::new(workspace));

        let lsp_button_menu_handle = PopoverMenuHandle::default();
        let lsp_button =
            cx.new(|cx| LspButton::new(workspace, lsp_button_menu_handle.clone(), window, cx));
        workspace.register_action({
            move |_, _: &lsp_button::ToggleMenu, window, cx| {
                lsp_button_menu_handle.toggle(window, cx);
            }
        });

        let cursor_position =
            cx.new(|_| go_to_line::cursor_position::CursorPosition::new(workspace));
        let line_ending_indicator =
            cx.new(|_| line_ending_selector::LineEndingIndicator::default());
        let git_blame_status = cx.new(|_| git_ui::GitBlameStatus::default());
        let merge_conflict_indicator =
            cx.new(|cx| git_ui::MergeConflictIndicator::new(workspace, cx));
        workspace.status_bar().update(cx, |status_bar, cx| {
            status_bar.add_left_item(search_button, window, cx);
            status_bar.add_left_item(lsp_button, window, cx);
            status_bar.add_left_item(diagnostic_summary, window, cx);
            status_bar.add_left_item(active_file_name, window, cx);
            status_bar.add_left_item(git_blame_status, window, cx);
            status_bar.add_left_item(merge_conflict_indicator, window, cx);
            status_bar.add_left_item(activity_indicator, window, cx);
            status_bar.add_right_item(edit_prediction_ui, window, cx);
            status_bar.add_right_item(active_buffer_encoding, window, cx);
            status_bar.add_right_item(active_buffer_language, window, cx);
            status_bar.add_right_item(active_toolchain_language, window, cx);
            status_bar.add_right_item(line_ending_indicator, window, cx);
            status_bar.add_right_item(vim_mode_indicator, window, cx);
            status_bar.add_right_item(cursor_position, window, cx);
            status_bar.add_right_item(image_info, window, cx);
        });

        let panels_task = initialize_panels(window, cx);
        workspace.set_panels_task(panels_task);
        register_actions(app_state.clone(), workspace, window, cx);

        if !workspace.has_active_modal(window, cx) {
            workspace.focus_handle(cx).focus(window, cx);
        }
    })
    .detach();
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
#[allow(unused)]
fn initialize_file_watcher(window: &mut Window, cx: &mut Context<Workspace>) {
    if let Err(e) = fs::fs_watcher::global(|_| {}) {
        let message = format!(
            db::indoc! {r#"
            inotify_init returned {}

            This may be due to system-wide limits on inotify instances. For troubleshooting see: https://mav.dev/docs/linux
            "#},
            e
        );
        let prompt = window.prompt(
            PromptLevel::Critical,
            "Could not start inotify",
            Some(&message),
            &["Troubleshoot and Quit"],
            cx,
        );
        cx.spawn(async move |_, cx| {
            if prompt.await == Ok(0) {
                cx.update(|cx| {
                    cx.open_url("https://mav.dev/docs/linux#could-not-start-inotify");
                    cx.quit();
                });
            }
        })
        .detach()
    }
}

#[cfg(target_os = "windows")]
#[allow(unused)]
fn initialize_file_watcher(window: &mut Window, cx: &mut Context<Workspace>) {
    if let Err(e) = fs::fs_watcher::global(|_| {}) {
        let message = format!(
            db::indoc! {r#"
            ReadDirectoryChangesW initialization failed: {}

            This may occur on network filesystems and WSL paths. For troubleshooting see: https://mav.dev/docs/windows
            "#},
            e
        );
        let prompt = window.prompt(
            PromptLevel::Critical,
            "Could not start ReadDirectoryChangesW",
            Some(&message),
            &["Troubleshoot and Quit"],
            cx,
        );
        cx.spawn(async move |_, cx| {
            if prompt.await == Ok(0) {
                cx.update(|cx| {
                    cx.open_url("https://mav.dev/docs/windows");
                    cx.quit()
                });
            }
        })
        .detach()
    }
}

fn show_software_emulation_warning_if_needed(
    specs: gpui::GpuSpecs,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    if specs.is_software_emulated && std::env::var("MAV_ALLOW_EMULATED_GPU").is_err() {
        let (graphics_api, docs_url, open_url) = if cfg!(target_os = "windows") {
            (
                "DirectX",
                "https://mav.dev/docs/windows",
                "https://mav.dev/docs/windows",
            )
        } else {
            (
                "Vulkan",
                "https://mav.dev/docs/linux",
                "https://mav.dev/docs/linux#mav-fails-to-open-windows",
            )
        };
        let message = format!(
            db::indoc! {r#"
            Mav uses {} for rendering and requires a compatible GPU.

            Currently you are using a software emulated GPU ({}) which
            will result in awful performance.

            For troubleshooting see: {}
            Set MAV_ALLOW_EMULATED_GPU=1 env var to permanently override.
            "#},
            graphics_api, specs.device_name, docs_url
        );
        let prompt = window.prompt(
            PromptLevel::Critical,
            "Unsupported GPU",
            Some(&message),
            &["Skip", "Troubleshoot and Quit"],
            cx,
        );
        cx.spawn(async move |_, cx| {
            if prompt.await == Ok(1) {
                cx.update(|cx| {
                    cx.open_url(open_url);
                    cx.quit();
                });
            }
        })
        .detach()
    }
}

fn initialize_panels(window: &mut Window, cx: &mut Context<Workspace>) -> Task<anyhow::Result<()>> {
    cx.spawn_in(window, async move |workspace_handle, cx| {
        let project_panel = ProjectPanel::load(workspace_handle.clone(), cx.clone());
        let outline_panel = OutlinePanel::load(workspace_handle.clone(), cx.clone());
        let git_panel = GitPanel::load(workspace_handle.clone(), cx.clone());
        let channels_panel =
            collab_ui::collab_panel::CollabPanel::load(workspace_handle.clone(), cx.clone());
        let debug_panel = DebugPanel::load(workspace_handle.clone(), cx);

        async fn add_panel_when_ready(
            panel_task: impl Future<Output = anyhow::Result<Entity<impl workspace::Panel>>> + 'static,
            workspace_handle: WeakEntity<Workspace>,
            mut cx: gpui::AsyncWindowContext,
        ) {
            if let Some(panel) = panel_task.await.context("failed to load panel").log_err()
            {
                workspace_handle
                    .update_in(&mut cx, |workspace, window, cx| {
                        workspace.add_panel(panel, window, cx);
                    })
                    .log_err();
            }
        }

        futures::join!(
            add_panel_when_ready(project_panel, workspace_handle.clone(), cx.clone()),
            add_panel_when_ready(outline_panel, workspace_handle.clone(), cx.clone()),
            add_panel_when_ready(git_panel, workspace_handle.clone(), cx.clone()),
            add_panel_when_ready(channels_panel, workspace_handle.clone(), cx.clone()),
            async move {
                debug_panel.await.context("failed to load debug panel").log_err();
            },
            initialize_agent_panel(workspace_handle, cx.clone()).map(|r| r.log_err()),
        );

        anyhow::Ok(())
    })
}

async fn initialize_agent_panel(
    workspace_handle: WeakEntity<Workspace>,
    mut cx: AsyncWindowContext,
) -> anyhow::Result<()> {
    workspace_handle.update_in(&mut cx, |workspace, _window, _cx| {
        if !cfg!(test) {
            workspace.register_action(agent_ui::InlineAssistant::inline_assist);
        }
    })?;

    anyhow::Ok(())
}

fn register_actions(
    app_state: Arc<AppState>,
    workspace: &mut Workspace,
    _: &mut Window,
    cx: &mut Context<Workspace>,
) {
    workspace
        .register_action(|_, _: &OpenDocs, _, cx| cx.open_url(DOCS_URL))
        .register_action(|_, _: &OpenStatusPage, _, cx| cx.open_url(STATUS_URL))
        .register_action(
            |workspace: &mut Workspace,
             _: &input_latency_ui::DumpInputLatencyHistogram,
             window: &mut Window,
             cx: &mut Context<Workspace>| {
                let report =
                    input_latency_ui::format_input_latency_report(window, cx);
                let project = workspace.project().clone();
                let buffer = project.update(cx, |project, cx| {
                    project.create_local_buffer(&report, None, true, cx)
                });
                let editor =
                    cx.new(|cx| Editor::for_buffer(buffer, Some(project), window, cx));
                workspace.add_item_to_active_pane(Box::new(editor), None, true, window, cx);
            },
        )
        .register_action(|_, _: &Minimize, window, _| {
            window.minimize_window();
        })
        .register_action(|_, _: &Zoom, window, _| {
            window.zoom_window();
        })
        .register_action(|_, _: &ToggleFullScreen, window, _| {
            window.toggle_fullscreen();
        })
        .register_action(|_, action: &OpenMavUrl, _, cx| {
            OpenListener::global(cx).open(RawOpenRequest {
                urls: vec![String::from(&*action.url)],
                ..Default::default()
            })
        })
        .register_action(|workspace, _: &OpenUrlPrompt, window, cx| {
            workspace.toggle_modal(window, cx, |window, cx| {
                open_url_modal::OpenUrlModal::new(window, cx)
            });
        })
        .register_action(|workspace, action: &OpenBrowser, _window, cx| {
            // Parse and validate the URL to ensure it's properly formatted
            match url::Url::parse(&action.url) {
                Ok(parsed_url) => {
                    // Use the parsed URL's string representation which is properly escaped
                    cx.open_url(parsed_url.as_str());
                }
                Err(e) => {
                    workspace.show_error(
                        format!(
                            "Opening this URL in a browser failed because the URL is invalid: {}\n\nError was: {e}",
                            action.url
                        ),
                        cx,
                    );
                }
            }
        })
        .register_action(|workspace, action: &workspace::Open, window, cx| {
            telemetry::event!("Project Opened");
            workspace::prompt_for_open_path_and_open(
                workspace,
                workspace.app_state().clone(),
                PathPromptOptions {
                    files: true,
                    directories: true,
                    multiple: true,
                    prompt: None,
                },
                action.create_new_window.unwrap_or_else(|| {
                    matches!(
                        WorkspaceSettings::get_global(cx).default_open_behavior,
                        DefaultOpenBehavior::NewWindow
                    )
                }),
                window,
                cx,
            );
        })
        .register_action(|workspace, _: &workspace::OpenFiles, window, cx| {
            let directories = cx.can_select_mixed_files_and_dirs();
            workspace::prompt_for_open_path_and_open(
                workspace,
                workspace.app_state().clone(),
                PathPromptOptions {
                    files: true,
                    directories,
                    multiple: true,
                    prompt: None,
                },
                true,
                window,
                cx,
            );
        })
        .register_action(|workspace, action: &mav_actions::OpenRemote, window, cx| {
            if !action.from_existing_connection {
                cx.propagate();
                return;
            }
            // You need existing remote connection to open it this way
            if workspace.project().read(cx).is_local() {
                return;
            }
            telemetry::event!("Project Opened");
            let paths = workspace.prompt_for_open_path(
                PathPromptOptions {
                    files: true,
                    directories: true,
                    multiple: true,
                    prompt: None,
                },
                DirectoryLister::Project(workspace.project().clone()),
                window,
                cx,
            );
            cx.spawn_in(window, async move |this, cx| {
                let Some(paths) = paths.await.log_err().flatten() else {
                    return;
                };
                if let Some(task) = this
                    .update_in(cx, |this, window, cx| {
                        open_new_ssh_project_from_project(this, paths, window, cx)
                    })
                    .log_err()
                {
                    task.await.log_err();
                }
            })
            .detach()
        })
        .register_action({
            let fs = app_state.fs.clone();
            move |_, action: &mav_actions::IncreaseUiFontSize, _window, cx| {
                if action.persist {
                    update_settings_file(fs.clone(), cx, move |settings, cx| {
                        let ui_font_size = ThemeSettings::get_global(cx).ui_font_size(cx) + px(1.0);
                        let _ = settings
                            .theme
                            .ui_font_size
                            .insert(f32::from(theme_settings::clamp_font_size(ui_font_size)).into());
                    });
                } else {
                    theme_settings::adjust_ui_font_size(cx, |size| size + px(1.0));
                }
            }
        })
        .register_action({
            let fs = app_state.fs.clone();
            move |_, action: &mav_actions::DecreaseUiFontSize, _window, cx| {
                if action.persist {
                    update_settings_file(fs.clone(), cx, move |settings, cx| {
                        let ui_font_size = ThemeSettings::get_global(cx).ui_font_size(cx) - px(1.0);
                        let _ = settings
                            .theme
                            .ui_font_size
                            .insert(f32::from(theme_settings::clamp_font_size(ui_font_size)).into());
                    });
                } else {
                    theme_settings::adjust_ui_font_size(cx, |size| size - px(1.0));
                }
            }
        })
        .register_action({
            let fs = app_state.fs.clone();
            move |_, action: &mav_actions::ResetUiFontSize, _window, cx| {
                if action.persist {
                    update_settings_file(fs.clone(), cx, move |settings, _| {
                        settings.theme.ui_font_size = None;
                    });
                } else {
                    theme_settings::reset_ui_font_size(cx);
                }
            }
        })
        .register_action({
            let fs = app_state.fs.clone();
            move |_, action: &mav_actions::IncreaseBufferFontSize, _window, cx| {
                if action.persist {
                    update_settings_file(fs.clone(), cx, move |settings, cx| {
                        let buffer_font_size =
                            ThemeSettings::get_global(cx).buffer_font_size(cx) + px(1.0);
                        let _ = settings
                            .theme
                            .buffer_font_size
                            .insert(f32::from(theme_settings::clamp_font_size(buffer_font_size)).into());
                    });
                } else {
                    theme_settings::increase_buffer_font_size(cx);
                }
            }
        })
        .register_action({
            let fs = app_state.fs.clone();
            move |_, action: &mav_actions::DecreaseBufferFontSize, _window, cx| {
                if action.persist {
                    update_settings_file(fs.clone(), cx, move |settings, cx| {
                        let buffer_font_size =
                            ThemeSettings::get_global(cx).buffer_font_size(cx) - px(1.0);
                        let _ = settings
                            .theme
                            .buffer_font_size
                            .insert(f32::from(theme_settings::clamp_font_size(buffer_font_size)).into());
                    });
                } else {
                    theme_settings::decrease_buffer_font_size(cx);
                }
            }
        })
        .register_action({
            let fs = app_state.fs.clone();
            move |_, action: &mav_actions::ResetBufferFontSize, _window, cx| {
                if action.persist {
                    update_settings_file(fs.clone(), cx, move |settings, _| {
                        settings.theme.buffer_font_size = None;
                    });
                } else {
                    theme_settings::reset_buffer_font_size(cx);
                }
            }
        })
        .register_action({
            let fs = app_state.fs.clone();
            move |_, action: &mav_actions::ResetAllZoom, _window, cx| {
                if action.persist {
                    update_settings_file(fs.clone(), cx, move |settings, _| {
                        settings.theme.ui_font_size = None;
                        settings.theme.buffer_font_size = None;
                        settings.theme.agent_ui_font_size = None;
                        settings.theme.agent_buffer_font_size = None;
                    });
                } else {
                    theme_settings::reset_ui_font_size(cx);
                    theme_settings::reset_buffer_font_size(cx);
                    theme_settings::reset_agent_ui_font_size(cx);
                    theme_settings::reset_agent_buffer_font_size(cx);
                }
            }
        })
        .register_action(|_, _: &install_cli::RegisterMavScheme, window, cx| {
            cx.spawn_in(window, async move |workspace, cx| {
                install_cli::register_mav_scheme(cx).await?;
                workspace.update_in(cx, |workspace, _, cx| {
                    struct RegisterMavScheme;

                    workspace.show_toast(
                        Toast::new(
                            NotificationId::unique::<RegisterMavScheme>(),
                            format!(
                                "mav:// links will now open in {}.",
                                ReleaseChannel::global(cx).display_name()
                            ),
                        ),
                        cx,
                    )
                })?;
                Ok(())
            })
            .detach_and_prompt_err(
                "Error registering mav:// scheme",
                window,
                cx,
                |_, _, _| None,
            );
        })
        .register_action(open_project_settings_file)
        .register_action(open_project_tasks_file)
        .register_action(open_project_debug_tasks_file)
        .register_action(
            |workspace: &mut Workspace,
             _: &mav_actions::project_panel::ToggleFocus,
             window: &mut Window,
             cx: &mut Context<Workspace>| {
                workspace.toggle_panel_focus::<ProjectPanel>(window, cx);
            },
        )
        .register_action(
            |workspace: &mut Workspace,
             _: &outline_panel::ToggleFocus,
             window: &mut Window,
             cx: &mut Context<Workspace>| {
                workspace.toggle_panel_focus::<OutlinePanel>(window, cx);
            },
        )
        .register_action(
            |workspace: &mut Workspace,
             _: &collab_ui::collab_panel::ToggleFocus,
             window: &mut Window,
             cx: &mut Context<Workspace>| {
                workspace.toggle_panel_focus::<collab_ui::collab_panel::CollabPanel>(window, cx);
            },
        )
        .register_action({
            let app_state = app_state.clone();
            move |_, _: &NewWindow, _, cx| {
                open_new(
                    Default::default(),
                    app_state.clone(),
                    cx,
                    |workspace, window, cx| {
                        cx.activate(true);
                        // Create buffer synchronously to avoid flicker
                        let project = workspace.project().clone();
                        let buffer = project.update(cx, |project, cx| {
                            project.create_local_buffer("", None, true, cx)
                        });
                        let editor = cx.new(|cx| {
                            Editor::for_buffer(buffer, Some(project), window, cx)
                        });
                        workspace.add_item_to_active_pane(
                            Box::new(editor),
                            None,
                            true,
                            window,
                            cx,
                        );
                    },
                )
                .detach();
            }
        })
        .register_action({
            let app_state = app_state.clone();
            move |workspace, _: &CloseProject, window, cx| {
                let Some(window_handle) = window.window_handle().downcast::<MultiWorkspace>() else {
                    return;
                };
                let app_state = app_state.clone();
                let old_group_key = workspace.project_group_key(cx);
                cx.spawn_in(window, async move |this, cx| {
                    let should_continue = this
                        .update_in(cx, |workspace, window, cx| {
                            workspace.prepare_to_close(
                                CloseIntent::ReplaceWindow,
                                window,
                                cx,
                            )
                        })?
                        .await?;
                    if should_continue {
                        let task = cx.update(|_window, cx| {
                            open_new(
                                workspace::OpenOptions {
                                    requesting_window: Some(window_handle),
                                    ..Default::default()
                                },
                                app_state,
                                cx,
                                |workspace, window, cx| {
                                    cx.activate(true);
                                    let project = workspace.project().clone();
                                    let buffer = project.update(cx, |project, cx| {
                                        project.create_local_buffer("", None, true, cx)
                                    });
                                    let editor = cx.new(|cx| {
                                        Editor::for_buffer(buffer, Some(project), window, cx)
                                    });
                                    workspace.add_item_to_active_pane(
                                        Box::new(editor),
                                        None,
                                        true,
                                        window,
                                        cx,
                                    );
                                },
                            )
                        })?;
                        task.await?;
                        window_handle.update(cx, |mw, window, cx| {
                            mw.remove_project_group(&old_group_key, window, cx)
                        })?.await.log_err();
                        Ok::<(), anyhow::Error>(())
                    } else {
                        Ok(())
                    }
                })
                .detach_and_log_err(cx);
            }
        })
        .register_action({
            let app_state = app_state.clone();
            move |_, _: &NewFile, _, cx| {
                open_new(
                    Default::default(),
                    app_state.clone(),
                    cx,
                    |workspace, window, cx| {
                        Editor::new_file(workspace, &Default::default(), window, cx)
                    },
                )
                .detach_and_log_err(cx);
            }
        });

    #[cfg(not(target_os = "windows"))]
    workspace.register_action(install_cli);

    if workspace.project().read(cx).is_via_remote_server() {
        workspace.register_action({
            move |workspace, _: &OpenServerSettings, window, cx| {
                let open_server_settings = workspace
                    .project()
                    .update(cx, |project, cx| project.open_server_settings(cx));

                cx.spawn_in(window, async move |workspace, cx| {
                    let buffer = open_server_settings.await?;

                    workspace
                        .update_in(cx, |workspace, window, cx| {
                            workspace.open_path(
                                buffer
                                    .read(cx)
                                    .project_path(cx)
                                    .expect("Settings file must have a location"),
                                None,
                                true,
                                window,
                                cx,
                            )
                        })?
                        .await?;

                    anyhow::Ok(())
                })
                .detach_and_log_err(cx);
            }
        });
    }

    workspace.register_action(sidebar::dump_workspace_info);

    #[cfg(debug_assertions)]
    workspace.register_action(|workspace, _: &ShowWorkspaceError, _, cx| {
        struct DebugError;
        struct SecondDebugError;

        impl WorkspaceError for DebugError {
            fn primary_message(&self) -> SharedString {
                SharedString::new_static(
                    "Error: Prepare rename via rust-analyzer failed: No references found at position",
                )
            }

            fn severity(&self) -> ErrorSeverity {
                ErrorSeverity::Warning
            }

            fn primary_action(&self) -> ErrorAction {
                ErrorAction::dismiss()
            }
        }

        impl WorkspaceError for SecondDebugError {
            fn primary_message(&self) -> SharedString {
                SharedString::new_static("This is some error to ignore.")
            }

            fn severity(&self) -> ErrorSeverity {
                ErrorSeverity::Error
            }

            fn primary_action(&self) -> ErrorAction {
                ErrorAction::dismiss()
            }
        }

        workspace.show_error(DebugError, cx);
        workspace.show_error(SecondDebugError, cx);
    });
}

fn initialize_pane(
    workspace: &Workspace,
    pane: &Entity<Pane>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let workspace_handle = cx.weak_entity();
    pane.update(cx, |pane, cx| {
        pane.toolbar().update(cx, |toolbar, cx| {
            let multibuffer_hint = cx.new(|_| MultibufferHint::new());
            toolbar.add_item(multibuffer_hint, window, cx);
            let solo_diff_style_toolbar = cx.new(SoloDiffStyleToolbar::new);
            toolbar.add_item(solo_diff_style_toolbar, window, cx);
            let breadcrumbs = cx.new(|_| Breadcrumbs::new());
            toolbar.add_item(breadcrumbs, window, cx);
            let buffer_search_bar = cx.new(|cx| {
                search::BufferSearchBar::new(
                    Some(workspace.project().read(cx).languages().clone()),
                    window,
                    cx,
                )
            });
            toolbar.add_item(buffer_search_bar.clone(), window, cx);
            let quick_action_bar =
                cx.new(|cx| QuickActionBar::new(buffer_search_bar, workspace, cx));
            toolbar.add_item(quick_action_bar, window, cx);
            let diagnostic_editor_controls = cx.new(|_| diagnostics::ToolbarControls::new());
            toolbar.add_item(diagnostic_editor_controls, window, cx);
            let project_search_bar = cx.new(|_| ProjectSearchBar::new());
            toolbar.add_item(project_search_bar, window, cx);
            let lsp_log_item = cx.new(|_| LspLogToolbarItemView::new());
            toolbar.add_item(lsp_log_item, window, cx);
            let dap_log_item = cx.new(|_| debugger_tools::DapLogToolbarItemView::new());
            toolbar.add_item(dap_log_item, window, cx);
            let acp_tools_item = cx.new(|_| acp_tools::AcpToolsToolbarItemView::new());
            toolbar.add_item(acp_tools_item, window, cx);
            let telemetry_log_item =
                cx.new(|cx| telemetry_log::TelemetryLogToolbarItemView::new(window, cx));
            toolbar.add_item(telemetry_log_item, window, cx);
            let syntax_tree_item = cx.new(|_| language_tools::SyntaxTreeToolbarItemView::new());
            toolbar.add_item(syntax_tree_item, window, cx);
            let migration_banner =
                cx.new(|inner_cx| MigrationBanner::new(workspace_handle.clone(), inner_cx));
            toolbar.add_item(migration_banner, window, cx);
            let highlights_tree_item =
                cx.new(|_| language_tools::HighlightsTreeToolbarItemView::new());
            toolbar.add_item(highlights_tree_item, window, cx);
            let project_diff_toolbar = cx.new(|cx| ProjectDiffToolbar::new(workspace, cx));
            toolbar.add_item(project_diff_toolbar, window, cx);
            let branch_diff_toolbar = cx.new(BranchDiffToolbar::new);
            toolbar.add_item(branch_diff_toolbar, window, cx);
            let solo_diff_git_toolbar = cx.new(SoloDiffGitToolbar::new);
            toolbar.add_item(solo_diff_git_toolbar, window, cx);
            let commit_view_toolbar = cx.new(|_| CommitViewToolbar::new());
            toolbar.add_item(commit_view_toolbar, window, cx);
            let agent_diff_toolbar = cx.new(AgentDiffToolbar::new);
            toolbar.add_item(agent_diff_toolbar, window, cx);
            let basedpyright_banner = cx.new(|cx| BasedPyrightBanner::new(workspace, cx));
            toolbar.add_item(basedpyright_banner, window, cx);
            let image_view_toolbar = cx.new(|_| image_viewer::ImageViewToolbarControls::new());
            toolbar.add_item(image_view_toolbar, window, cx);
        })
    });
}

fn open_log_file(workspace: &mut Workspace, window: &mut Window, cx: &mut Context<Workspace>) {
    const MAX_LINES: usize = 1000;
    let app_state = workspace.app_state();
    let languages = app_state.languages.clone();
    let fs = app_state.fs.clone();
    cx.spawn_in(window, async move |workspace, cx| {
        let log = {
            let result = futures::join!(
                fs.load(&paths::old_log_file()),
                fs.load(&paths::log_file()),
                languages.language_for_name("log")
            );
            match result {
                (Err(_), Err(e), _) => Err(e),
                (old_log, new_log, lang) => {
                    let mut lines = VecDeque::with_capacity(MAX_LINES);
                    for line in old_log
                        .iter()
                        .flat_map(|log| log.lines())
                        .chain(new_log.iter().flat_map(|log| log.lines()))
                    {
                        if lines.len() == MAX_LINES {
                            lines.pop_front();
                        }
                        lines.push_back(line);
                    }
                    Ok((
                        lines
                            .into_iter()
                            .flat_map(|line| [line, "\n"])
                            .collect::<String>(),
                        lang.ok(),
                    ))
                }
            }
        };

        let (log, log_language) = match log {
            Ok((log, log_language)) => (log, log_language),
            Err(e) => {
                struct OpenLogError;

                workspace
                    .update(cx, |workspace, cx| {
                        workspace.show_notification(
                            NotificationId::unique::<OpenLogError>(),
                            cx,
                            |cx| {
                                cx.new(|cx| {
                                    MessageNotification::new(
                                        format!(
                                            "Unable to access/open log file at path \
                                                    {}: {e:#}",
                                            paths::log_file().display()
                                        ),
                                        cx,
                                    )
                                })
                            },
                        );
                    })
                    .ok();
                return;
            }
        };
        maybe!(async move {
            let project = workspace
                .read_with(cx, |workspace, _| workspace.project().clone())
                .ok()?;
            let buffer = project
                .update(cx, |project, cx| {
                    project.create_buffer(log_language, false, cx)
                })
                .await
                .ok()?;
            buffer.update(cx, |buffer, cx| {
                buffer.set_capability(Capability::ReadOnly, cx);
                buffer.set_text(log, cx);
            });

            let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx).with_title("Log".into()));

            let editor = cx
                .new_window_entity(|window, cx| {
                    let mut editor = Editor::for_multibuffer(buffer, Some(project), window, cx);
                    editor.set_read_only(true);
                    editor.set_breadcrumb_header(format!(
                        "Last {} lines in {}",
                        MAX_LINES,
                        paths::log_file().display()
                    ));
                    let last_multi_buffer_offset = editor.buffer().read(cx).len(cx);
                    editor.change_selections(Default::default(), window, cx, |s| {
                        s.select_ranges(Some(last_multi_buffer_offset..last_multi_buffer_offset));
                    });
                    editor
                })
                .ok()?;

            workspace
                .update_in(cx, |workspace, window, cx| {
                    workspace.add_item_to_active_pane(Box::new(editor), None, true, window, cx);
                })
                .ok()
        })
        .await;
    })
    .detach();
}

#[derive(Copy, Clone, Debug, settings::RegisterSetting)]
struct CursorHideModeSetting(gpui::CursorHideMode);

impl Settings for CursorHideModeSetting {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        Self(match content.hide_mouse.unwrap_or_default() {
            settings::HideMouseMode::Never => gpui::CursorHideMode::Never,
            settings::HideMouseMode::OnTyping => gpui::CursorHideMode::OnTyping,
            settings::HideMouseMode::OnTypingAndAction => gpui::CursorHideMode::OnTypingAndAction,
        })
    }
}

fn init_cursor_hide_mode(cx: &mut App) {
    let apply = |cx: &mut App| cx.set_cursor_hide_mode(CursorHideModeSetting::get_global(cx).0);
    apply(cx);
    cx.observe_global::<SettingsStore>(apply).detach();
}

pub fn open_new_ssh_project_from_project(
    workspace: &mut Workspace,
    paths: Vec<PathBuf>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> Task<anyhow::Result<()>> {
    let app_state = workspace.app_state().clone();
    let Some(ssh_client) = workspace.project().read(cx).remote_client() else {
        return Task::ready(Err(anyhow::anyhow!("Not an ssh project")));
    };
    let connection_options = ssh_client.read(cx).connection_options();
    cx.spawn_in(window, async move |_, cx| {
        open_remote_project(
            connection_options,
            paths,
            app_state,
            workspace::OpenOptions {
                workspace_matching: workspace::WorkspaceMatching::None,
                ..Default::default()
            },
            cx,
        )
        .await
        .map(|_| ())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use assets::Assets;
    use collections::HashSet;
    use editor::{
        DisplayPoint, Editor, MultiBufferOffset, SelectionEffects, display_map::DisplayRow,
    };
    use gpui::{
        Action, AnyWindowHandle, App, AssetSource, BorrowAppContext, Modifiers, TestAppContext,
        UpdateGlobal, VisualTestContext, WindowHandle, actions, point, px,
    };
    use language::LanguageRegistry;
    use languages::{markdown_lang, rust_lang};
    use pretty_assertions::{assert_eq, assert_ne};
    use project::{Project, ProjectPath};
    use prompt_store::PromptBuilder;
    use semver::Version;
    use serde_json::json;
    use settings::{SaturatingBool, SettingsStore, watch_config_file};
    use std::{
        path::{Path, PathBuf},
        sync::Arc,
        time::Duration,
    };
    use theme::ThemeRegistry;
    use util::{
        path,
        rel_path::{RelPath, rel_path},
    };
    use workspace::MultiWorkspace;
    use workspace::{
        NewFile, OpenOptions, OpenVisible, SERIALIZATION_THROTTLE_TIME, SaveIntent, SplitDirection,
        WorkspaceHandle,
        item::SaveOptions,
        item::{Item, ItemHandle},
        open_new, open_paths, pane,
    };

    async fn flush_workspace_serialization(
        window: &WindowHandle<MultiWorkspace>,
        cx: &mut TestAppContext,
    ) {
        let all_tasks = window
            .update(cx, |multi_workspace, window, cx| {
                let mut tasks = multi_workspace
                    .workspaces()
                    .map(|workspace| {
                        workspace.update(cx, |workspace, cx| {
                            workspace.flush_serialization(window, cx)
                        })
                    })
                    .collect::<Vec<_>>();
                tasks.push(multi_workspace.flush_serialization());
                tasks
            })
            .unwrap();

        futures::future::join_all(all_tasks).await;
    }

    #[path = "open_path_tests.rs"]
    mod open_path_tests;

    #[path = "edit_state_tests.rs"]
    mod edit_state_tests;

    #[path = "workspace_open_tests.rs"]
    mod workspace_open_tests;

    #[path = "save_file_tests.rs"]
    mod save_file_tests;

    #[path = "pane_editor_tests.rs"]
    mod pane_editor_tests;

    #[path = "navigation_tests.rs"]
    mod navigation_tests;

    #[gpui::test]
    async fn test_reopening_closed_items(cx: &mut TestAppContext) {
        let app_state = init_test(cx);
        app_state
            .fs
            .as_fake()
            .insert_tree(
                path!("/root"),
                json!({
                    "a": {
                        "file1": "",
                        "file2": "",
                        "file3": "",
                        "file4": "",
                    },
                }),
            )
            .await;

        let project = Project::test(app_state.fs.clone(), [path!("/root").as_ref()], cx).await;
        project.update(cx, |project, _cx| project.languages().add(markdown_lang()));
        let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project, window, cx));
        let workspace = window
            .read_with(cx, |mw, _| mw.workspace().clone())
            .unwrap();
        let cx = &mut VisualTestContext::from_window(*window, cx);
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        let entries = cx.read(|cx| workspace.file_project_paths(cx));
        let file1 = entries[0].clone();
        let file2 = entries[1].clone();
        let file3 = entries[2].clone();
        let file4 = entries[3].clone();

        let file1_item_id = workspace
            .update_in(cx, |w, window, cx| {
                w.open_path(file1.clone(), None, true, window, cx)
            })
            .await
            .unwrap()
            .item_id();
        let file2_item_id = workspace
            .update_in(cx, |w, window, cx| {
                w.open_path(file2.clone(), None, true, window, cx)
            })
            .await
            .unwrap()
            .item_id();
        let file3_item_id = workspace
            .update_in(cx, |w, window, cx| {
                w.open_path(file3.clone(), None, true, window, cx)
            })
            .await
            .unwrap()
            .item_id();
        let file4_item_id = workspace
            .update_in(cx, |w, window, cx| {
                w.open_path(file4.clone(), None, true, window, cx)
            })
            .await
            .unwrap()
            .item_id();
        assert_eq!(active_path(&workspace, cx), Some(file4.clone()));

        // Close all the pane items in some arbitrary order.
        workspace
            .update_in(cx, |_, window, cx| {
                pane.update(cx, |pane, cx| {
                    pane.close_item_by_id(file1_item_id, SaveIntent::Close, window, cx)
                })
            })
            .await
            .unwrap();
        assert_eq!(active_path(&workspace, cx), Some(file4.clone()));

        workspace
            .update_in(cx, |_, window, cx| {
                pane.update(cx, |pane, cx| {
                    pane.close_item_by_id(file4_item_id, SaveIntent::Close, window, cx)
                })
            })
            .await
            .unwrap();
        assert_eq!(active_path(&workspace, cx), Some(file3.clone()));

        workspace
            .update_in(cx, |_, window, cx| {
                pane.update(cx, |pane, cx| {
                    pane.close_item_by_id(file2_item_id, SaveIntent::Close, window, cx)
                })
            })
            .await
            .unwrap();
        assert_eq!(active_path(&workspace, cx), Some(file3.clone()));
        workspace
            .update_in(cx, |_, window, cx| {
                pane.update(cx, |pane, cx| {
                    pane.close_item_by_id(file3_item_id, SaveIntent::Close, window, cx)
                })
            })
            .await
            .unwrap();

        assert_eq!(active_path(&workspace, cx), None);

        // Reopen all the closed items, ensuring they are reopened in the same order
        // in which they were closed.
        workspace
            .update_in(cx, Workspace::reopen_closed_item)
            .await
            .unwrap();
        assert_eq!(active_path(&workspace, cx), Some(file3.clone()));

        workspace
            .update_in(cx, Workspace::reopen_closed_item)
            .await
            .unwrap();
        assert_eq!(active_path(&workspace, cx), Some(file2.clone()));

        workspace
            .update_in(cx, Workspace::reopen_closed_item)
            .await
            .unwrap();
        assert_eq!(active_path(&workspace, cx), Some(file4.clone()));

        workspace
            .update_in(cx, Workspace::reopen_closed_item)
            .await
            .unwrap();
        assert_eq!(active_path(&workspace, cx), Some(file1.clone()));

        // Reopening past the last closed item is a no-op.
        workspace
            .update_in(cx, Workspace::reopen_closed_item)
            .await
            .unwrap();
        assert_eq!(active_path(&workspace, cx), Some(file1.clone()));

        // Reopening closed items doesn't interfere with navigation history.
        // Verify we can navigate back through the history after reopening items.
        workspace
            .update_in(cx, |workspace, window, cx| {
                workspace.go_back(workspace.active_pane().downgrade(), window, cx)
            })
            .await
            .unwrap();

        // After go_back, we should be at a different file than file1
        let after_go_back = active_path(&workspace, cx);
        assert!(
            after_go_back.is_some() && after_go_back != Some(file1.clone()),
            "After go_back from file1, should be at a different file"
        );

        pane.read_with(cx, |pane, _| {
            assert!(pane.can_navigate_forward(), "Should be able to go forward");
        });

        fn active_path(
            workspace: &Entity<Workspace>,
            cx: &VisualTestContext,
        ) -> Option<ProjectPath> {
            workspace.read_with(cx, |workspace, cx| {
                let item = workspace.active_item(cx)?;
                item.project_path(cx)
            })
        }
    }

    fn init_keymap_test(cx: &mut TestAppContext) -> Arc<AppState> {
        cx.update(|cx| {
            let app_state = AppState::test(cx);

            theme_settings::init(theme::LoadThemes::JustBase, cx);
            client::init(&app_state.client, cx);
            workspace::init(app_state.clone(), cx);
            onboarding::init(cx);
            app_state
        })
    }

    actions!(test_only, [ActionA, ActionB]);

    #[gpui::test]
    async fn test_base_keymap(cx: &mut gpui::TestAppContext) {
        let executor = cx.executor();
        let app_state = init_keymap_test(cx);
        let project = Project::test(app_state.fs.clone(), [], cx).await;
        let window =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = window
            .read_with(cx, |mw, _| mw.workspace().clone())
            .unwrap();

        // From the Atom keymap
        use workspace::ActivatePreviousPane;
        // From the JetBrains keymap
        use workspace::ActivatePreviousItem;

        app_state
            .fs
            .save(
                paths::settings_file(),
                &r#"{"base_keymap": "Atom"}"#.into(),
                Default::default(),
            )
            .await
            .unwrap();

        app_state
            .fs
            .save(
                "/keymap.json".as_ref(),
                &r#"[{"bindings": {"backspace": "test_only::ActionA"}}]"#.into(),
                Default::default(),
            )
            .await
            .unwrap();
        executor.run_until_parked();
        cx.update(|cx| {
            let (keymap_rx, keymap_watcher) = watch_config_file(
                &executor,
                app_state.fs.clone(),
                PathBuf::from("/keymap.json"),
            );
            watch_settings_files(app_state.fs.clone(), cx);
            handle_keymap_file_changes(keymap_rx, keymap_watcher, cx);
        });
        window
            .update(cx, |_, _, cx| {
                workspace.update(cx, |workspace, cx| {
                    workspace.register_action(|_, _: &ActionA, _window, _cx| {});
                    workspace.register_action(|_, _: &ActionB, _window, _cx| {});
                    workspace.register_action(|_, _: &ActivatePreviousPane, _window, _cx| {});
                    workspace.register_action(|_, _: &ActivatePreviousItem, _window, _cx| {});
                    cx.notify();
                });
            })
            .unwrap();
        executor.run_until_parked();
        // Test loading the keymap base at all
        assert_key_bindings_for(
            window.into(),
            cx,
            vec![("backspace", &ActionA), ("k", &ActivatePreviousPane)],
            line!(),
        );

        // Test modifying the users keymap, while retaining the base keymap
        app_state
            .fs
            .save(
                "/keymap.json".as_ref(),
                &r#"[{"bindings": {"backspace": "test_only::ActionB"}}]"#.into(),
                Default::default(),
            )
            .await
            .unwrap();

        executor.run_until_parked();

        assert_key_bindings_for(
            window.into(),
            cx,
            vec![("backspace", &ActionB), ("k", &ActivatePreviousPane)],
            line!(),
        );

        // Test modifying the base, while retaining the users keymap
        app_state
            .fs
            .save(
                paths::settings_file(),
                &r#"{"base_keymap": "JetBrains"}"#.into(),
                Default::default(),
            )
            .await
            .unwrap();

        executor.run_until_parked();

        assert_key_bindings_for(
            window.into(),
            cx,
            vec![
                ("backspace", &ActionB),
                ("{", &ActivatePreviousItem::default()),
            ],
            line!(),
        );
    }

    #[gpui::test]
    async fn test_disabled_keymap_binding(cx: &mut gpui::TestAppContext) {
        let executor = cx.executor();
        let app_state = init_keymap_test(cx);
        let project = Project::test(app_state.fs.clone(), [], cx).await;
        let window =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = window
            .read_with(cx, |mw, _| mw.workspace().clone())
            .unwrap();

        // From the Atom keymap
        use workspace::ActivatePreviousPane;
        // From the JetBrains keymap
        use diagnostics::Deploy;

        window
            .update(cx, |_, _, cx| {
                workspace.update(cx, |workspace, cx| {
                    workspace.register_action(|_, _: &ActionA, _window, _cx| {});
                    workspace.register_action(|_, _: &ActionB, _window, _cx| {});
                    workspace.register_action(|_, _: &Deploy, _window, _cx| {});
                    cx.notify();
                });
            })
            .unwrap();
        app_state
            .fs
            .save(
                paths::settings_file(),
                &r#"{"base_keymap": "Atom"}"#.into(),
                Default::default(),
            )
            .await
            .unwrap();
        app_state
            .fs
            .save(
                "/keymap.json".as_ref(),
                &r#"[{"bindings": {"backspace": "test_only::ActionA"}}]"#.into(),
                Default::default(),
            )
            .await
            .unwrap();

        cx.update(|cx| {
            let (keymap_rx, keymap_watcher) = watch_config_file(
                &executor,
                app_state.fs.clone(),
                PathBuf::from("/keymap.json"),
            );

            watch_settings_files(app_state.fs.clone(), cx);
            handle_keymap_file_changes(keymap_rx, keymap_watcher, cx);
        });

        cx.background_executor.run_until_parked();

        cx.background_executor.run_until_parked();
        // Test loading the keymap base at all
        assert_key_bindings_for(
            window.into(),
            cx,
            vec![("backspace", &ActionA), ("k", &ActivatePreviousPane)],
            line!(),
        );

        // Test disabling the key binding for the base keymap
        app_state
            .fs
            .save(
                "/keymap.json".as_ref(),
                &r#"[{"bindings": {"backspace": null}}]"#.into(),
                Default::default(),
            )
            .await
            .unwrap();

        cx.background_executor.run_until_parked();

        assert_key_bindings_for(
            window.into(),
            cx,
            vec![("k", &ActivatePreviousPane)],
            line!(),
        );

        // Test modifying the base, while retaining the users keymap
        app_state
            .fs
            .save(
                paths::settings_file(),
                &r#"{"base_keymap": "JetBrains"}"#.into(),
                Default::default(),
            )
            .await
            .unwrap();

        cx.background_executor.run_until_parked();

        assert_key_bindings_for(window.into(), cx, vec![("6", &Deploy)], line!());
    }

    #[gpui::test]
    async fn test_generate_keymap_json_schema_for_registered_actions(
        cx: &mut gpui::TestAppContext,
    ) {
        init_keymap_test(cx);
        cx.update(|cx| {
            // Make sure it doesn't panic.
            KeymapFile::generate_json_schema_for_registered_actions(cx);
        });
    }

    /// Checks that action namespaces are the expected set. The purpose of this is to prevent typos
    /// and let you know when introducing a new namespace.
    #[gpui::test]
    async fn test_action_namespaces(cx: &mut gpui::TestAppContext) {
        use itertools::Itertools;

        init_keymap_test(cx);
        cx.update(|cx| {
            let all_actions = cx.all_action_names();

            let mut actions_without_namespace = Vec::new();
            let all_namespaces = all_actions
                .iter()
                .filter_map(|action_name| {
                    let namespace = action_name
                        .split("::")
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .skip(1)
                        .rev()
                        .join("::");
                    if namespace.is_empty() {
                        actions_without_namespace.push(*action_name);
                    }
                    if &namespace == "test_only" || &namespace == "stories" {
                        None
                    } else {
                        Some(namespace)
                    }
                })
                .sorted()
                .dedup()
                .collect::<Vec<_>>();
            assert_eq!(actions_without_namespace, Vec::<&str>::new());

            let expected_namespaces = vec![
                "action",
                "activity_indicator",
                "agent",
                "sidebar",
                "app_menu",
                "assistant",
                "assistant2",
                "auto_update",
                "branch_picker",
                "bedrock",
                "branches",
                "buffer_search",
                "channel_modal",
                "cli",
                "client",
                "collab",
                "collab_panel",
                "command_palette",
                "console",
                "context_server",
                "copilot",
                "csv",
                "debug_panel",
                "debugger",
                "dev",
                "diagnostics",
                "edit_prediction",
                "editor",
                "encoding_selector",
                "feedback",
                "file_finder",
                "git",
                "git_graph",
                "git_onboarding",
                "git_panel",
                "git_picker",
                "go_to_line",
                "highlights_tree_view",
                "icon_theme_selector",
                "image_viewer",
                "inline_assistant",
                "journal",
                "keymap_editor",
                "keystroke_input",
                "language_selector",
                "welcome",
                "line_ending_selector",
                "lsp_tool",
                "markdown",
                "menu",
                "multi_workspace",
                "new_process_modal",
                "notebook",
                "onboarding",
                "outline",
                "outline_panel",
                "pane",
                "panel",
                "picker",
                "project_panel",
                "project_search",
                "project_symbols",
                "projects",
                "recent_projects",
                "remote_debug",
                "repl",
                "search",
                "settings_editor",
                "settings_profile_selector",
                "skill_creator",
                "snippets",
                "stash_picker",
                "svg",
                "syntax_tree_view",
                "tab_switcher",
                "task",
                "terminal",
                "terminal_panel",
                "text_finder",
                "theme",
                "theme_selector",
                "toast",
                "toolchain",
                "variable_list",
                "vim",
                "window",
                "workspace",
                "worktree_picker",
                "mav",
                "mav_actions",
                "mav_predict_onboarding",
                "zeta",
            ];
            assert_eq!(
                all_namespaces,
                expected_namespaces
                    .into_iter()
                    .map(|namespace| namespace.to_string())
                    .sorted()
                    .collect::<Vec<_>>()
            );
        });
    }

    #[gpui::test]
    fn test_bundled_settings_and_themes(cx: &mut App) {
        cx.text_system()
            .add_fonts(vec![
                Assets
                    .load("fonts/lilex/Lilex-Regular.ttf")
                    .unwrap()
                    .unwrap(),
                Assets
                    .load("fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf")
                    .unwrap()
                    .unwrap(),
            ])
            .unwrap();
        let themes = ThemeRegistry::default();
        settings::init(cx);
        theme_settings::init(theme::LoadThemes::JustBase, cx);

        let mut has_default_theme = false;
        for theme_name in themes.list().into_iter().map(|meta| meta.name) {
            let theme = themes.get(&theme_name).unwrap();
            assert_eq!(theme.name, theme_name);
            if theme.name.as_ref() == "One Dark" {
                has_default_theme = true;
            }
        }
        assert!(has_default_theme);
    }

    #[gpui::test]
    async fn test_bundled_files_editor(cx: &mut TestAppContext) {
        let app_state = init_test(cx);
        cx.update(init);

        let project = Project::test(app_state.fs.clone(), [], cx).await;
        let _window = cx.add_window(|window, cx| MultiWorkspace::test_new(project, window, cx));

        cx.update(|cx| {
            cx.dispatch_action(&OpenDefaultSettings);
        });
        cx.run_until_parked();

        assert_eq!(cx.read(|cx| cx.windows().len()), 1);

        let multi_workspace = cx.windows()[0].downcast::<MultiWorkspace>().unwrap();
        let active_editor = multi_workspace
            .update(cx, |multi_workspace, _, cx| {
                multi_workspace
                    .workspace()
                    .update(cx, |workspace, cx| workspace.active_item_as::<Editor>(cx))
            })
            .unwrap();
        assert!(
            active_editor.is_some(),
            "Settings action should have opened an editor with the default file contents"
        );

        let active_editor = active_editor.unwrap();
        assert!(
            active_editor.read_with(cx, |editor, cx| editor.read_only(cx)),
            "Default settings should be readonly"
        );
        assert!(
            active_editor.read_with(cx, |editor, cx| editor.buffer().read(cx).read_only()),
            "The underlying buffer should also be readonly for the shipped default settings"
        );
    }

    #[gpui::test]
    async fn test_bundled_files_reuse_existing_editor(cx: &mut TestAppContext) {
        let app_state = init_test(cx);
        cx.update(init);

        let project = Project::test(app_state.fs.clone(), [], cx).await;
        let _window = cx.add_window(|window, cx| MultiWorkspace::test_new(project, window, cx));

        cx.update(|cx| {
            cx.dispatch_action(&OpenDefaultSettings);
        });
        cx.run_until_parked();

        let multi_workspace = cx.windows()[0].downcast::<MultiWorkspace>().unwrap();
        let first_item_id = multi_workspace
            .update(cx, |multi_workspace, _, cx| {
                multi_workspace.workspace().update(cx, |workspace, cx| {
                    workspace
                        .active_item(cx)
                        .expect("default settings should be open")
                        .item_id()
                })
            })
            .unwrap();

        cx.update(|cx| {
            cx.dispatch_action(&OpenDefaultSettings);
        });
        cx.run_until_parked();

        let (second_item_id, item_count) = multi_workspace
            .update(cx, |multi_workspace, _, cx| {
                multi_workspace.workspace().update(cx, |workspace, cx| {
                    let pane = workspace.active_pane().read(cx);
                    (
                        pane.active_item()
                            .expect("default settings should still be open")
                            .item_id(),
                        pane.items_len(),
                    )
                })
            })
            .unwrap();

        assert_eq!(first_item_id, second_item_id);
        assert_eq!(item_count, 1);
    }

    #[gpui::test]
    async fn test_bundled_languages(cx: &mut TestAppContext) {
        let fs = fs::FakeFs::new(cx.background_executor.clone());
        env_logger::builder().is_test(true).try_init().ok();
        let settings = cx.update(SettingsStore::test);
        cx.set_global(settings);
        let languages = LanguageRegistry::test(cx.executor());
        let languages = Arc::new(languages);
        let node_runtime = node_runtime::NodeRuntime::unavailable();
        cx.update(|cx| {
            languages::init(languages.clone(), fs, node_runtime, cx);
        });
        for name in languages.language_names() {
            languages
                .language_for_name(name.as_ref())
                .await
                .with_context(|| format!("language name {name}"))
                .unwrap();
        }
        cx.run_until_parked();
    }

    pub(crate) fn init_test(cx: &mut TestAppContext) -> Arc<AppState> {
        init_test_with_state(cx, cx.update(AppState::test))
    }

    fn init_test_with_state(
        cx: &mut TestAppContext,
        mut app_state: Arc<AppState>,
    ) -> Arc<AppState> {
        cx.update(move |cx| {
            env_logger::builder().is_test(true).try_init().ok();

            let state = Arc::get_mut(&mut app_state).unwrap();
            state.build_window_options = build_window_options;
            app_state.languages.add(markdown_lang());

            gpui_tokio::init(cx);
            AppState::set_global(app_state.clone(), cx);
            theme_settings::init(theme::LoadThemes::JustBase, cx);
            audio::init(cx);
            channel::init(&app_state.client, app_state.user_store.clone(), cx);
            call::init(app_state.client.clone(), app_state.user_store.clone(), cx);
            notifications::init(app_state.client.clone(), app_state.user_store.clone(), cx);
            workspace::init(app_state.clone(), cx);
            release_channel::init(Version::new(0, 0, 0), cx);
            command_palette::init(cx);
            editor::init(cx);
            collab_ui::init(&app_state, cx);
            git_ui::init(cx);
            project_panel::init(cx);
            outline_panel::init(cx);
            terminal_view::init(cx);
            copilot_chat::init(
                app_state.fs.clone(),
                app_state.client.http_client(),
                copilot_chat::CopilotChatConfiguration::default(),
                cx,
            );
            image_viewer::init(cx);
            language_model::init(cx);
            client::RefreshLlmTokenListener::register(
                app_state.client.clone(),
                app_state.user_store.clone(),
                cx,
            );
            language_models::init(app_state.user_store.clone(), app_state.client.clone(), cx);
            web_search::init(cx);
            web_search_providers::init(app_state.client.clone(), app_state.user_store.clone(), cx);
            let prompt_builder = PromptBuilder::load(app_state.fs.clone(), false, cx);
            project::AgentRegistryStore::init_global(
                cx,
                app_state.fs.clone(),
                app_state.client.http_client(),
            );
            agent_ui::init(
                app_state.fs.clone(),
                prompt_builder,
                app_state.languages.clone(),
                true,
                false,
                cx,
            );

            repl::init(app_state.fs.clone(), cx);
            repl::notebook::init(cx);
            tasks_ui::init(cx);
            project::debugger::breakpoint_store::BreakpointStore::init(
                &app_state.client.clone().into(),
            );
            project::debugger::dap_store::DapStore::init(&app_state.client.clone().into(), cx);
            debugger_ui::init(cx);
            initialize_workspace(app_state.clone(), cx);
            search::init(cx);
            cx.set_global(workspace::PaneSearchBarCallbacks {
                setup_search_bar: |languages, toolbar, window, cx| {
                    let search_bar =
                        cx.new(|cx| search::BufferSearchBar::new(languages, window, cx));
                    toolbar.update(cx, |toolbar, cx| {
                        toolbar.add_item(search_bar, window, cx);
                    });
                },
                wrap_div_with_search_actions: search::buffer_search::register_pane_search_actions,
            });
            app_state
        })
    }

    #[track_caller]
    fn assert_key_bindings_for(
        window: AnyWindowHandle,
        cx: &TestAppContext,
        actions: Vec<(&'static str, &dyn Action)>,
        line: u32,
    ) {
        let available_actions = cx
            .update(|cx| window.update(cx, |_, window, cx| window.available_actions(cx)))
            .unwrap();
        for (key, action) in actions {
            let bindings = cx
                .update(|cx| window.update(cx, |_, window, _| window.bindings_for_action(action)))
                .unwrap();
            // assert that...
            assert!(
                available_actions.iter().any(|bound_action| {
                    // actions match...
                    bound_action.partial_eq(action)
                }),
                "On {} Failed to find {}",
                line,
                action.name(),
            );
            assert!(
                // and key strokes contain the given key
                bindings
                    .into_iter()
                    .any(|binding| binding.keystrokes().iter().any(|k| k.key() == key)),
                "On {} Failed to find {} with key binding {}",
                line,
                action.name(),
                key
            );
        }
    }

    #[gpui::test]
    async fn test_opening_project_settings_when_excluded(cx: &mut gpui::TestAppContext) {
        // Use the proper initialization for runtime state
        let app_state = init_keymap_test(cx);

        eprintln!("Running test_opening_project_settings_when_excluded");

        // 1. Set up a project with some project settings
        let settings_init =
            r#"{ "UNIQUEVALUE": true, "git": { "inline_blame": { "enabled": false } } }"#;
        app_state
            .fs
            .as_fake()
            .insert_tree(
                Path::new("/root"),
                json!({
                    ".mav": {
                        "settings.json": settings_init
                    }
                }),
            )
            .await;

        eprintln!("Created project with .mav/settings.json containing UNIQUEVALUE");

        // 2. Create a project with the file system and load it
        let project = Project::test(app_state.fs.clone(), [Path::new("/root")], cx).await;

        // Save original settings content for comparison
        let original_settings = app_state
            .fs
            .load(Path::new("/root/.mav/settings.json"))
            .await
            .unwrap();

        let original_settings_str = original_settings.clone();

        // Verify settings exist on disk and have expected content
        eprintln!("Original settings content: {}", original_settings_str);
        assert!(
            original_settings_str.contains("UNIQUEVALUE"),
            "Test setup failed - settings file doesn't contain our marker"
        );

        // 3. Add .mav to file scan exclusions in user settings
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |worktree_settings| {
                worktree_settings.project.worktree.file_scan_exclusions =
                    Some(vec![".mav".to_string()]);
            });
        });

        eprintln!("Added .mav to file_scan_exclusions in settings");

        // 4. Run tasks to apply settings
        cx.background_executor.run_until_parked();

        // 5. Critical: Verify .mav is actually excluded from worktree
        let worktree = cx.update(|cx| project.read(cx).worktrees(cx).next().unwrap());

        let has_mav_entry =
            cx.update(|cx| worktree.read(cx).entry_for_path(rel_path(".mav")).is_some());

        eprintln!(
            "Is .mav directory visible in worktree after exclusion: {}",
            has_mav_entry
        );

        // This assertion verifies the test is set up correctly to show the bug
        // If .mav is not excluded, the test will fail here
        assert!(
            !has_mav_entry,
            "Test precondition failed: .mav directory should be excluded but was found in worktree"
        );

        // 6. Create workspace and trigger the actual function that causes the bug
        let window =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = window
            .read_with(cx, |mw, _| mw.workspace().clone())
            .unwrap();
        window
            .update(cx, |_, window, cx| {
                workspace.update(cx, |workspace, cx| {
                    // Call the exact function that contains the bug
                    eprintln!("About to call open_project_settings_file");
                    open_project_settings_file(workspace, &OpenProjectSettingsFile, window, cx);
                });
            })
            .unwrap();

        // 7. Run background tasks until completion
        cx.background_executor.run_until_parked();

        // 8. Verify file contents after calling function
        let new_content = app_state
            .fs
            .load(Path::new("/root/.mav/settings.json"))
            .await
            .unwrap();

        let new_content_str = new_content;
        eprintln!("New settings content: {}", new_content_str);

        // The bug causes the settings to be overwritten with empty settings
        // So if the unique value is no longer present, the bug has been reproduced
        let bug_exists = !new_content_str.contains("UNIQUEVALUE");
        eprintln!("Bug reproduced: {}", bug_exists);

        // This assertion should fail if the bug exists - showing the bug is real
        assert!(
            new_content_str.contains("UNIQUEVALUE"),
            "BUG FOUND: Project settings were overwritten when opening via command - original custom content was lost"
        );
    }

    #[gpui::test]
    async fn test_disable_ai_crash(cx: &mut gpui::TestAppContext) {
        let app_state = init_test(cx);
        cx.update(init);
        let project = Project::test(app_state.fs.clone(), [], cx).await;
        let _window = cx.add_window(|window, cx| MultiWorkspace::test_new(project, window, cx));

        cx.run_until_parked();

        cx.update(|cx| {
            SettingsStore::update_global(cx, |settings_store, cx| {
                settings_store.update_user_settings(cx, |settings| {
                    settings.project.disable_ai = Some(SaturatingBool(true));
                });
            });
        });

        cx.run_until_parked();

        // If this panics, the test has failed
    }

    #[gpui::test]
    async fn test_prefer_focused_window(cx: &mut gpui::TestAppContext) {
        let app_state = init_test(cx);
        let paths = [PathBuf::from(path!("/dir/document.txt"))];

        app_state
            .fs
            .as_fake()
            .insert_tree(
                path!("/dir"),
                json!({
                    "document.txt": "Some of the documentation's content."
                }),
            )
            .await;

        let project_a = Project::test(app_state.fs.clone(), [path!("/dir").as_ref()], cx).await;
        let window_a = cx.add_window({
            let project = project_a.clone();
            |window, cx| MultiWorkspace::test_new(project, window, cx)
        });

        let project_b = Project::test(app_state.fs.clone(), [path!("/dir").as_ref()], cx).await;
        let window_b = cx.add_window({
            let project = project_b.clone();
            |window, cx| MultiWorkspace::test_new(project, window, cx)
        });

        let project_c = Project::test(app_state.fs.clone(), [path!("/dir").as_ref()], cx).await;
        let window_c = cx.add_window({
            let project = project_c.clone();
            |window, cx| MultiWorkspace::test_new(project, window, cx)
        });

        for window in [window_a, window_b, window_c] {
            let _ = cx.update_window(*window, |_, window, _| {
                window.activate_window();
            });

            cx.update(|cx| {
                let open_options = OpenOptions {
                    wait: true,
                    ..Default::default()
                };

                workspace::open_paths(&paths, app_state.clone(), open_options, cx)
            })
            .await
            .unwrap();

            cx.update_window(*window, |_, window, _| assert!(window.is_window_active()))
                .unwrap();

            let _ = window.read_with(cx, |multi_workspace, cx| {
                let pane = multi_workspace.workspace().read(cx).active_pane().read(cx);
                let project_path = pane.active_item().unwrap().project_path(cx).unwrap();

                assert_eq!(
                    project_path.path.as_ref().as_std_path().to_str().unwrap(),
                    path!("document.txt")
                )
            });
        }
    }

    #[gpui::test]
    async fn test_open_paths_switches_to_best_workspace(cx: &mut TestAppContext) {
        let app_state = init_test(cx);

        app_state
            .fs
            .as_fake()
            .insert_tree(
                path!("/"),
                json!({
                    "dir1": {
                        "a.txt": "content a"
                    },
                    "dir2": {
                        "b.txt": "content b"
                    },
                    "dir3": {
                        "c.txt": "content c"
                    }
                }),
            )
            .await;

        // Create a window with workspace 0 containing /dir1
        let project1 = Project::test(app_state.fs.clone(), [path!("/dir1").as_ref()], cx).await;

        let window = cx.add_window({
            let project = project1.clone();
            |window, cx| MultiWorkspace::test_new(project, window, cx)
        });
        window
            .update(cx, |multi_workspace, _, cx| {
                multi_workspace.open_sidebar(cx);
            })
            .unwrap();

        cx.run_until_parked();
        assert_eq!(cx.windows().len(), 1, "Should start with 1 window");

        // Create workspace 2 with /dir2
        let project2 = Project::test(app_state.fs.clone(), [path!("/dir2").as_ref()], cx).await;
        let workspace2 = window
            .update(cx, |multi_workspace, window, cx| {
                multi_workspace.test_add_workspace(project2.clone(), window, cx)
            })
            .unwrap();

        // Create workspace 3 with /dir3
        let project3 = Project::test(app_state.fs.clone(), [path!("/dir3").as_ref()], cx).await;
        let workspace3 = window
            .update(cx, |multi_workspace, window, cx| {
                multi_workspace.test_add_workspace(project3.clone(), window, cx)
            })
            .unwrap();

        let workspace1 = window
            .read_with(cx, |multi_workspace, _| {
                multi_workspace.workspaces().next().unwrap().clone()
            })
            .unwrap();

        window
            .update(cx, |multi_workspace, window, cx| {
                multi_workspace.activate(workspace2.clone(), None, window, cx);
                multi_workspace.activate(workspace3.clone(), None, window, cx);
                // Switch back to workspace1 for test setup
                multi_workspace.activate(workspace1.clone(), None, window, cx);
                assert_eq!(multi_workspace.workspace(), &workspace1);
            })
            .unwrap();

        cx.run_until_parked();

        // Verify setup: 3 workspaces, workspace 0 active, still 1 window
        window
            .read_with(cx, |multi_workspace, _| {
                assert_eq!(multi_workspace.workspaces().count(), 3);
                assert_eq!(multi_workspace.workspace(), &workspace1);
            })
            .unwrap();
        assert_eq!(cx.windows().len(), 1);

        // Open a file in /dir3 - should switch to workspace 3 (not just "the other one")
        cx.update(|cx| {
            open_paths(
                &[PathBuf::from(path!("/dir3/c.txt"))],
                app_state.clone(),
                OpenOptions::default(),
                cx,
            )
        })
        .await
        .unwrap();

        cx.run_until_parked();

        // Verify workspace 2 is active and file opened there
        window
            .read_with(cx, |multi_workspace, cx| {
                assert_eq!(
                    multi_workspace.workspace(),
                    &workspace3,
                    "Should have switched to workspace 3 which contains /dir3"
                );
                let active_item = multi_workspace
                    .workspace()
                    .read(cx)
                    .active_pane()
                    .read(cx)
                    .active_item()
                    .expect("Should have an active item");
                assert_eq!(active_item.tab_content_text(0, cx), "c.txt");
            })
            .unwrap();
        assert_eq!(cx.windows().len(), 1, "Should reuse existing window");

        // Open a file in /dir2 - should switch to workspace 2
        cx.update(|cx| {
            open_paths(
                &[PathBuf::from(path!("/dir2/b.txt"))],
                app_state.clone(),
                OpenOptions::default(),
                cx,
            )
        })
        .await
        .unwrap();

        cx.run_until_parked();

        // Verify workspace 1 is active and file opened there
        window
            .read_with(cx, |multi_workspace, cx| {
                assert_eq!(
                    multi_workspace.workspace(),
                    &workspace2,
                    "Should have switched to workspace 2 which contains /dir2"
                );
                let active_item = multi_workspace
                    .workspace()
                    .read(cx)
                    .active_pane()
                    .read(cx)
                    .active_item()
                    .expect("Should have an active item");
                assert_eq!(active_item.tab_content_text(0, cx), "b.txt");
            })
            .unwrap();

        // Verify c.txt is still in workspace 3 (file opened in correct workspace, not active one)
        workspace3.read_with(cx, |workspace, cx| {
            let active_item = workspace
                .active_pane()
                .read(cx)
                .active_item()
                .expect("Workspace 2 should have an active item");
            assert_eq!(
                active_item.tab_content_text(0, cx),
                "c.txt",
                "c.txt should have been opened in workspace 3, not the active workspace"
            );
        });

        assert_eq!(cx.windows().len(), 1, "Should still have only 1 window");

        // Open a file in /dir1 - should switch back to workspace 0
        cx.update(|cx| {
            open_paths(
                &[PathBuf::from(path!("/dir1/a.txt"))],
                app_state.clone(),
                OpenOptions::default(),
                cx,
            )
        })
        .await
        .unwrap();

        cx.run_until_parked();

        // Verify workspace 0 is active and file opened there
        window
            .read_with(cx, |multi_workspace, cx| {
                assert_eq!(
                    multi_workspace.workspace(),
                    &workspace1,
                    "Should have switched back to workspace 0 which contains /dir1"
                );
                let active_item = multi_workspace
                    .workspace()
                    .read(cx)
                    .active_pane()
                    .read(cx)
                    .active_item()
                    .expect("Should have an active item");
                assert_eq!(active_item.tab_content_text(0, cx), "a.txt");
            })
            .unwrap();
        assert_eq!(cx.windows().len(), 1, "Should still have only 1 window");
    }

    #[gpui::test]
    async fn test_quit_checks_all_workspaces_for_dirty_items(cx: &mut TestAppContext) {
        let app_state = init_test(cx);
        cx.update(init);

        app_state
            .fs
            .as_fake()
            .insert_tree(
                path!("/"),
                json!({
                    "dir1": {
                        "a.txt": "content a"
                    },
                    "dir2": {
                        "b.txt": "content b"
                    },
                    "dir3": {
                        "c.txt": "content c"
                    }
                }),
            )
            .await;

        // === Setup Window 1 with two workspaces ===
        let project1 = Project::test(app_state.fs.clone(), [path!("/dir1").as_ref()], cx).await;
        let window1 = cx.add_window({
            let project = project1.clone();
            |window, cx| MultiWorkspace::test_new(project, window, cx)
        });
        window1
            .update(cx, |multi_workspace, _, cx| {
                multi_workspace.open_sidebar(cx);
            })
            .unwrap();

        cx.run_until_parked();

        let project2 = Project::test(app_state.fs.clone(), [path!("/dir2").as_ref()], cx).await;
        let workspace1_1 = window1
            .read_with(cx, |multi_workspace, _| multi_workspace.workspace().clone())
            .unwrap();
        let workspace1_2 = window1
            .update(cx, |multi_workspace, window, cx| {
                multi_workspace.test_add_workspace(project2.clone(), window, cx)
            })
            .unwrap();

        window1
            .update(cx, |multi_workspace, window, cx| {
                multi_workspace.activate(workspace1_2.clone(), None, window, cx);
                multi_workspace.activate(workspace1_1.clone(), None, window, cx);
            })
            .unwrap();

        // === Setup Window 2 with one workspace ===
        let project3 = Project::test(app_state.fs.clone(), [path!("/dir3").as_ref()], cx).await;
        let window2 = cx.add_window({
            let project = project3.clone();
            |window, cx| MultiWorkspace::test_new(project, window, cx)
        });
        window2
            .update(cx, |multi_workspace, _, cx| {
                multi_workspace.open_sidebar(cx);
            })
            .unwrap();

        cx.run_until_parked();
        assert_eq!(cx.windows().len(), 2);

        // === Case 1: Active workspace has dirty item, quit can be cancelled ===
        let worktree1_id = project1.update(cx, |project, cx| {
            project.worktrees(cx).next().unwrap().read(cx).id()
        });

        let editor1 = window1
            .update(cx, |_, window, cx| {
                workspace1_1.update(cx, |workspace, cx| {
                    workspace.open_path((worktree1_id, rel_path("a.txt")), None, true, window, cx)
                })
            })
            .unwrap()
            .await
            .unwrap()
            .downcast::<Editor>()
            .unwrap();

        window1
            .update(cx, |_, window, cx| {
                editor1.update(cx, |editor, cx| {
                    editor.insert("dirty in active workspace", window, cx);
                });
            })
            .unwrap();

        cx.run_until_parked();

        // Verify workspace1_1 is active
        window1
            .read_with(cx, |multi_workspace, _| {
                assert_eq!(multi_workspace.workspace(), &workspace1_1);
            })
            .unwrap();

        cx.dispatch_action(*window1, Quit);
        cx.run_until_parked();

        assert!(
            cx.has_pending_prompt(),
            "Case 1: Should prompt to save dirty item in active workspace"
        );

        cx.simulate_prompt_answer("Cancel");
        cx.run_until_parked();

        assert_eq!(
            cx.windows().len(),
            2,
            "Case 1: Windows should still exist after cancelling quit"
        );

        // Clean up Case 1: Close the dirty item without saving
        let close_task = window1
            .update(cx, |_, window, cx| {
                workspace1_1.update(cx, |workspace, cx| {
                    workspace.active_pane().update(cx, |pane, cx| {
                        pane.close_active_item(&Default::default(), window, cx)
                    })
                })
            })
            .unwrap();
        cx.run_until_parked();
        cx.simulate_prompt_answer("Don't Save");
        close_task.await.ok();
        cx.run_until_parked();

        // === Case 2: Non-active workspace (same window) has dirty item ===
        let worktree2_id = project2.update(cx, |project, cx| {
            project.worktrees(cx).next().unwrap().read(cx).id()
        });

        let editor2 = window1
            .update(cx, |_, window, cx| {
                workspace1_2.update(cx, |workspace, cx| {
                    workspace.open_path((worktree2_id, rel_path("b.txt")), None, true, window, cx)
                })
            })
            .unwrap()
            .await
            .unwrap()
            .downcast::<Editor>()
            .unwrap();

        window1
            .update(cx, |_, window, cx| {
                editor2.update(cx, |editor, cx| {
                    editor.insert("dirty in non-active workspace", window, cx);
                });
            })
            .unwrap();

        cx.run_until_parked();

        // Verify workspace1_1 is still active (not workspace1_2 with dirty item)
        window1
            .read_with(cx, |multi_workspace, _| {
                assert_eq!(multi_workspace.workspace(), &workspace1_1);
            })
            .unwrap();

        cx.dispatch_action(*window1, Quit);
        cx.run_until_parked();

        // Verify the non-active workspace got activated to show the dirty item
        window1
            .read_with(cx, |multi_workspace, _| {
                assert_eq!(
                    multi_workspace.workspace(),
                    &workspace1_2,
                    "Case 2: Non-active workspace should be activated when it has dirty item"
                );
            })
            .unwrap();

        assert!(
            cx.has_pending_prompt(),
            "Case 2: Should prompt to save dirty item in non-active workspace"
        );

        cx.simulate_prompt_answer("Cancel");
        cx.run_until_parked();

        assert_eq!(
            cx.windows().len(),
            2,
            "Case 2: Windows should still exist after cancelling quit"
        );

        // Clean up Case 2: Close the dirty item without saving
        let close_task = window1
            .update(cx, |_, window, cx| {
                workspace1_2.update(cx, |workspace, cx| {
                    workspace.active_pane().update(cx, |pane, cx| {
                        pane.close_active_item(&Default::default(), window, cx)
                    })
                })
            })
            .unwrap();
        cx.run_until_parked();
        cx.simulate_prompt_answer("Don't Save");
        close_task.await.ok();
        cx.run_until_parked();

        // === Case 3: Non-active window has dirty item ===
        let workspace3 = window2
            .read_with(cx, |multi_workspace, _| multi_workspace.workspace().clone())
            .unwrap();

        let worktree3_id = project3.update(cx, |project, cx| {
            project.worktrees(cx).next().unwrap().read(cx).id()
        });

        let editor3 = window2
            .update(cx, |_, window, cx| {
                workspace3.update(cx, |workspace, cx| {
                    workspace.open_path((worktree3_id, rel_path("c.txt")), None, true, window, cx)
                })
            })
            .unwrap()
            .await
            .unwrap()
            .downcast::<Editor>()
            .unwrap();

        window2
            .update(cx, |_, window, cx| {
                editor3.update(cx, |editor, cx| {
                    editor.insert("dirty in other window", window, cx);
                });
            })
            .unwrap();

        cx.run_until_parked();

        // Activate window1 explicitly (editing in window2 may have activated it)
        window1
            .update(cx, |_, window, _| window.activate_window())
            .unwrap();
        cx.run_until_parked();

        // Verify window2 is not active (window1 should still be active)
        assert_eq!(
            cx.update(|cx| window2.is_active(cx)),
            Some(false),
            "Case 3: window2 should not be active before quit"
        );

        // Dispatch quit from window1 (window2 has the dirty item)
        cx.dispatch_action(*window1, Quit);
        cx.run_until_parked();

        // Verify window2 is now active (quit handler activated it to show dirty item)
        assert_eq!(
            cx.update(|cx| window2.is_active(cx)),
            Some(true),
            "Case 3: window2 should be activated when it has dirty item"
        );

        assert!(
            cx.has_pending_prompt(),
            "Case 3: Should prompt to save dirty item in non-active window"
        );

        cx.simulate_prompt_answer("Cancel");
        cx.run_until_parked();

        assert_eq!(
            cx.windows().len(),
            2,
            "Case 3: Windows should still exist after cancelling quit"
        );
    }

    #[gpui::test]
    async fn test_multi_workspace_session_restore(cx: &mut TestAppContext) {
        use collections::HashMap;
        use session::Session;
        use util::path_list::PathList;
        use workspace::{OpenMode, ProjectGroupKey, Workspace, WorkspaceId};

        let app_state = init_test(cx);

        let dir1 = path!("/dir1");
        let dir2 = path!("/dir2");
        let dir3 = path!("/dir3");

        let fs = app_state.fs.clone();
        let fake_fs = fs.as_fake();
        fake_fs.insert_tree(dir1, json!({})).await;
        fake_fs.insert_tree(dir2, json!({})).await;
        fake_fs.insert_tree(dir3, json!({})).await;

        let session_id = cx.read(|cx| app_state.session.read(cx).id().to_owned());

        // --- Create 3 workspaces in 2 windows ---
        //
        //   Window A: workspace for dir1, workspace for dir2
        //   Window B: workspace for dir3
        let workspace::OpenResult {
            window: window_a, ..
        } = cx
            .update(|cx| {
                Workspace::new_local(
                    vec![dir1.into()],
                    app_state.clone(),
                    None,
                    None,
                    None,
                    OpenMode::Activate,
                    cx,
                )
            })
            .await
            .expect("failed to open first workspace");

        window_a
            .update(cx, |multi_workspace, _, cx| {
                multi_workspace.open_sidebar(cx);
            })
            .unwrap();

        window_a
            .update(cx, |multi_workspace, window, cx| {
                multi_workspace.open_project(vec![dir2.into()], OpenMode::Activate, window, cx)
            })
            .unwrap()
            .await
            .expect("failed to open second workspace into window A");
        cx.run_until_parked();

        let workspace::OpenResult {
            window: window_b, ..
        } = cx
            .update(|cx| {
                Workspace::new_local(
                    vec![dir3.into()],
                    app_state.clone(),
                    None,
                    None,
                    None,
                    OpenMode::Activate,
                    cx,
                )
            })
            .await
            .expect("failed to open third workspace");

        window_b
            .update(cx, |multi_workspace, _, cx| {
                multi_workspace.open_sidebar(cx);
            })
            .unwrap();

        // Currently dir2 is active because it was added last.
        // So, switch window_a's active workspace to dir1 (index 0).
        // This sets up a non-trivial assertion: after restore, dir1 should
        // still be active rather than whichever workspace happened to restore last.
        window_a
            .update(cx, |multi_workspace, window, cx| {
                let workspace = multi_workspace.workspaces().next().unwrap().clone();
                multi_workspace.activate(workspace, None, window, cx);
            })
            .unwrap();

        cx.run_until_parked();
        flush_workspace_serialization(&window_a, cx).await;
        flush_workspace_serialization(&window_b, cx).await;
        cx.run_until_parked();

        // Verify all workspaces retained their session_ids.
        let db = cx.update(|cx| workspace::WorkspaceDb::global(cx));
        let locations =
            workspace::last_session_workspace_locations(&db, &session_id, None, fs.as_ref())
                .await
                .expect("expected session workspace locations");
        assert_eq!(
            locations.len(),
            3,
            "all 3 workspaces should have session_ids in the DB"
        );

        // Close the original windows.
        window_a
            .update(cx, |_, window, _| window.remove_window())
            .unwrap();
        window_b
            .update(cx, |_, window, _| window.remove_window())
            .unwrap();
        cx.run_until_parked();

        // Simulate a new session launch: replace the session so that
        // `last_session_id()` returns the ID used during workspace creation.
        // `restore_on_startup` defaults to `LastSession`, which is what we need.
        cx.update(|cx| {
            app_state.session.update(cx, |app_session, _cx| {
                app_session
                    .replace_session_for_test(Session::test_with_old_session(session_id.clone()));
            });
        });

        // --- Read back from DB and verify grouping ---
        let locations =
            workspace::last_session_workspace_locations(&db, &session_id, None, fs.as_ref())
                .await
                .expect("expected session workspace locations");

        assert_eq!(locations.len(), 3, "expected 3 session workspaces");

        let mut groups_by_window: HashMap<gpui::WindowId, Vec<WorkspaceId>> = HashMap::default();
        for session_workspace in &locations {
            if let Some(window_id) = session_workspace.window_id {
                groups_by_window
                    .entry(window_id)
                    .or_default()
                    .push(session_workspace.workspace_id);
            }
        }
        assert_eq!(
            groups_by_window.len(),
            2,
            "expected 2 window groups, got {groups_by_window:?}"
        );
        assert!(
            groups_by_window.values().any(|g| g.len() == 2),
            "expected one group with 2 workspaces"
        );
        assert!(
            groups_by_window.values().any(|g| g.len() == 1),
            "expected one group with 1 workspace"
        );

        let mut async_cx = cx.to_async();
        crate::restore_or_create_workspace(app_state.clone(), &mut async_cx)
            .await
            .expect("failed to restore workspaces");
        cx.run_until_parked();

        // --- Verify the restored windows ---
        let restored_windows: Vec<WindowHandle<MultiWorkspace>> = cx.read(|cx| {
            cx.windows()
                .into_iter()
                .filter_map(|window| window.downcast::<MultiWorkspace>())
                .collect()
        });
        assert_eq!(restored_windows.len(), 2,);

        // Identify restored windows by their active workspace root paths.
        let (restored_a, restored_b) = {
            let (mut with_dir1, mut with_dir3) = (None, None);
            for window in &restored_windows {
                let active_paths = window
                    .read_with(cx, |mw, cx| mw.workspace().read(cx).root_paths(cx))
                    .unwrap();
                if active_paths.iter().any(|p| p.as_ref() == Path::new(dir1)) {
                    with_dir1 = Some(window);
                } else {
                    with_dir3 = Some(window);
                }
            }
            (
                with_dir1.expect("expected a window with dir1 active"),
                with_dir3.expect("expected a window with dir3 active"),
            )
        };

        // Window A (dir1+dir2): 1 workspace restored, but 2 project group keys.
        restored_a
            .read_with(cx, |mw, _| {
                assert_eq!(
                    mw.project_group_keys(),
                    vec![
                        ProjectGroupKey::new(None, PathList::new(&[dir2])),
                        ProjectGroupKey::new(None, PathList::new(&[dir1])),
                    ]
                );
                assert_eq!(mw.workspaces().count(), 1);
            })
            .unwrap();

        // Window B (dir3): 1 workspace, 1 project group key.
        restored_b
            .read_with(cx, |mw, _| {
                assert_eq!(
                    mw.project_group_keys(),
                    vec![ProjectGroupKey::new(None, PathList::new(&[dir3]))]
                );
                assert_eq!(mw.workspaces().count(), 1);
            })
            .unwrap();
    }

    #[gpui::test]
    async fn test_quit_preserves_focused_workspace_for_restore(cx: &mut TestAppContext) {
        use session::Session;
        use workspace::{OpenMode, Workspace};

        let app_state = init_test(cx);
        cx.update(init);

        let dir1 = path!("/dir1");
        let dir2 = path!("/dir2");

        let fs = app_state.fs.clone();
        let fake_fs = fs.as_fake();
        fake_fs.insert_tree(dir1, json!({})).await;
        fake_fs.insert_tree(dir2, json!({})).await;

        let session_id = cx.read(|cx| app_state.session.read(cx).id().to_owned());

        // Window with two retained workspaces: dir1 added first, dir2 second.
        let workspace::OpenResult { window, .. } = cx
            .update(|cx| {
                Workspace::new_local(
                    vec![dir1.into()],
                    app_state.clone(),
                    None,
                    None,
                    None,
                    OpenMode::Activate,
                    cx,
                )
            })
            .await
            .expect("failed to open first workspace");

        window
            .update(cx, |multi_workspace, _, cx| {
                multi_workspace.open_sidebar(cx);
            })
            .unwrap();

        window
            .update(cx, |multi_workspace, window, cx| {
                multi_workspace.open_project(vec![dir2.into()], OpenMode::Activate, window, cx)
            })
            .unwrap()
            .await
            .expect("failed to open second workspace");
        cx.run_until_parked();

        // Focus dir1 (the first workspace). dir2 was activated last when it was
        // opened and is iterated last by the quit-time close-prompt loop, so
        // without the fix the persisted active workspace gets clobbered to dir2.
        window
            .update(cx, |multi_workspace, window, cx| {
                let workspace = multi_workspace.workspaces().next().unwrap().clone();
                multi_workspace.activate(workspace, None, window, cx);
            })
            .unwrap();
        cx.run_until_parked();

        window
            .read_with(cx, |mw, cx| {
                assert!(
                    mw.workspace()
                        .read(cx)
                        .root_paths(cx)
                        .iter()
                        .any(|p| p.as_ref() == Path::new(dir1)),
                    "dir1 should be the focused workspace before quitting"
                );
            })
            .unwrap();

        // Quit. With no dirty items there are no save prompts, so the quit flow
        // runs the prepare_to_close loop (which activates every workspace in
        // turn to surface prompts) and then flushes serialization. cx.quit() is
        // a no-op in tests, so the window stays around for inspection.
        cx.dispatch_action(*window, Quit);
        cx.run_until_parked();

        // The fix re-activates the originally-focused workspace after the loop,
        // so the window must still be focused on dir1, not dir2.
        window
            .read_with(cx, |mw, cx| {
                let active = mw.workspace().read(cx).root_paths(cx);
                assert!(
                    active.iter().any(|p| p.as_ref() == Path::new(dir1)),
                    "quitting must not change which workspace is focused"
                );
                assert!(
                    !active.iter().any(|p| p.as_ref() == Path::new(dir2)),
                    "dir2 must not become the focused workspace after quitting"
                );
            })
            .unwrap();

        // Simulate a fresh launch and verify dir1 is restored as the active
        // workspace rather than dir2 (or an empty window).
        window
            .update(cx, |_, window, _| window.remove_window())
            .unwrap();
        cx.run_until_parked();

        cx.update(|cx| {
            app_state.session.update(cx, |app_session, _cx| {
                app_session
                    .replace_session_for_test(Session::test_with_old_session(session_id.clone()));
            });
        });

        let mut async_cx = cx.to_async();
        crate::restore_or_create_workspace(app_state.clone(), &mut async_cx)
            .await
            .expect("failed to restore workspaces");
        cx.run_until_parked();

        let restored_windows: Vec<WindowHandle<MultiWorkspace>> = cx.read(|cx| {
            cx.windows()
                .into_iter()
                .filter_map(|window| window.downcast::<MultiWorkspace>())
                .collect()
        });
        assert_eq!(restored_windows.len(), 1);

        restored_windows[0]
            .read_with(cx, |mw, cx| {
                let active = mw.workspace().read(cx).root_paths(cx);
                assert!(
                    active.iter().any(|p| p.as_ref() == Path::new(dir1)),
                    "the focused workspace (dir1) must be restored as active"
                );
                assert!(
                    !active.iter().any(|p| p.as_ref() == Path::new(dir2)),
                    "dir2 must not be restored as the active workspace"
                );
            })
            .unwrap();
    }

    #[gpui::test]
    async fn test_restored_project_groups_survive_workspace_key_change(cx: &mut TestAppContext) {
        use session::Session;
        use util::path_list::PathList;
        use workspace::{OpenMode, ProjectGroupKey};

        let app_state = init_test(cx);

        let fs = app_state.fs.clone();
        let fake_fs = fs.as_fake();
        fake_fs
            .insert_tree(path!("/root_a"), json!({ "file.txt": "" }))
            .await;
        fake_fs
            .insert_tree(path!("/root_b"), json!({ "file.txt": "" }))
            .await;
        fake_fs
            .insert_tree(path!("/root_c"), json!({ "file.txt": "" }))
            .await;
        fake_fs
            .insert_tree(path!("/root_d"), json!({ "other.txt": "" }))
            .await;

        let session_id = cx.read(|cx| app_state.session.read(cx).id().to_owned());

        // --- Phase 1: Build a multi-workspace with 3 project groups ---

        let workspace::OpenResult { window, .. } = cx
            .update(|cx| {
                workspace::Workspace::new_local(
                    vec![path!("/root_a").into()],
                    app_state.clone(),
                    None,
                    None,
                    None,
                    OpenMode::Activate,
                    cx,
                )
            })
            .await
            .expect("failed to open workspace");

        window.update(cx, |mw, _, cx| mw.open_sidebar(cx)).unwrap();

        window
            .update(cx, |mw, window, cx| {
                mw.open_project(vec![path!("/root_b").into()], OpenMode::Add, window, cx)
            })
            .unwrap()
            .await
            .expect("failed to add root_b");

        window
            .update(cx, |mw, window, cx| {
                mw.open_project(vec![path!("/root_c").into()], OpenMode::Add, window, cx)
            })
            .unwrap()
            .await
            .expect("failed to add root_c");
        cx.run_until_parked();

        let key_b = ProjectGroupKey::new(None, PathList::new(&[path!("/root_b")]));
        let key_c = ProjectGroupKey::new(None, PathList::new(&[path!("/root_c")]));

        // Make root_a the active workspace so it's the one eagerly restored.
        window
            .update(cx, |mw, window, cx| {
                let workspace_a = mw
                    .workspaces()
                    .find(|ws| {
                        ws.read(cx)
                            .root_paths(cx)
                            .iter()
                            .any(|p| p.as_ref() == Path::new(path!("/root_a")))
                    })
                    .expect("workspace_a should exist")
                    .clone();
                mw.activate(workspace_a, None, window, cx);
            })
            .unwrap();
        cx.run_until_parked();

        // --- Phase 2: Serialize, close, and restore ---

        flush_workspace_serialization(&window, cx).await;
        cx.run_until_parked();

        window
            .update(cx, |_, window, _| window.remove_window())
            .unwrap();
        cx.run_until_parked();

        cx.update(|cx| {
            app_state.session.update(cx, |app_session, _cx| {
                app_session
                    .replace_session_for_test(Session::test_with_old_session(session_id.clone()));
            });
        });

        let mut async_cx = cx.to_async();
        crate::restore_or_create_workspace(app_state.clone(), &mut async_cx)
            .await
            .expect("failed to restore workspace");
        cx.run_until_parked();

        let restored_windows: Vec<WindowHandle<MultiWorkspace>> = cx.read(|cx| {
            cx.windows()
                .into_iter()
                .filter_map(|w| w.downcast::<MultiWorkspace>())
                .collect()
        });
        assert_eq!(restored_windows.len(), 1);
        let restored = &restored_windows[0];

        // Verify the restored window has all 3 project groups.
        restored
            .read_with(cx, |mw, _cx| {
                let keys = mw.project_group_keys();
                assert_eq!(
                    keys.len(),
                    3,
                    "restored window should have 3 groups; got {keys:?}"
                );
                assert!(keys.contains(&key_b), "should contain key_b");
                assert!(keys.contains(&key_c), "should contain key_c");
            })
            .unwrap();

        // --- Phase 3: Trigger a workspace key change and verify survival ---

        let active_project = restored
            .read_with(cx, |mw, cx| mw.workspace().read(cx).project().clone())
            .unwrap();

        active_project
            .update(cx, |project, cx| {
                project.find_or_create_worktree(path!("/root_d"), true, cx)
            })
            .await
            .expect("adding worktree should succeed");
        cx.run_until_parked();

        restored
            .read_with(cx, |mw, _cx| {
                let keys = mw.project_group_keys();
                assert!(
                    keys.contains(&key_b),
                    "restored group key_b should survive a workspace key change; got {keys:?}"
                );
                assert!(
                    keys.contains(&key_c),
                    "restored group key_c should survive a workspace key change; got {keys:?}"
                );
            })
            .unwrap();
    }

    #[gpui::test]
    async fn test_close_project_removes_project_group(cx: &mut TestAppContext) {
        use util::path_list::PathList;
        use workspace::{OpenMode, ProjectGroupKey};

        let app_state = init_test(cx);
        app_state
            .fs
            .as_fake()
            .insert_tree(path!("/my-project"), json!({}))
            .await;

        let workspace::OpenResult { window, .. } = cx
            .update(|cx| {
                workspace::Workspace::new_local(
                    vec![path!("/my-project").into()],
                    app_state.clone(),
                    None,
                    None,
                    None,
                    OpenMode::Activate,
                    cx,
                )
            })
            .await
            .unwrap();

        window.update(cx, |mw, _, cx| mw.open_sidebar(cx)).unwrap();
        cx.background_executor.run_until_parked();

        let project_key = ProjectGroupKey::new(None, PathList::new(&[path!("/my-project")]));
        let keys = window
            .read_with(cx, |mw, _| mw.project_group_keys())
            .unwrap();
        assert_eq!(
            keys,
            vec![project_key],
            "project group should exist before CloseProject: {keys:?}"
        );

        cx.dispatch_action(window.into(), CloseProject);

        let keys = window
            .read_with(cx, |mw, _| mw.project_group_keys())
            .unwrap();
        assert!(
            keys.is_empty(),
            "project group should be removed after CloseProject: {keys:?}"
        );
    }
}
