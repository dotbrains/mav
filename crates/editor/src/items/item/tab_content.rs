use super::super::*;

pub(super) fn tab_content(editor: &Editor, params: TabContentParams, cx: &App) -> AnyElement {
    let label_color = if ItemSettings::get_global(cx).git_status {
        editor
            .buffer()
            .read(cx)
            .as_singleton()
            .and_then(|buffer| {
                let buffer = buffer.read(cx);
                let path = buffer.project_path(cx)?;
                let buffer_id = buffer.remote_id();
                let project = editor.project()?.read(cx);
                let entry = project.entry_for_path(&path, cx)?;
                let (repo, repo_path) = project
                    .git_store()
                    .read(cx)
                    .repository_and_path_for_buffer_id(buffer_id, cx)?;
                let status = repo.read(cx).status_for_path(&repo_path)?.status;

                Some(entry_git_aware_label_color(
                    status.summary(),
                    entry.is_ignored,
                    params.selected,
                ))
            })
            .unwrap_or_else(|| entry_label_color(params.selected))
    } else {
        entry_label_color(params.selected)
    };

    let description = params.detail.and_then(|detail| {
        let path = path_for_buffer(&editor.buffer, detail, false, cx)?;
        let description = path.trim();

        if description.is_empty() {
            return None;
        }

        Some(util::truncate_and_trailoff(
            description,
            params.max_title_len.unwrap_or(MAX_TAB_TITLE_LEN),
        ))
    });

    // Whether the file was saved in the past but is now deleted.
    let was_deleted: bool = editor
        .buffer()
        .read(cx)
        .as_singleton()
        .and_then(|buffer| buffer.read(cx).file())
        .is_some_and(|file| file.disk_state().is_deleted());

    h_flex()
        .gap_1()
        .when(params.truncate_title_middle, |this| {
            this.w_full().min_w_0().overflow_hidden()
        })
        .child(
            Label::new(if params.truncate_title_middle {
                editor.title(cx).to_string()
            } else {
                util::truncate_and_trailoff(
                    &editor.title(cx),
                    params.max_title_len.unwrap_or(MAX_TAB_TITLE_LEN),
                )
            })
            .color(label_color)
            .when(params.truncate_title_middle, |this| {
                this.truncate_middle().flex_1()
            })
            .when(params.preview, |this| this.italic())
            .when(was_deleted, |this| this.strikethrough()),
        )
        .when_some(description, |this, description| {
            this.child(
                Label::new(description)
                    .size(LabelSize::XSmall)
                    .when(params.truncate_title_middle, |this| {
                        this.truncate_start().flex_shrink()
                    })
                    .color(Color::Muted),
            )
        })
        .into_any_element()
}
