use super::*;

pub(super) fn parse_metadata_table_rows(
    source: &str,
    source_range: Range<usize>,
) -> Option<Vec<MetadataRow>> {
    let mut rows = Vec::new();
    let mut line_start = source_range.start;

    for line in source[source_range].split_inclusive('\n') {
        let line_end = line_start + line.len();
        let content_end = line_start + line.trim_end_matches(['\r', '\n']).len();
        let content_range = line_start..content_end;
        let line_text = &source[content_range.clone()];

        if line_text.is_empty()
            || line_text
                .chars()
                .next()
                .is_some_and(|character| character.is_whitespace())
        {
            return None;
        }

        let delimiter = line_text.find(':')?;
        let key = trim_metadata_range(source, content_range.start..content_range.start + delimiter);
        let value = trim_metadata_range(
            source,
            content_range.start + delimiter + 1..content_range.end,
        );
        if key.is_empty() || value.is_empty() {
            return None;
        }

        rows.push(MetadataRow { key, value });
        line_start = line_end;
    }

    if rows.is_empty() { None } else { Some(rows) }
}

pub(super) fn trim_metadata_range(source: &str, range: Range<usize>) -> Range<usize> {
    let text = &source[range.clone()];
    let start_offset = text.len() - text.trim_start().len();
    let end_offset = text.trim_end().len();
    let start = range.start + start_offset;
    let end = (range.start + end_offset).max(start);
    start..end
}
