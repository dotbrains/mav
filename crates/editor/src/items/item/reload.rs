use super::super::*;

pub(super) fn reload(
    editor: &mut Editor,
    project: Entity<Project>,
    window: &mut Window,
    cx: &mut Context<Editor>,
) -> Task<Result<()>> {
    let buffer = editor.buffer().clone();
    let buffers = editor.buffer.read(cx).all_buffers();
    let reload_buffers =
        project.update(cx, |project, cx| project.reload_buffers(buffers, true, cx));
    cx.spawn_in(window, async move |this, cx| {
        let transaction = reload_buffers.log_err().await;
        this.update(cx, |editor, cx| {
            editor.request_autoscroll(Autoscroll::fit(), cx)
        })?;
        buffer.update(cx, |buffer, cx| {
            if let Some(transaction) = transaction
                && !buffer.is_singleton()
            {
                buffer.push_transaction(&transaction.0, cx);
            }
        });
        Ok(())
    })
}
