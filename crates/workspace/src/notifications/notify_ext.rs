use crate::Workspace;
use gpui::{App, AppContext as _, AsyncApp, Context, PromptLevel, Task, WeakEntity, Window};

use super::{show_app_notification, simple_message_notification, workspace_error_notification_id};

pub trait NotifyResultExt {
    type Ok;

    fn notify_err(self, workspace: &mut Workspace, cx: &mut Context<Workspace>)
    -> Option<Self::Ok>;

    fn notify_workspace_async_err(
        self,
        workspace: WeakEntity<Workspace>,
        cx: &mut AsyncApp,
    ) -> Option<Self::Ok>;

    /// Notifies the active workspace if there is one, otherwise notifies all workspaces.
    fn notify_app_err(self, cx: &mut App) -> Option<Self::Ok>;
}

impl<T, E> NotifyResultExt for std::result::Result<T, E>
where
    E: std::fmt::Debug + std::fmt::Display,
{
    type Ok = T;

    fn notify_err(self, workspace: &mut Workspace, cx: &mut Context<Workspace>) -> Option<T> {
        match self {
            Ok(value) => Some(value),
            Err(err) => {
                log::error!("Showing error notification in workspace: {err:?}");
                workspace.show_error(format!("Error: {err}"), cx);
                None
            }
        }
    }

    fn notify_workspace_async_err(
        self,
        workspace: WeakEntity<Workspace>,
        cx: &mut AsyncApp,
    ) -> Option<T> {
        match self {
            Ok(value) => Some(value),
            Err(err) => {
                log::error!("{err:?}");
                let message = format!("Error: {err}");
                workspace
                    .update(cx, |workspace, cx| workspace.show_error(message, cx))
                    .ok();
                None
            }
        }
    }

    fn notify_app_err(self, cx: &mut App) -> Option<T> {
        match self {
            Ok(value) => Some(value),
            Err(err) => {
                let message = format!("Error: {err}");
                log::error!("Showing error notification in app: {message}");
                show_app_notification(workspace_error_notification_id(), cx, {
                    move |cx| {
                        cx.new({
                        let message = message.clone();
                        move |cx| {
                            simple_message_notification::MessageNotification::from_workspace_error(message, cx)
                        }
                    })
                    }
                });

                None
            }
        }
    }
}

pub trait NotifyTaskExt {
    fn detach_and_notify_err(
        self,
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut App,
    );
}

impl<R, E> NotifyTaskExt for Task<std::result::Result<R, E>>
where
    E: std::fmt::Debug + std::fmt::Display + Sized + 'static,
    R: 'static,
{
    fn detach_and_notify_err(
        self,
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut App,
    ) {
        window
            .spawn(cx, async move |mut cx| {
                self.await.notify_workspace_async_err(workspace, &mut cx)
            })
            .detach();
    }
}

pub trait DetachAndPromptErr<R> {
    fn prompt_err(
        self,
        msg: &str,
        window: &Window,
        cx: &App,
        f: impl FnOnce(&anyhow::Error, &mut Window, &mut App) -> Option<String> + 'static,
    ) -> Task<Option<R>>;

    fn detach_and_prompt_err(
        self,
        msg: &str,
        window: &Window,
        cx: &App,
        f: impl FnOnce(&anyhow::Error, &mut Window, &mut App) -> Option<String> + 'static,
    );
}

impl<R> DetachAndPromptErr<R> for Task<anyhow::Result<R>>
where
    R: 'static,
{
    fn prompt_err(
        self,
        msg: &str,
        window: &Window,
        cx: &App,
        f: impl FnOnce(&anyhow::Error, &mut Window, &mut App) -> Option<String> + 'static,
    ) -> Task<Option<R>> {
        let msg = msg.to_owned();
        window.spawn(cx, async move |cx| {
            let result = self.await;
            if let Err(err) = result.as_ref() {
                log::error!("{err:#}");
                if let Ok(prompt) = cx.update(|window, cx| {
                    let mut display = format!("{err:#}");
                    if !display.ends_with('\n') {
                        display.push('.');
                    }
                    let detail = f(err, window, cx).unwrap_or(display);
                    window.prompt(PromptLevel::Critical, &msg, Some(&detail), &["OK"], cx)
                }) {
                    prompt.await.ok();
                }
                return None;
            }
            Some(result.unwrap())
        })
    }

    fn detach_and_prompt_err(
        self,
        msg: &str,
        window: &Window,
        cx: &App,
        f: impl FnOnce(&anyhow::Error, &mut Window, &mut App) -> Option<String> + 'static,
    ) {
        self.prompt_err(msg, window, cx, f).detach();
    }
}
