use gpui::{AppContext, AsyncApp, WindowHandle};
use mav_actions::feedback::FileBugReport;
use ui::prelude::*;
use util::ResultExt;

use crate::{
    MultiWorkspace,
    notifications::{NotificationId, simple_message_notification::MessageNotification},
};

pub(super) fn notify_if_database_failed(window: WindowHandle<MultiWorkspace>, cx: &mut AsyncApp) {
    window
        .update(cx, |multi_workspace, _, cx| {
            let workspace = multi_workspace.workspace().clone();
            workspace.update(cx, |workspace, cx| {
                if (*db::ALL_FILE_DB_FAILED).load(std::sync::atomic::Ordering::Acquire) {
                    struct DatabaseFailedNotification;

                    workspace.show_notification(
                        NotificationId::unique::<DatabaseFailedNotification>(),
                        cx,
                        |cx| {
                            cx.new(|cx| {
                                MessageNotification::new("Failed to load the database file.", cx)
                                    .primary_message("File an Issue")
                                    .primary_icon(IconName::Plus)
                                    .primary_on_click(|window, cx| {
                                        window.dispatch_action(Box::new(FileBugReport), cx)
                                    })
                            })
                        },
                    );
                }
            });
        })
        .log_err();
}
