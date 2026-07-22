use std::sync::Arc;

pub(super) fn filter_edit_history_by_path<'a>(
    edit_history: &'a [Arc<zeta_prompt::Event>],
    cursor_path: &std::path::Path,
) -> Vec<&'a zeta_prompt::Event> {
    edit_history
        .iter()
        .filter(|event| match event.as_ref() {
            zeta_prompt::Event::BufferChange { path, .. } => {
                let event_path = path.as_ref();
                if event_path == cursor_path {
                    return true;
                }
                let stripped = event_path
                    .components()
                    .skip(1)
                    .collect::<std::path::PathBuf>();
                stripped == cursor_path
            }
        })
        .map(|arc| arc.as_ref())
        .collect()
}

pub(super) fn extract_diff_from_event(event: &zeta_prompt::Event) -> &str {
    match event {
        zeta_prompt::Event::BufferChange { diff, .. } => diff.as_str(),
    }
}

pub(super) fn is_predicted_event(event: &zeta_prompt::Event) -> bool {
    match event {
        zeta_prompt::Event::BufferChange { predicted, .. } => *predicted,
    }
}
