use crate::restore_or_create_workspace;
use anyhow::{Context as _, Result, anyhow};
use cli::{CliResponse, CliResponseSink};
use db::kvp::KeyValueStore;
use editor::Editor;
use fs::Fs;
use futures::FutureExt;
use futures::channel::oneshot;
use futures::future;
use git_ui::{file_diff_view::FileDiffView, multi_diff_view::MultiDiffView};
use gpui::{AsyncApp, TaskExt, WindowHandle};
use onboarding::{FIRST_OPEN, show_onboarding_view};
use recent_projects::{RemoteSettings, navigate_to_positions, open_remote_project};
use remote::RemoteConnectionOptions;
use settings::Settings;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use util::ResultExt;
use util::paths::PathWithPosition;
use workspace::PathList;
use workspace::item::ItemHandle;
use workspace::{AppState, MultiWorkspace, OpenOptions, OpenResult, SerializedWorkspaceLocation};

pub async fn open_paths_with_positions(
    path_positions: &[PathWithPosition],
    diff_paths: &[[String; 2]],
    diff_all: bool,
    app_state: Arc<AppState>,
    open_options: workspace::OpenOptions,
    cx: &mut AsyncApp,
) -> Result<(
    WindowHandle<MultiWorkspace>,
    Vec<Option<Result<Box<dyn ItemHandle>>>>,
)> {
    let paths = path_positions
        .iter()
        .map(|path_with_position| path_with_position.path.clone())
        .collect::<Vec<_>>();

    let OpenResult {
        window: multi_workspace,
        opened_items: mut items,
        ..
    } = cx
        .update(|cx| workspace::open_paths(&paths, app_state.clone(), open_options, cx))
        .await?;

    if diff_all && !diff_paths.is_empty() {
        if let Ok(diff_view) = multi_workspace.update(cx, |multi_workspace, window, cx| {
            multi_workspace.workspace().update(cx, |workspace, cx| {
                MultiDiffView::open(diff_paths.to_vec(), workspace, window, cx)
            })
        }) && let Some(diff_view) = diff_view.await.log_err()
        {
            items.push(Some(Ok(Box::new(diff_view))));
        }
    } else {
        let workspace_weak = multi_workspace.read_with(cx, |multi_workspace, _cx| {
            multi_workspace.workspace().downgrade()
        })?;
        let canonicalize = async |raw: &str| {
            app_state
                .fs
                .canonicalize(Path::new(raw))
                .await
                .with_context(|| format!("opening --diff path {raw:?}"))
        };
        for diff_pair in diff_paths {
            let (old_path, new_path) =
                match futures::join!(canonicalize(&diff_pair[0]), canonicalize(&diff_pair[1])) {
                    (Ok(old), Ok(new)) => (old, new),
                    (old, new) => {
                        for result in [old, new] {
                            if let Err(err) = result {
                                items.push(Some(Err(err)));
                            }
                        }
                        continue;
                    }
                };
            if let Ok(diff_view) = multi_workspace.update(cx, |_multi_workspace, window, cx| {
                FileDiffView::open(old_path, new_path, workspace_weak.clone(), window, cx)
            }) && let Some(diff_view) = diff_view.await.log_err()
            {
                items.push(Some(Ok(Box::new(diff_view))))
            }
        }
    }

    for (item, path) in items.iter_mut().zip(&paths) {
        if let Some(Err(error)) = item {
            *error = anyhow!("error opening {path:?}: {error:#}");
        }
    }

    let items_for_navigation = items
        .iter()
        .map(|item| item.as_ref().and_then(|r| r.as_ref().ok()).cloned())
        .collect::<Vec<_>>();
    navigate_to_positions(&multi_workspace, items_for_navigation, path_positions, cx);

    Ok((multi_workspace, items))
}

pub fn open_options_for_request(
    open_behavior: Option<cli::OpenBehavior>,
    location: &SerializedWorkspaceLocation,
    cx: &gpui::App,
) -> workspace::OpenOptions {
    open_behavior.map_or_else(workspace::OpenOptions::default, |open_behavior| {
        open_options_for_behavior(open_behavior, location, cx)
    })
}

pub(crate) fn open_options_for_behavior(
    open_behavior: cli::OpenBehavior,
    location: &SerializedWorkspaceLocation,
    cx: &gpui::App,
) -> workspace::OpenOptions {
    let requesting_window = if open_behavior == cli::OpenBehavior::Reuse {
        workspace::workspace_windows_for_location(location, cx)
            .into_iter()
            .next()
    } else {
        None
    };
    workspace::OpenOptions {
        workspace_matching: match open_behavior {
            cli::OpenBehavior::AlwaysNew | cli::OpenBehavior::Reuse => {
                workspace::WorkspaceMatching::None
            }
            cli::OpenBehavior::Add => workspace::WorkspaceMatching::MatchSubdirectory,
            _ => workspace::WorkspaceMatching::MatchExact,
        },
        add_dirs_to_sidebar: match open_behavior {
            cli::OpenBehavior::ExistingWindow => true,
            cli::OpenBehavior::Default => {
                workspace::WorkspaceSettings::get_global(cx).cli_default_open_behavior
                    == settings::CliDefaultOpenBehavior::ExistingWindow
            }
            _ => false,
        },
        requesting_window,
        ..Default::default()
    }
}

pub(crate) async fn open_workspaces(
    paths: Vec<String>,
    diff_paths: Vec<[String; 2]>,
    diff_all: bool,
    open_behavior: cli::OpenBehavior,
    responses: &dyn CliResponseSink,
    wait: bool,
    dev_container: bool,
    app_state: Arc<AppState>,
    env: Option<collections::HashMap<String, String>>,
    cwd: Option<PathBuf>,
    cx: &mut AsyncApp,
) -> Result<()> {
    if paths.is_empty() && diff_paths.is_empty() && open_behavior != cli::OpenBehavior::AlwaysNew {
        return restore_or_create_workspace(app_state, cx).await;
    }

    let grouped_locations: Vec<(SerializedWorkspaceLocation, PathList)> =
        if paths.is_empty() && diff_paths.is_empty() {
            Vec::new()
        } else {
            vec![(
                SerializedWorkspaceLocation::Local,
                PathList::new(&paths.into_iter().map(PathBuf::from).collect::<Vec<_>>()),
            )]
        };

    if grouped_locations.is_empty() {
        let kvp = cx.update(|cx| KeyValueStore::global(cx));
        if matches!(kvp.read_kvp(FIRST_OPEN), Ok(None)) {
            cx.update(|cx| show_onboarding_view(app_state, cx).detach());
        } else {
            cx.update(|cx| {
                let open_options = OpenOptions {
                    env,
                    ..Default::default()
                };
                workspace::open_new(open_options, app_state, cx, |workspace, window, cx| {
                    Editor::new_file(workspace, &Default::default(), window, cx)
                })
                .detach_and_log_err(cx);
            });
        }
        return Ok(());
    }

    let mut errored = false;

    for (location, workspace_paths) in grouped_locations {
        let base_open_options =
            cx.update(|cx| open_options_for_behavior(open_behavior, &location, cx));
        let open_options = workspace::OpenOptions {
            wait,
            env: env.clone(),
            open_in_dev_container: dev_container,
            ..base_open_options
        };

        match location {
            SerializedWorkspaceLocation::Local => {
                let workspace_paths = workspace_paths
                    .paths()
                    .iter()
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect();

                let workspace_failed_to_open = open_local_workspace(
                    workspace_paths,
                    diff_paths.clone(),
                    diff_all,
                    open_options,
                    cwd.clone(),
                    responses,
                    &app_state,
                    cx,
                )
                .await;

                if workspace_failed_to_open {
                    errored = true
                }
            }
            SerializedWorkspaceLocation::Remote(mut connection) => {
                let app_state = app_state.clone();
                if let RemoteConnectionOptions::Ssh(options) = &mut connection {
                    cx.update(|cx| {
                        RemoteSettings::get_global(cx)
                            .fill_connection_options_from_settings(options)
                    });
                }
                cx.spawn(async move |cx| {
                    open_remote_project(
                        connection,
                        workspace_paths.paths().to_vec(),
                        app_state,
                        open_options,
                        cx,
                    )
                    .await
                    .log_err();
                })
                .detach();
            }
        }
    }

    anyhow::ensure!(!errored, "failed to open a workspace");

    Ok(())
}

pub(crate) async fn open_local_workspace(
    mut workspace_paths: Vec<String>,
    diff_paths: Vec<[String; 2]>,
    diff_all: bool,
    open_options: workspace::OpenOptions,
    cwd: Option<PathBuf>,
    responses: &dyn CliResponseSink,
    app_state: &Arc<AppState>,
    cx: &mut AsyncApp,
) -> bool {
    let user_provided_paths = !workspace_paths.is_empty();

    if !user_provided_paths
        && !diff_paths.is_empty()
        && let Some(cwd) = cwd
    {
        workspace_paths.push(cwd.to_string_lossy().to_string());
    }

    let paths_with_position =
        derive_paths_with_position(app_state.fs.as_ref(), workspace_paths).await;

    let (workspace, items) = match open_paths_with_positions(
        &paths_with_position,
        &diff_paths,
        diff_all,
        app_state.clone(),
        open_options.clone(),
        cx,
    )
    .await
    {
        Ok(result) => result,
        Err(error) => {
            let paths = paths_with_position
                .iter()
                .map(|p| p.path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            log::error!("failed to open workspace [{paths}]: {error:#}");
            responses
                .send(CliResponse::Stderr {
                    message: format!("error opening [{paths}]: {error:#}"),
                })
                .log_err();
            return true;
        }
    };

    let mut errored = false;
    let mut item_release_futures = Vec::new();
    let mut subscriptions = Vec::new();
    if open_options.wait {
        let mut wait_for_window_close = paths_with_position.is_empty() && diff_paths.is_empty();
        if user_provided_paths {
            for path_with_position in &paths_with_position {
                if app_state.fs.is_dir(&path_with_position.path).await {
                    wait_for_window_close = true;
                    break;
                }
            }
        }

        if wait_for_window_close {
            let (release_tx, release_rx) = oneshot::channel();
            item_release_futures.push(release_rx);
            subscriptions.push(workspace.update(cx, |_, _, cx| {
                cx.on_release(move |_, _| {
                    let _ = release_tx.send(());
                })
            }));
        }
    }

    for item in items {
        match item {
            Some(Ok(item)) => {
                if open_options.wait {
                    let (release_tx, release_rx) = oneshot::channel();
                    item_release_futures.push(release_rx);
                    subscriptions.push(Ok(cx.update(|cx| {
                        item.on_release(
                            cx,
                            Box::new(move |_| {
                                release_tx.send(()).ok();
                            }),
                        )
                    })));
                }
            }
            Some(Err(err)) => {
                log::error!("{err:#}");
                responses
                    .send(CliResponse::Stderr {
                        message: format!("{err:#}"),
                    })
                    .log_err();
                errored = true;
            }
            None => {}
        }
    }

    if open_options.wait {
        let wait = async move {
            let _subscriptions = subscriptions;
            let _ = future::try_join_all(item_release_futures).await;
        }
        .fuse();
        futures::pin_mut!(wait);

        let background = cx.background_executor().clone();
        loop {
            let mut timer = background.timer(Duration::from_secs(1)).fuse();
            futures::select_biased! {
                _ = wait => break,
                _ = timer => {
                    if responses.send(CliResponse::Ping).is_err() {
                        break;
                    }
                }
            }
        }
    }

    errored
}

pub async fn derive_paths_with_position(
    fs: &dyn Fs,
    path_strings: impl IntoIterator<Item = impl AsRef<str>>,
) -> Vec<PathWithPosition> {
    let path_strings: Vec<_> = path_strings.into_iter().collect();
    let mut result = Vec::with_capacity(path_strings.len());
    for path_str in path_strings {
        let original_path = Path::new(path_str.as_ref());
        let mut parsed = PathWithPosition::parse_str(path_str.as_ref());

        let has_colon = original_path
            .file_name()
            .and_then(|name| name.to_str())
            .is_none_or(|name| name.contains(':'));

        if (!has_colon || !cfg!(windows))
            && parsed.row.is_some()
            && parsed.path != original_path
            && (fs.is_file(original_path).await || fs.is_dir(original_path).await)
        {
            parsed = PathWithPosition::from_path(original_path.to_path_buf());
        }

        if let Ok(canonicalized) = fs.canonicalize(&parsed.path).await {
            parsed.path = canonicalized;
        }

        result.push(parsed);
    }
    result
}
