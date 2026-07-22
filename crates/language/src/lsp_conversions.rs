use super::*;

pub fn point_to_lsp(point: PointUtf16) -> lsp::Position {
    lsp::Position::new(point.row, point.column)
}

pub fn point_from_lsp(point: lsp::Position) -> Unclipped<PointUtf16> {
    Unclipped(PointUtf16::new(point.line, point.character))
}

pub fn range_to_lsp(range: Range<PointUtf16>) -> Result<lsp::Range> {
    anyhow::ensure!(
        range.start <= range.end,
        "Inverted range provided to an LSP request: {:?}-{:?}",
        range.start,
        range.end
    );
    Ok(lsp::Range {
        start: point_to_lsp(range.start),
        end: point_to_lsp(range.end),
    })
}

pub fn range_from_lsp(range: lsp::Range) -> Range<Unclipped<PointUtf16>> {
    let mut start = point_from_lsp(range.start);
    let mut end = point_from_lsp(range.end);
    if start > end {
        // We debug instead of warn so that this is not logged by default unless explicitly requested.
        // Using warn would write to the log file, and since we receive an enormous amount of
        // range_from_lsp calls (especially during completions), that can hang the main thread.
        //
        // See issue #36223.
        zlog::debug!("range_from_lsp called with inverted range {start:?}-{end:?}");
        mem::swap(&mut start, &mut end);
    }
    start..end
}
