use super::*;

fn icon_for_project_group(key: &ProjectGroupKey) -> IconName {
    let host = key.host();
    icon_for_remote_connection(host.as_ref())
}

pub(crate) fn icon_for_remote_connection(options: Option<&RemoteConnectionOptions>) -> IconName {
    match options {
        None => IconName::Screen,
        Some(options) => match options {
            RemoteConnectionOptions::Ssh(_) => IconName::Server,
            RemoteConnectionOptions::Wsl(_) => IconName::Linux,
            RemoteConnectionOptions::Docker(_) => IconName::Box,
            #[cfg(any(test, feature = "test-support"))]
            RemoteConnectionOptions::Mock(_) => IconName::Server,
        },
    }
}

// Compute the highlighted text for the name and path
pub(crate) fn highlights_for_path(
    path: &Path,
    match_positions: &Vec<usize>,
    path_start_offset: usize,
) -> (Option<HighlightedMatch>, HighlightedMatch) {
    let path_string = path.to_string_lossy();
    let path_text = path_string.to_string();
    let path_byte_len = path_text.len();
    // Get the subset of match highlight positions that line up with the given path.
    // Also adjusts them to start at the path start
    let path_positions = match_positions
        .iter()
        .copied()
        .skip_while(|position| *position < path_start_offset)
        .take_while(|position| *position < path_start_offset + path_byte_len)
        .map(|position| position - path_start_offset)
        .collect::<Vec<_>>();

    // Again subset the highlight positions to just those that line up with the file_name
    // again adjusted to the start of the file_name
    let file_name_text_and_positions = path.file_name().map(|file_name| {
        let file_name_text = file_name.to_string_lossy().into_owned();
        let file_name_start_byte = path_byte_len - file_name_text.len();
        let highlight_positions = path_positions
            .iter()
            .copied()
            .skip_while(|position| *position < file_name_start_byte)
            .take_while(|position| *position < file_name_start_byte + file_name_text.len())
            .map(|position| position - file_name_start_byte)
            .collect::<Vec<_>>();
        HighlightedMatch {
            text: file_name_text,
            highlight_positions,
            color: Color::Default,
        }
    });

    (
        file_name_text_and_positions,
        HighlightedMatch {
            text: path_text,
            highlight_positions: path_positions,
            color: Color::Default,
        },
    )
}

fn move_project_group_to_new_window(key: &ProjectGroupKey, window: &mut Window, cx: &mut App) {
    if let Some(handle) = window.window_handle().downcast::<MultiWorkspace>() {
        let key = key.clone();
        cx.defer(move |cx| {
            handle
                .update(cx, |multi_workspace, window, cx| {
                    multi_workspace
                        .open_project_group_in_new_window(&key, window, cx)
                        .detach_and_log_err(cx);
                })
                .log_err();
        });
    }
}

fn open_local_project(
    workspace: WeakEntity<Workspace>,
    create_new_window: bool,
    window: &mut Window,
    cx: &mut App,
) {
    use gpui::PathPromptOptions;
    use project::DirectoryLister;

    let Some(workspace) = workspace.upgrade() else {
        return;
    };

    let paths = workspace.update(cx, |workspace, cx| {
        workspace.prompt_for_open_path(
            PathPromptOptions {
                files: true,
                directories: true,
                multiple: true,
                prompt: None,
            },
            DirectoryLister::Local(
                workspace.project().clone(),
                workspace.app_state().fs.clone(),
            ),
            window,
            cx,
        )
    });

    let multi_workspace_handle = window.window_handle().downcast::<MultiWorkspace>();
    window
        .spawn(cx, async move |cx| {
            let Some(paths) = paths.await.log_err().flatten() else {
                return;
            };
            if !create_new_window {
                if let Some(handle) = multi_workspace_handle {
                    if let Some(task) = handle
                        .update(cx, |multi_workspace, window, cx| {
                            multi_workspace.open_project(paths, OpenMode::Activate, window, cx)
                        })
                        .log_err()
                    {
                        task.await.log_err();
                    }
                    return;
                }
            }
            if let Some(task) = workspace
                .update_in(cx, |workspace, window, cx| {
                    workspace.open_workspace_for_paths(OpenMode::NewWindow, paths, window, cx)
                })
                .log_err()
            {
                task.await.log_err();
            }
        })
        .detach();
}
