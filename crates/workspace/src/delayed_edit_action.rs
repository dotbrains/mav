use std::time::Duration;

use anyhow::Result;
use futures::{FutureExt, channel::oneshot};
use gpui::{Context, Task};
use ui::Window;
use util::ResultExt as _;

use crate::Workspace;

pub(crate) struct DelayedDebouncedEditAction {
    task: Option<Task<()>>,
    cancel_channel: Option<oneshot::Sender<()>>,
}

impl DelayedDebouncedEditAction {
    pub(crate) fn new() -> DelayedDebouncedEditAction {
        DelayedDebouncedEditAction {
            task: None,
            cancel_channel: None,
        }
    }

    pub(crate) fn fire_new<F>(
        &mut self,
        delay: Duration,
        window: &mut Window,
        cx: &mut Context<Workspace>,
        func: F,
    ) where
        F: 'static
            + Send
            + FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>) -> Task<Result<()>>,
    {
        if let Some(channel) = self.cancel_channel.take() {
            _ = channel.send(());
        }

        let (sender, mut receiver) = oneshot::channel::<()>();
        self.cancel_channel = Some(sender);

        let previous_task = self.task.take();
        self.task = Some(cx.spawn_in(window, async move |workspace, cx| {
            let mut timer = cx.background_executor().timer(delay).fuse();
            if let Some(previous_task) = previous_task {
                previous_task.await;
            }

            futures::select_biased! {
                _ = receiver => return,
                    _ = timer => {}
            }

            if let Some(result) = workspace
                .update_in(cx, |workspace, window, cx| (func)(workspace, window, cx))
                .log_err()
            {
                result.await.log_err();
            }
        }));
    }
}
