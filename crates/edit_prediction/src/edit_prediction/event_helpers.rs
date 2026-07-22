use super::*;

fn lines_between_ranges(left: &Range<Point>, right: &Range<Point>) -> u32 {
    if left.start > right.end {
        return left.start.row - right.end.row;
    }
    if right.start > left.end {
        return right.start.row - left.end.row;
    }
    0
}

fn push_recent_file(files: &mut VecDeque<RecentFile>, mut file: RecentFile) {
    if let Some(ix) = files.iter().position(|probe| probe.path == file.path)
        && let Some(previous) = files.remove(ix)
        && file.cursor_position.is_none()
    {
        file.cursor_position = previous.cursor_position;
    }
    files.push_front(file);
    files.truncate(RECENT_PATH_COUNT_MAX);
}
