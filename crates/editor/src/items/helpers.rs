use super::*;

pub fn entry_label_color(selected: bool) -> Color {
    tab_label_color(selected)
}

pub fn entry_diagnostic_aware_icon_name_and_color(
    diagnostic_severity: Option<DiagnosticSeverity>,
) -> Option<(IconName, Color)> {
    match diagnostic_severity {
        Some(DiagnosticSeverity::ERROR) => Some((IconName::Close, Color::Error)),
        Some(DiagnosticSeverity::WARNING) => Some((IconName::Triangle, Color::Warning)),
        _ => None,
    }
}

pub fn entry_diagnostic_aware_icon_decoration_and_color(
    diagnostic_severity: Option<DiagnosticSeverity>,
) -> Option<(IconDecorationKind, Color)> {
    match diagnostic_severity {
        Some(DiagnosticSeverity::ERROR) => Some((IconDecorationKind::X, Color::Error)),
        Some(DiagnosticSeverity::WARNING) => Some((IconDecorationKind::Triangle, Color::Warning)),
        _ => None,
    }
}

pub fn entry_git_aware_label_color(git_status: GitSummary, ignored: bool, selected: bool) -> Color {
    let tracked = git_status.index + git_status.worktree;
    if git_status.conflict > 0 {
        Color::Conflict
    } else if tracked.deleted > 0 {
        Color::Deleted
    } else if tracked.modified > 0 {
        Color::Modified
    } else if tracked.added > 0 || git_status.untracked > 0 {
        Color::Created
    } else if ignored {
        Color::Ignored
    } else {
        entry_label_color(selected)
    }
}

pub(crate) fn path_for_buffer<'a>(
    buffer: &Entity<MultiBuffer>,
    height: usize,
    include_filename: bool,
    cx: &'a App,
) -> Option<Cow<'a, str>> {
    let file = buffer.read(cx).as_singleton()?.read(cx).file()?;
    path_for_file(file, height, include_filename, cx)
}

fn path_for_file<'a>(
    file: &'a Arc<dyn language::File>,
    mut height: usize,
    include_filename: bool,
    cx: &'a App,
) -> Option<Cow<'a, str>> {
    if project::File::from_dyn(Some(file)).is_none() {
        return None;
    }

    let file = file.as_ref();
    // Ensure we always render at least the filename.
    height += 1;

    let mut prefix = file.path().as_ref();
    while height > 0 {
        if let Some(parent) = prefix.parent() {
            prefix = parent;
            height -= 1;
        } else {
            break;
        }
    }

    // The full_path method allocates, so avoid calling it if height is zero.
    if height > 0 {
        let mut full_path = file.full_path(cx);
        if !include_filename {
            if !full_path.pop() {
                return None;
            }
        }
        Some(full_path.to_string_lossy().into_owned().into())
    } else {
        let mut path = file.path().strip_prefix(prefix).ok()?;
        if !include_filename {
            path = path.parent()?;
        }
        Some(path.display(file.path_style(cx)))
    }
}

/// Restores serialized buffer contents by overwriting the buffer with saved text.
/// This is somewhat wasteful since we load the whole buffer from disk then overwrite it,
/// but keeps implementation simple as we don't need to persist all metadata from loading
/// (git diff base, etc.).
pub(crate) fn restore_serialized_buffer_contents(
    buffer: &mut Buffer,
    contents: String,
    mtime: Option<MTime>,
    cx: &mut Context<Buffer>,
) {
    // If we did restore an mtime, store it on the buffer so that
    // the next edit will mark the buffer as dirty/conflicted.
    if mtime.is_some() {
        buffer.did_reload(buffer.version(), buffer.line_ending(), mtime, cx);
    }
    buffer.set_text(contents, cx);
    if let Some(entry) = buffer.peek_undo_stack() {
        buffer.forget_transaction(entry.transaction_id());
    }
}

pub(crate) fn serialize_path_key(path_key: &PathKey) -> proto::PathKey {
    proto::PathKey {
        sort_prefix: path_key.sort_prefix,
        path: path_key.path.to_proto(),
    }
}

pub(crate) fn deserialize_path_key(path_key: proto::PathKey) -> Option<PathKey> {
    Some(PathKey {
        sort_prefix: path_key.sort_prefix,
        path: RelPath::from_proto(&path_key.path).ok()?,
    })
}

pub(crate) fn chunk_search_range(
    buffer: BufferSnapshot,
    query: &SearchQuery,
    num_cpus: u32,
    initial_range: Range<BufferOffset>,
) -> Box<dyn Iterator<Item = Range<usize>> + 'static> {
    let range = initial_range.to_offset(&buffer);
    if range.is_empty() {
        return Box::new(std::iter::empty());
    }

    let summary: TextSummary = buffer.text_summary_for_range(initial_range);
    let num_chunks = if !query.is_regex() && !query.as_str().contains('\n') {
        NonZeroU32::new(summary.lines.row.saturating_add(1).min(num_cpus.max(1)))
    } else {
        NonZeroU32::new(1)
    };

    let Some(num_chunks) = num_chunks else {
        return Box::new(std::iter::empty());
    };

    let mut chunk_start = range.start;
    let rope = buffer.as_rope().clone();
    let range_end = range.end;
    let average_chunk_length = summary.len.div_ceil(num_chunks.get() as usize);
    Box::new(std::iter::from_fn(move || {
        if chunk_start >= range_end {
            return None;
        }
        let candidate_position = chunk_start + average_chunk_length;
        let adjusted = rope.ceil_char_boundary(candidate_position);
        let mut as_point = rope.offset_to_point(adjusted);
        as_point.row += 1;
        as_point.column = 0;
        let end_offset = buffer.point_to_offset(as_point).min(range_end);
        let ret = chunk_start..end_offset;
        chunk_start = end_offset;
        Some(ret)
    }))
}
