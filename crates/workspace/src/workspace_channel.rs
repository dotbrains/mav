use schemars::JsonSchema;
use serde::Deserialize;

use super::*;

actions!(
    collab,
    [
        /// Opens the channel notes for the current call.
        ///
        /// Use `collab_panel::OpenSelectedChannelNotes` to open the channel notes for the selected
        /// channel in the collab panel.
        ///
        /// If you want to open a specific channel, use `mav::OpenMavUrl` with a channel notes URL -
        /// can be copied via "Copy link to section" in the context menu of the channel notes
        /// buffer. These URLs look like `https://mav.dev/channel/channel-name-CHANNEL_ID/notes`.
        OpenChannelNotes,
        /// Mutes your microphone.
        Mute,
        /// Deafens yourself (mute both microphone and speakers).
        Deafen,
        /// Leaves the current call.
        LeaveCall,
        /// Shares the current project with collaborators.
        ShareProject,
        /// Shares your screen with collaborators.
        ScreenShare,
        /// Copies the current room name and session id for debugging purposes.
        CopyRoomId,
    ]
);

/// Opens the channel notes for a specific channel by its ID.
#[derive(Clone, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = collab)]
#[serde(deny_unknown_fields)]
pub struct OpenChannelNotesById {
    pub channel_id: u64,
}

actions!(
    mav,
    [
        /// Opens the Mav log file.
        OpenLog,
        /// Reveals the Mav log file in the system file manager.
        RevealLogInFileManager
    ]
);

async fn join_channel_internal(
    channel_id: ChannelId,
    app_state: &Arc<AppState>,
    requesting_window: Option<WindowHandle<MultiWorkspace>>,
    requesting_workspace: Option<WeakEntity<Workspace>>,
    active_call: &dyn AnyActiveCall,
    cx: &mut AsyncApp,
) -> Result<bool> {
    let (should_prompt, already_in_channel) = cx.update(|cx| {
        if !active_call.is_in_room(cx) {
            return (false, false);
        }

        let already_in_channel = active_call.channel_id(cx) == Some(channel_id);
        let should_prompt = active_call.is_sharing_project(cx)
            && active_call.has_remote_participants(cx)
            && !already_in_channel;
        (should_prompt, already_in_channel)
    });

    if already_in_channel {
        let task = cx.update(|cx| {
            if let Some((project, host)) = active_call.most_active_project(cx) {
                Some(join_in_room_project(project, host, app_state.clone(), cx))
            } else {
                None
            }
        });
        if let Some(task) = task {
            task.await?;
        }
        return anyhow::Ok(true);
    }

    if should_prompt {
        if let Some(multi_workspace) = requesting_window {
            let answer = multi_workspace
                .update(cx, |_, window, cx| {
                    window.prompt(
                        PromptLevel::Warning,
                        "Do you want to switch channels?",
                        Some("Leaving this call will unshare your current project."),
                        &["Yes, Join Channel", "Cancel"],
                        cx,
                    )
                })?
                .await;

            if answer == Ok(1) {
                return Ok(false);
            }
        } else {
            return Ok(false);
        }
    }

    let client = cx.update(|cx| active_call.client(cx));

    let mut client_status = client.status();

    // this loop will terminate within client::CONNECTION_TIMEOUT seconds.
    'outer: loop {
        let Some(status) = client_status.recv().await else {
            anyhow::bail!("error connecting");
        };

        match status {
            Status::Connecting
            | Status::Authenticating
            | Status::Authenticated
            | Status::Reconnecting
            | Status::Reauthenticating
            | Status::Reauthenticated => continue,
            Status::Connected { .. } => break 'outer,
            Status::SignedOut | Status::AuthenticationError => {
                return Err(ErrorCode::SignedOut.into());
            }
            Status::UpgradeRequired => return Err(ErrorCode::UpgradeRequired.into()),
            Status::ConnectionError | Status::ConnectionLost | Status::ReconnectionError { .. } => {
                return Err(ErrorCode::Disconnected.into());
            }
        }
    }

    let joined = cx
        .update(|cx| active_call.join_channel(channel_id, cx))
        .await?;

    if !joined {
        return anyhow::Ok(true);
    }

    cx.update(|cx| active_call.room_update_completed(cx)).await;

    let task = cx.update(|cx| {
        if let Some((project, host)) = active_call.most_active_project(cx) {
            return Some(join_in_room_project(project, host, app_state.clone(), cx));
        }

        // If you are the first to join a channel, see if you should share your project.
        if !active_call.has_remote_participants(cx)
            && !active_call.local_participant_is_guest(cx)
            && let Some(workspace) = requesting_workspace.as_ref().and_then(|w| w.upgrade())
        {
            let project = workspace.update(cx, |workspace, cx| {
                let project = workspace.project.read(cx);

                if !active_call.share_on_join(cx) {
                    return None;
                }

                if (project.is_local() || project.is_via_remote_server())
                    && project.visible_worktrees(cx).any(|tree| {
                        tree.read(cx)
                            .root_entry()
                            .is_some_and(|entry| entry.is_dir())
                    })
                {
                    Some(workspace.project.clone())
                } else {
                    None
                }
            });
            if let Some(project) = project {
                let share_task = active_call.share_project(project, cx);
                return Some(cx.spawn(async move |_cx| -> Result<()> {
                    share_task.await?;
                    Ok(())
                }));
            }
        }

        None
    });
    if let Some(task) = task {
        task.await?;
        return anyhow::Ok(true);
    }
    anyhow::Ok(false)
}

pub fn join_channel(
    channel_id: ChannelId,
    app_state: Arc<AppState>,
    requesting_window: Option<WindowHandle<MultiWorkspace>>,
    requesting_workspace: Option<WeakEntity<Workspace>>,
    cx: &mut App,
) -> Task<Result<()>> {
    let active_call = GlobalAnyActiveCall::global(cx).clone();
    cx.spawn(async move |cx| {
        let result = join_channel_internal(
            channel_id,
            &app_state,
            requesting_window,
            requesting_workspace,
            &*active_call.0,
            cx,
        )
        .await;

        // join channel succeeded, and opened a window
        if matches!(result, Ok(true)) {
            return anyhow::Ok(());
        }

        // find an existing workspace to focus and show call controls
        let mut active_window = requesting_window.or_else(|| activate_any_workspace_window(cx));
        if active_window.is_none() {
            // no open workspaces, make one to show the error in (blergh)
            let OpenResult {
                window: window_handle,
                ..
            } = cx
                .update(|cx| {
                    Workspace::new_local(
                        vec![],
                        app_state.clone(),
                        requesting_window,
                        None,
                        None,
                        OpenMode::Activate,
                        cx,
                    )
                })
                .await?;

            window_handle
                .update(cx, |_, window, _cx| {
                    window.activate_window();
                })
                .ok();

            if result.is_ok() {
                cx.update(|cx| {
                    cx.dispatch_action(&OpenChannelNotes);
                });
            }

            active_window = Some(window_handle);
        }

        if let Err(err) = result {
            log::error!("failed to join channel: {}", err);
            if let Some(active_window) = active_window {
                active_window
                    .update(cx, |_, window, cx| {
                        let detail: SharedString = match err.error_code() {
                            ErrorCode::SignedOut => "Please sign in to continue.".into(),
                            ErrorCode::UpgradeRequired => concat!(
                                "Your are running an unsupported version of Mav. ",
                                "Please update to continue."
                            )
                            .into(),
                            ErrorCode::NoSuchChannel => concat!(
                                "No matching channel was found. ",
                                "Please check the link and try again."
                            )
                            .into(),
                            ErrorCode::Forbidden => concat!(
                                "This channel is private, and you do not have access. ",
                                "Please ask someone to add you and try again."
                            )
                            .into(),
                            ErrorCode::Disconnected => {
                                "Please check your internet connection and try again.".into()
                            }
                            _ => format!("{}\n\nPlease try again.", err).into(),
                        };
                        window.prompt(
                            PromptLevel::Critical,
                            "Failed to join channel",
                            Some(&detail),
                            &["OK"],
                            cx,
                        )
                    })?
                    .await
                    .ok();
            }
        }

        // return ok, we showed the error to the user.
        anyhow::Ok(())
    })
}
