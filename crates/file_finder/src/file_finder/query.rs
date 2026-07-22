use super::*;

#[derive(Debug, Clone)]
pub(super) struct FileSearchQuery {
    raw_query: String,
    file_query_end: Option<usize>,
    path_position: PathWithPosition,
    line_range: Option<RangeInclusive<u32>>,
}

impl FileSearchQuery {
    pub(super) fn path_query(&self) -> &str {
        match self.file_query_end {
            Some(file_path_end) => &self.raw_query[..file_path_end],
            None => &self.raw_query,
        }
    }

    pub(super) fn selection_range(&self, buffer_snapshot: &BufferSnapshot) -> Option<Range<Point>> {
        if let Some(line_range) = self.line_range.clone() {
            return Some(buffer_range_for_line_range(buffer_snapshot, line_range));
        }

        let row = self.path_position.row.map(|row| row.saturating_sub(1))?;
        let col = self.path_position.column.unwrap_or(0).saturating_sub(1);
        let point = buffer_snapshot.point_from_external_input(row, col);
        Some(point..point)
    }
}

pub(super) fn parse_file_search_query(raw_query: &str) -> FileSearchQuery {
    let raw_query = raw_query.trim().trim_end_matches(':').to_owned();

    if let Some((path_query, start_line, end_line)) = parse_line_range_query(&raw_query) {
        let path_query = path_query.to_owned();
        return FileSearchQuery {
            raw_query,
            file_query_end: Some(path_query.len()),
            path_position: PathWithPosition {
                path: PathBuf::from(&path_query),
                row: Some(start_line),
                column: None,
            },
            line_range: end_line.map(|end| start_line..=end),
        };
    }

    let path_position = PathWithPosition::parse_str(&raw_query);
    let path_str = path_position.path.to_str();
    let path_trimmed = path_str.unwrap_or(&raw_query).trim_end_matches(':');
    let file_query_end = if path_trimmed == raw_query {
        None
    } else {
        path_str.map(str::len)
    };

    FileSearchQuery {
        raw_query,
        file_query_end,
        path_position,
        line_range: None,
    }
}

fn parse_line_range_query(raw_query: &str) -> Option<(&str, u32, Option<u32>)> {
    let (path_query, line_range) = raw_query.rsplit_once(':')?;
    if path_query.is_empty() {
        return None;
    }

    let (start_line, end_line) = line_range.split_once('-')?;
    let start_line = start_line.parse::<u32>().ok()?;
    if start_line == 0 {
        return None;
    }

    let end_line = end_line
        .parse::<u32>()
        .ok()
        .filter(|&end| end != 0 && end >= start_line);

    Some((path_query, start_line, end_line))
}

fn buffer_range_for_line_range(
    buffer_snapshot: &BufferSnapshot,
    line_range: RangeInclusive<u32>,
) -> Range<Point> {
    let max_point = buffer_snapshot.max_point();
    let start_line = line_range.start().saturating_sub(1);
    let start_point = if start_line > max_point.row {
        max_point
    } else {
        Point::new(start_line, 0)
    };
    let end_line = *line_range.end();
    let end_point = if end_line > max_point.row {
        max_point
    } else {
        Point::new(end_line, 0)
    };

    start_point..end_point
}
