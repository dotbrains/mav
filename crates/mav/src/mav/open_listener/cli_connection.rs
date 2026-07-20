use super::{RawOpenRequest, open_workspaces};
use crate::handle_open_request;
use cli::{CliRequest, CliResponse, CliResponseSink};
use futures::StreamExt;
use futures::channel::mpsc;
use gpui::AsyncApp;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use util::ResultExt;
use util::debug_panic;
use workspace::{AppState, MultiWorkspace};

use super::OpenRequest;

pub async fn handle_cli_connection(
    (mut requests, responses): (
        mpsc::UnboundedReceiver<CliRequest>,
        Box<dyn CliResponseSink>,
    ),
    app_state: Arc<AppState>,
    cx: &mut AsyncApp,
) {
    if let Some(request) = requests.next().await {
        match request {
            CliRequest::Open {
                urls,
                paths,
                diff_paths,
                diff_all,
                wait,
                wsl,
                mut open_behavior,
                env,
                user_data_dir: _,
                dev_container,
                cwd,
            } => {
                if !urls.is_empty() {
                    cx.update(|cx| {
                        match OpenRequest::parse(
                            RawOpenRequest {
                                urls,
                                diff_paths,
                                diff_all,
                                dev_container,
                                wsl,
                                open_behavior: Some(open_behavior),
                            },
                            cx,
                        ) {
                            Ok(open_request) => {
                                cx.activate(true);
                                handle_open_request(open_request, app_state.clone(), cx);
                                responses.send(CliResponse::Exit { status: 0 }).log_err();
                            }
                            Err(e) => {
                                responses
                                    .send(CliResponse::Stderr {
                                        message: format!("{e}"),
                                    })
                                    .log_err();
                                responses.send(CliResponse::Exit { status: 1 }).log_err();
                            }
                        };
                    });
                    return;
                }

                if open_behavior == cli::OpenBehavior::Default {
                    match resolve_open_behavior(
                        &paths,
                        &app_state,
                        responses.as_ref(),
                        &mut requests,
                        cx,
                    )
                    .await
                    {
                        Some(settings::CliDefaultOpenBehavior::ExistingWindow) => {
                            open_behavior = cli::OpenBehavior::ExistingWindow;
                        }
                        Some(settings::CliDefaultOpenBehavior::NewWindow) => {
                            open_behavior = cli::OpenBehavior::Classic;
                        }
                        None => {}
                    }
                }

                cx.update(|cx| cx.activate(true));

                let open_workspace_result = open_workspaces(
                    paths,
                    diff_paths,
                    diff_all,
                    open_behavior,
                    responses.as_ref(),
                    wait,
                    dev_container,
                    app_state.clone(),
                    env,
                    cwd,
                    cx,
                )
                .await;

                let status = if open_workspace_result.is_err() { 1 } else { 0 };
                responses.send(CliResponse::Exit { status }).log_err();
            }
            CliRequest::SetOpenBehavior { .. } => {
                // We handle this case in a situation-specific way in
                // resolve_open_behavior
                debug_panic!("unexpected SetOpenBehavior message");
            }
        }
    }
}

/// Resolves the CLI open behavior when no explicit flag (`-n`, `-e`, `--reuse`)
/// was given. May prompt the user interactively on first run.
///
/// Returns `Some(behavior)` to override the default, or `None` if no override
/// is needed (e.g. no existing windows, paths already in a workspace, or the
/// user has already configured `cli_default_open_behavior` in settings).
async fn resolve_open_behavior(
    paths: &[String],
    app_state: &Arc<AppState>,
    responses: &dyn CliResponseSink,
    requests: &mut mpsc::UnboundedReceiver<CliRequest>,
    cx: &mut AsyncApp,
) -> Option<settings::CliDefaultOpenBehavior> {
    let has_existing_windows = cx.update(|cx| {
        cx.windows()
            .iter()
            .any(|window| window.downcast::<MultiWorkspace>().is_some())
    });

    if !has_existing_windows {
        return None;
    }

    if !paths.is_empty() {
        let paths_as_pathbufs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
        let paths_in_existing_workspace = cx.update(|cx| {
            for window in cx.windows() {
                if let Some(multi_workspace) = window.downcast::<MultiWorkspace>() {
                    if let Ok(multi_workspace) = multi_workspace.read(cx) {
                        for workspace in multi_workspace.workspaces() {
                            let project = workspace.read(cx).project().read(cx);
                            if project
                                .visibility_for_paths(&paths_as_pathbufs, false, cx)
                                .is_some()
                            {
                                return true;
                            }
                        }
                    }
                }
            }
            false
        });

        if paths_in_existing_workspace {
            return None;
        }
    }

    if !paths.is_empty() {
        let has_directory =
            futures::future::join_all(paths.iter().map(|p| app_state.fs.is_dir(Path::new(p))))
                .await
                .into_iter()
                .any(|is_dir| is_dir);

        if !has_directory {
            return None;
        }
    }

    let settings_text = app_state
        .fs
        .load(paths::settings_file())
        .await
        .unwrap_or_default();

    if settings_text.contains("cli_default_open_behavior") {
        return None;
    }

    responses.send(CliResponse::PromptOpenBehavior).log_err()?;

    if let Some(CliRequest::SetOpenBehavior { behavior }) = requests.next().await {
        let behavior = match behavior {
            cli::CliBehaviorSetting::ExistingWindow => {
                settings::CliDefaultOpenBehavior::ExistingWindow
            }
            cli::CliBehaviorSetting::NewWindow => settings::CliDefaultOpenBehavior::NewWindow,
        };

        let fs = app_state.fs.clone();
        cx.update(|cx| {
            settings::update_settings_file(fs, cx, move |content, _cx| {
                content.workspace.cli_default_open_behavior = Some(behavior);
            });
        });

        return Some(behavior);
    }

    None
}
