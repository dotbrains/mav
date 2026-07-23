use super::*;

pub(crate) fn update_uncommitted_diff_for_buffer(
    editor: Entity<Editor>,
    project: &Entity<Project>,
    buffers: impl IntoIterator<Item = Entity<Buffer>>,
    buffer: Entity<MultiBuffer>,
    cx: &mut App,
) -> Task<()> {
    let mut tasks = Vec::new();
    project.update(cx, |project, cx| {
        for buffer in buffers {
            if project::File::from_dyn(buffer.read(cx).file()).is_some() {
                tasks.push(project.open_uncommitted_diff(buffer.clone(), cx))
            }
        }
    });
    cx.spawn(async move |cx| {
        let diffs = future::join_all(tasks).await;
        if editor.read_with(cx, |editor, _cx| editor.temporary_diff_override) {
            return;
        }
        buffer.update(cx, |buffer, cx| {
            for diff in diffs.into_iter().flatten() {
                buffer.add_diff(diff, cx);
            }
        });
    })
}
