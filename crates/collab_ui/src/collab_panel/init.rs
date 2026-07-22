use super::*;
use anyhow::Context as _;

pub(super) fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _, _| {
        workspace.register_action(|workspace, _: &ToggleFocus, window, cx| {
            workspace.toggle_panel_focus::<CollabPanel>(window, cx);
            if let Some(collab_panel) = workspace.panel::<CollabPanel>(cx) {
                collab_panel.update(cx, |panel, cx| {
                    panel.filter_editor.update(cx, |editor, cx| {
                        if editor.snapshot(window, cx).is_focused() {
                            editor.select_all(&Default::default(), window, cx);
                        }
                    });
                })
            }
        });
        workspace.register_action(|workspace, _: &Toggle, window, cx| {
            if !workspace.toggle_panel_focus::<CollabPanel>(window, cx) {
                workspace.close_panel::<CollabPanel>(window, cx);
            }
        });
        workspace.register_action(|_, _: &OpenChannelNotes, window, cx| {
            let channel_id = ActiveCall::global(cx)
                .read(cx)
                .room()
                .and_then(|room| room.read(cx).channel_id());

            if let Some(channel_id) = channel_id {
                let workspace = cx.entity();
                window.defer(cx, move |window, cx| {
                    ChannelView::open(channel_id, None, workspace, window, cx)
                        .detach_and_log_err(cx)
                });
            }
        });
        workspace.register_action(|_, action: &OpenChannelNotesById, window, cx| {
            let channel_id = client::ChannelId(action.channel_id);
            let workspace = cx.entity();
            window.defer(cx, move |window, cx| {
                ChannelView::open(channel_id, None, workspace, window, cx).detach_and_log_err(cx)
            });
        });
        // TODO: make it possible to bind this one to a held key for push to talk?
        // how to make "toggle_on_modifiers_press" contextual?
        workspace.register_action(|_, _: &Mute, _, cx| title_bar::collab::toggle_mute(cx));
        workspace.register_action(|_, _: &Deafen, _, cx| title_bar::collab::toggle_deafen(cx));
        workspace.register_action(|_, _: &LeaveCall, window, cx| {
            CollabPanel::leave_call(window, cx);
        });
        workspace.register_action(|workspace, _: &CopyRoomId, window, cx| {
            use workspace::notifications::{NotificationId, NotifyTaskExt as _};

            struct RoomIdCopiedToast;

            if let Some(room) = ActiveCall::global(cx).read(cx).room() {
                let romo_id_fut = room.read(cx).room_id();
                let workspace_handle = cx.weak_entity();
                cx.spawn(async move |workspace, cx| {
                    let room_id = romo_id_fut.await.context("Failed to get livekit room")?;
                    workspace.update(cx, |workspace, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(room_id));
                        workspace.show_toast(
                            workspace::Toast::new(
                                NotificationId::unique::<RoomIdCopiedToast>(),
                                "Room ID copied to clipboard",
                            )
                            .autohide(),
                            cx,
                        );
                    })
                })
                .detach_and_notify_err(workspace_handle, window, cx);
            } else {
                workspace.show_error("There’s no active call; join one first.", cx);
            }
        });
        workspace.register_action(|workspace, _: &ShareProject, window, cx| {
            let project = workspace.project().clone();
            println!("{project:?}");
            window.defer(cx, move |_window, cx| {
                ActiveCall::global(cx).update(cx, move |call, cx| {
                    if let Some(room) = call.room() {
                        println!("{room:?}");
                        if room.read(cx).is_sharing_project() {
                            call.unshare_project(project, cx).ok();
                        } else {
                            call.share_project(project, cx).detach_and_log_err(cx);
                        }
                    }
                });
            });
        });
        // TODO(jk): Is this action ever triggered?
        workspace.register_action(|_, _: &ScreenShare, window, cx| {
            let room = ActiveCall::global(cx).read(cx).room().cloned();
            if let Some(room) = room {
                window.defer(cx, move |_window, cx| {
                    room.update(cx, |room, cx| {
                        if room.is_sharing_screen() {
                            room.unshare_screen(true, cx).ok();
                        } else {
                            #[cfg(target_os = "linux")]
                            let is_wayland = gpui::guess_compositor() == "Wayland";
                            #[cfg(not(target_os = "linux"))]
                            let is_wayland = false;

                            #[cfg(target_os = "linux")]
                            {
                                if is_wayland {
                                    room.share_screen_wayland(cx).detach_and_log_err(cx);
                                }
                            }
                            if !is_wayland {
                                let sources = cx.screen_capture_sources();

                                cx.spawn(async move |room, cx| {
                                    let sources = sources.await??;
                                    let first = sources.into_iter().next();
                                    if let Some(first) = first {
                                        room.update(cx, |room, cx| room.share_screen(first, cx))?
                                            .await
                                    } else {
                                        Ok(())
                                    }
                                })
                                .detach_and_log_err(cx);
                            }
                        };
                    });
                });
            }
        });
    })
    .detach();
}
