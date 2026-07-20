use super::*;

pub(super) fn full_path_for_empty_project_path(
    file: &dyn language::File,
    cx: &App,
) -> Option<String> {
    if file.path().file_name().is_some() {
        return None;
    }

    let full_path = file.full_path(cx).display().to_string();
    (!full_path.is_empty()).then_some(full_path)
}

pub(super) fn skill_issue_file_label(path: &std::path::Path) -> String {
    let file_name = path.file_name().and_then(|name| name.to_str());
    let parent_name = path
        .parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str());

    match (parent_name, file_name) {
        (Some(parent_name), Some(file_name)) => format!("{parent_name}/{file_name}"),
        (_, Some(file_name)) => file_name.to_string(),
        _ => path.display().to_string(),
    }
}

pub fn open_markdown_in_workspace(
    title: String,
    markdown: String,
    workspace: Entity<Workspace>,
    window: &mut Window,
    cx: &mut App,
) -> Task<Result<()>> {
    let markdown_language_task = workspace
        .read(cx)
        .app_state()
        .languages
        .language_for_name("Markdown");
    let project = workspace.read(cx).project().clone();

    window.spawn(cx, async move |cx| {
        let markdown_language = markdown_language_task.await?;

        let buffer = project
            .update(cx, |project, cx| {
                project.create_buffer(Some(markdown_language), false, cx)
            })
            .await?;

        buffer.update(cx, |buffer, cx| {
            buffer.set_text(markdown, cx);
            buffer.set_capability(language::Capability::ReadWrite, cx);
        });

        workspace.update_in(cx, |workspace, window, cx| {
            let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx).with_title(title.clone()));

            workspace.add_item_to_active_pane(
                Box::new(cx.new(|cx| {
                    let mut editor =
                        Editor::for_multibuffer(buffer, Some(project.clone()), window, cx);
                    editor.set_breadcrumb_header(title);
                    editor.disable_mouse_wheel_zoom();
                    editor
                })),
                None,
                true,
                window,
                cx,
            );
        })?;
        anyhow::Ok(())
    })
}
