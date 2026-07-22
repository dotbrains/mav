use super::*;

impl Editor {
    pub(super) fn copy_permalink_to_line(
        &mut self,
        _: &CopyPermalinkToLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let permalink_task = self.get_permalink_to_line(cx);
        let workspace = self.workspace();
        cx.spawn_in(window, async move |_, cx| match permalink_task.await {
            Ok(permalink) => {
                cx.update(|_, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(permalink.to_string()));
                })
                .ok();
            }
            Err(err) => {
                let message = format!("Failed to copy permalink: {err}");

                anyhow::Result::<()>::Err(err).log_err();

                if let Some(workspace) = workspace {
                    workspace
                        .update_in(cx, |workspace, _, cx| {
                            struct CopyPermalinkToLine;

                            workspace.show_toast(
                                Toast::new(
                                    NotificationId::unique::<CopyPermalinkToLine>(),
                                    message,
                                ),
                                cx,
                            )
                        })
                        .ok();
                }
            }
        })
        .detach();
    }

    pub(super) fn open_permalink_to_line(
        &mut self,
        _: &OpenPermalinkToLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let permalink_task = self.get_permalink_to_line(cx);
        let workspace = self.workspace();

        cx.spawn_in(window, async move |_, cx| match permalink_task.await {
            Ok(permalink) => {
                cx.update(|_, cx| {
                    cx.open_url(permalink.as_ref());
                })
                .ok();
            }
            Err(err) => {
                let message = format!("Failed to open permalink: {err}");

                anyhow::Result::<()>::Err(err).log_err();

                if let Some(workspace) = workspace {
                    workspace.update(cx, |workspace, cx| {
                        struct OpenPermalinkToLine;

                        workspace.show_toast(
                            Toast::new(NotificationId::unique::<OpenPermalinkToLine>(), message),
                            cx,
                        )
                    });
                }
            }
        })
        .detach();
    }
}
