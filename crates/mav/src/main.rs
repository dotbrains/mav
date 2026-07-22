// Disable command line from opening on release mode
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_runtime;
mod mav;
mod reliability;
mod startup_cli;
mod startup_diagnostics;
mod startup_open;
mod startup_theme;
mod workspace_restore;

pub(crate) use startup_cli::{Args, FORCE_CLI_MODE, parse_url_arg, stdout_is_a_pty};
use startup_diagnostics::{
    check_for_conpty_dll, dump_all_gpui_actions, fail_to_open_window, fail_to_open_window_async,
    files_not_created_on_launch,
};
pub(crate) use startup_open::handle_open_request;
#[cfg(debug_assertions)]
use startup_theme::watch_languages;
use startup_theme::{load_embedded_fonts, load_user_themes_in_background, watch_themes};
pub(crate) use workspace_restore::{restorable_workspace_locations, restore_or_create_workspace};

// Ensure the binary name stays in sync with APP_NAME so that the paths used
// at runtime (data dir, config dir, etc.) match what the binary is called.
const _: () = assert!(
    paths::APP_NAME_LOWERCASE
        .as_bytes()
        .eq_ignore_ascii_case(env!("CARGO_BIN_NAME").as_bytes()),
    "paths::APP_NAME_LOWERCASE must match the binary name. \
     Forks: update APP_NAME in crates/paths/src/paths.rs when renaming the binary.",
);

use agent_ui::AgentPanel;
use anyhow::{Context as _, Result};
use clap::Parser;
use cli::FORCE_CLI_MODE_ENV_VAR_NAME;
use client::{Client, ProxySettings, RefreshLlmTokenListener, UserStore, parse_mav_link};
use collab_ui::channel_view::ChannelView;
use collections::HashMap;
use crashes::InitCrashHandler;
use db::kvp::{GlobalKeyValueStore, KeyValueStore};
use editor::Editor;
use extension::ExtensionHostProxy;
use fs::{Fs, RealFs};
use futures::{StreamExt, channel::oneshot, future};
use git::GitHostingProviderRegistry;
use git_ui::clone::clone_and_open;
use gpui::{
    App, AppContext, Application, AsyncApp, QuitMode, Task, TaskExt, UpdateGlobal as _, block_on,
};
use gpui_platform;

use gpui_tokio::Tokio;
use language::LanguageRegistry;
use onboarding::{FIRST_OPEN, show_onboarding_view};
use project_panel::ProjectPanel;
use prompt_store::PromptBuilder;
use remote::RemoteConnectionOptions;
use reqwest_client::ReqwestClient;

use assets::Assets;
use mav::{
    OpenListener, OpenRequest, RawOpenRequest, app_menus, build_window_options,
    derive_paths_with_position, edit_prediction_registry, handle_cli_connection,
    handle_keymap_file_changes, initialize_workspace, open_paths_with_positions,
};
use node_runtime::{NodeBinaryOptions, NodeRuntime};
use parking_lot::Mutex;
use project::{project_settings::ProjectSettings, trusted_worktrees};
use recent_projects::{RemoteSettings, open_remote_project};
use release_channel::{AppCommitSha, AppVersion, ReleaseChannel};
use session::{AppSession, Session};
use settings::{BaseKeymap, Settings, SettingsStore, watch_config_file};
use smol::future::poll_once;
use std::{
    cell::RefCell,
    env,
    io::{self, IsTerminal},
    path::{Path, PathBuf},
    process,
    rc::Rc,
    sync::{Arc, LazyLock, OnceLock},
    time::Instant,
};
use theme::{ActiveTheme, GlobalTheme, ThemeRegistry};
use theme_settings::load_user_theme;
use util::{ResultExt, maybe};
use uuid::Uuid;
use workspace::{
    AppState, MultiWorkspace, SerializedWorkspaceLocation, SessionWorkspace, Toast,
    WorkspaceSettings, WorkspaceStore,
    notifications::{NotificationId, NotifyResultExt},
    restore_multiworkspace,
};

use crate::mav::{CrashHandler, OpenRequestKind, eager_load_active_theme_and_icon_theme};

#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn build_application() -> Application {
    let platform = gpui_platform::current_platform(false);
    if std::env::var("MAV_EXPERIMENTAL_A11Y").as_deref() == Ok("1") {
        Application::with_platform(platform)
    } else {
        Application::new_inaccessible(platform)
    }
}

static STARTUP_TIME: OnceLock<Instant> = OnceLock::new();

fn main() {
    STARTUP_TIME.get_or_init(|| Instant::now());

    // If this process was re-executed as a Linux sandbox helper, run that mode
    // without returning. Must run before argument parsing: the wrapped command's
    // args are appended verbatim and would otherwise be misinterpreted as Mav's
    // own arguments.
    sandbox::run_sandbox_launcher_if_invoked();

    #[cfg(unix)]
    util::prevent_root_execution();

    let args = Args::parse();

    // `mav --askpass` Makes mav operate in nc/netcat mode for use with askpass
    #[cfg(not(target_os = "windows"))]
    if let Some(socket) = &args.askpass {
        askpass::main(socket);
        return;
    }

    // `mav --crash-handler` Makes mav operate in minidump crash handler mode
    if let Some(socket) = &args.crash_handler {
        crashes::crash_server(socket.as_path(), paths::logs_dir().clone());
        return;
    }

    #[cfg(target_os = "windows")]
    if args.record_etw_trace {
        let mav_pid = args
            .etw_mav_pid
            .and_then(|pid| if pid >= 0 { Some(pid as u32) } else { None });
        let Some(output_path) = args.etw_output else {
            eprintln!("--etw-output is required for --record-etw-trace");
            process::exit(1);
        };

        let Some(etw_socket) = args.etw_socket else {
            eprintln!("--etw-socket is required for --record-etw-trace");
            process::exit(1);
        };

        if let Err(error) =
            etw_tracing::record_etw_trace(mav_pid, &output_path, etw_socket.as_str())
        {
            eprintln!("ETW trace recording failed: {error:#}");
            process::exit(1);
        }
        return;
    }

    #[cfg(all(not(debug_assertions), target_os = "windows"))]
    unsafe {
        use windows::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};

        if args.foreground {
            let _ = AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }

    // `mav --printenv` Outputs environment variables as JSON to stdout
    if args.printenv {
        util::shell_env::print_env();
        return;
    }

    if args.dump_all_actions {
        dump_all_gpui_actions();
        return;
    }

    // Set custom data directory.
    if let Some(dir) = &args.user_data_dir {
        paths::set_custom_data_dir(dir);
    }

    #[cfg(target_os = "windows")]
    match util::get_mav_cli_path() {
        Ok(path) => askpass::set_askpass_program(path),
        Err(err) => {
            eprintln!("Error: {}", err);
            if std::option_env!("MAV_BUNDLE").is_some() {
                process::exit(1);
            }
        }
    }

    let file_errors = init_paths();
    if !file_errors.is_empty() {
        files_not_created_on_launch(file_errors);
        return;
    }

    zlog::init();

    if stdout_is_a_pty() {
        zlog::init_output_stdout();
    } else {
        let result = zlog::init_output_file(paths::log_file(), Some(paths::old_log_file()));
        if let Err(err) = result {
            eprintln!("Could not open log file: {}... Defaulting to stdout", err);
            zlog::init_output_stdout();
        };
    }
    ztracing::init();

    let version = option_env!("MAV_BUILD_ID");
    let app_commit_sha =
        option_env!("MAV_COMMIT_SHA").map(|commit_sha| AppCommitSha::new(commit_sha.to_string()));
    let app_version = AppVersion::load(env!("CARGO_PKG_VERSION"), version, app_commit_sha.clone());

    if args.system_specs {
        let system_specs = system_specs::SystemSpecs::new_stateless(
            app_version,
            app_commit_sha,
            *release_channel::RELEASE_CHANNEL,
            client::telemetry::os_name(),
            client::telemetry::os_version(),
        );
        println!("Mav System Specs (from CLI):\n{}", system_specs);
        return;
    }

    rayon::ThreadPoolBuilder::new()
        .num_threads(std::thread::available_parallelism().map_or(1, |n| n.get().div_ceil(2)))
        .stack_size(10 * 1024 * 1024)
        .thread_name(|ix| format!("RayonWorker{}", ix))
        .build_global()
        .unwrap();

    log::info!(
        "========== starting mav version {}, sha {} ==========",
        app_version,
        app_commit_sha
            .as_ref()
            .map(|sha| sha.short())
            .as_deref()
            .unwrap_or("unknown"),
    );

    #[cfg(windows)]
    check_for_conpty_dll();

    let app = build_application().with_assets(Assets);

    let app_db = db::AppDatabase::new();
    let system_id = app.background_executor().spawn(system_id());
    let installation_id = app
        .background_executor()
        .spawn(installation_id(KeyValueStore::from_app_db(&app_db)));
    let session_id = Uuid::new_v4().to_string();
    let session = app.background_executor().spawn(Session::new(
        session_id.clone(),
        KeyValueStore::from_app_db(&app_db),
    ));
    let background_executor = app.background_executor();

    let (open_listener, mut open_rx) = OpenListener::new();

    let failed_single_instance_check = if *mav_env_vars::MAV_STATELESS
        || *release_channel::RELEASE_CHANNEL == ReleaseChannel::Dev
    {
        false
    } else {
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        {
            crate::mav::listen_for_cli_connections(open_listener.clone()).is_err()
        }

        #[cfg(target_os = "windows")]
        {
            !crate::mav::windows_only_instance::handle_single_instance(open_listener.clone(), &args)
        }

        #[cfg(target_os = "macos")]
        {
            use mav::mac_only_instance::*;
            ensure_only_instance() != IsOnlyInstance::Yes
        }
    };
    if failed_single_instance_check {
        println!("mav is already running");
        return;
    }

    let should_install_crash_handler = matches!(
        env::var("MAV_GENERATE_MINIDUMPS").as_deref(),
        Ok("true" | "1")
    ) || *release_channel::RELEASE_CHANNEL
        != ReleaseChannel::Dev;

    let crash_handler = if should_install_crash_handler {
        Some(
            app.background_executor().spawn(crashes::init(
                InitCrashHandler {
                    session_id,
                    // strip the build and channel information from the version string, we send them separately
                    mav_version: semver::Version::new(
                        app_version.major,
                        app_version.minor,
                        app_version.patch,
                    )
                    .to_string(),
                    binary: "mav".to_string(),
                    release_channel: release_channel::RELEASE_CHANNEL_NAME.clone(),
                    commit_sha: app_commit_sha
                        .as_ref()
                        .map(|sha| sha.full())
                        .unwrap_or_else(|| "no sha".to_owned()),
                },
                {
                    let background_executor1 = app.background_executor();
                    move |task| {
                        background_executor1.spawn(task).detach();
                    }
                },
                |pid| paths::temp_dir().join(format!("mav-crash-handler-{pid}")),
                move |duration| background_executor.timer(duration),
            )),
        )
    } else {
        crashes::force_backtrace();
        None
    };

    let git_hosting_provider_registry = Arc::new(GitHostingProviderRegistry::new());
    let git_binary_path =
        if cfg!(target_os = "macos") && option_env!("MAV_BUNDLE").as_deref() == Some("true") {
            app.path_for_auxiliary_executable("git")
                .context("could not find git binary path")
                .log_err()
        } else {
            None
        };
    if let Some(git_binary_path) = &git_binary_path {
        log::info!("Using git binary path: {:?}", git_binary_path);
    }

    let fs = Arc::new(RealFs::new(git_binary_path, app.background_executor()));
    let (user_keymap_file_rx, user_keymap_watcher) = watch_config_file(
        &app.background_executor(),
        fs.clone(),
        paths::keymap_file().clone(),
    );

    let (shell_env_loaded_tx, shell_env_loaded_rx) = oneshot::channel();
    if !stdout_is_a_pty() {
        app.background_executor()
            .spawn(async {
                #[cfg(unix)]
                util::load_login_shell_environment().await.log_err();
                shell_env_loaded_tx.send(()).ok();
            })
            .detach();
    } else {
        drop(shell_env_loaded_tx)
    }

    app.on_open_urls({
        let open_listener = open_listener.clone();
        move |urls| {
            open_listener.open(RawOpenRequest {
                urls,
                diff_paths: Vec::new(),
                ..Default::default()
            })
        }
    });
    app.on_reopen(move |cx| {
        if let Some(app_state) = AppState::try_global(cx) {
            cx.spawn({
                async move |cx| {
                    if let Err(e) = restore_or_create_workspace(app_state, cx).await {
                        fail_to_open_window_async(e, cx)
                    }
                }
            })
            .detach();
        }
    });

    app.run(move |cx| {
        app_runtime::run(
            cx,
            args,
            app_db,
            app_version,
            app_commit_sha,
            fs,
            git_hosting_provider_registry,
            open_listener,
            open_rx,
            crash_handler,
            system_id,
            installation_id,
            session,
            user_keymap_file_rx,
            user_keymap_watcher,
            shell_env_loaded_rx,
        )
    });
}
