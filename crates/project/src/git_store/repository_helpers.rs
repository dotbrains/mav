use super::*;

pub(super) fn format_job_key(key: &GitJobKey) -> SharedString {
    match key {
        GitJobKey::WriteIndex(paths) => {
            let paths_str: Vec<_> = paths
                .iter()
                .map(|p| {
                    let rel: &RelPath = p;
                    format!("{}", AsRef::<Path>::as_ref(rel).display())
                })
                .collect();
            format!("WriteIndex({})", paths_str.join(", ")).into()
        }
        GitJobKey::ReloadBufferDiffBases => "ReloadBufferDiffBases".into(),
        GitJobKey::RefreshStatuses => "RefreshStatuses".into(),
        GitJobKey::ReloadGitState => "ReloadGitState".into(),
    }
}

pub(super) async fn append_pattern_to_ignore_file(
    fs: Arc<dyn Fs>,
    file_path: PathBuf,
    pattern: String,
) -> Result<()> {
    let existing_content = fs.load(&file_path).await.unwrap_or_default();

    if existing_content.lines().any(|line| line.trim() == pattern) {
        return Ok(());
    }

    let new_content = if existing_content.is_empty() {
        format!("{}\n", pattern)
    } else if existing_content.ends_with('\n') {
        format!("{}{}\n", existing_content, pattern)
    } else {
        format!("{}\n{}\n", existing_content, pattern)
    };

    fs.save(
        &file_path,
        &text::Rope::from(new_content.as_str()),
        text::LineEnding::Unix,
    )
    .await
}
