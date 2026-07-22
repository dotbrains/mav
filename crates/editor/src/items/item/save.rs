use super::super::*;

pub(super) fn save(
    editor: &mut Editor,
    options: SaveOptions,
    project: Entity<Project>,
    window: &mut Window,
    cx: &mut Context<Editor>,
) -> Task<Result<()>> {
    // Add meta data tracking # of auto saves
    if options.autosave {
        editor.report_editor_event(ReportEditorEvent::Saved { auto_saved: true }, None, cx);
    } else {
        editor.report_editor_event(ReportEditorEvent::Saved { auto_saved: false }, None, cx);
    }

    let buffers = editor.buffer().clone().read(cx).all_buffers();
    let buffers = buffers
        .into_iter()
        .map(|handle| handle.read(cx).base_buffer().unwrap_or(handle.clone()))
        .collect::<HashSet<_>>();

    let buffers_to_save = if editor.buffer.read(cx).is_singleton() && !options.autosave {
        buffers
    } else {
        buffers
            .into_iter()
            .filter(|buffer| buffer.read(cx).is_dirty())
            .collect()
    };

    let format_trigger = if options.force_format {
        FormatTrigger::Manual
    } else {
        FormatTrigger::Save
    };

    cx.spawn_in(window, async move |this, cx| {
        if options.format {
            this.update_in(cx, |editor, window, cx| {
                editor.perform_format(
                    project.clone(),
                    format_trigger,
                    FormatTarget::Buffers(buffers_to_save.clone()),
                    window,
                    cx,
                )
            })?
            .await?;
        }

        if !buffers_to_save.is_empty() {
            project
                .update(cx, |project, cx| {
                    project.save_buffers(buffers_to_save.clone(), cx)
                })
                .await?;
        }

        Ok(())
    })
}
