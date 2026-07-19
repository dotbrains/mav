use super::*;
use zeta_prompt::{ContextSource, RelatedExcerpt, RelatedFile};

fn excerpt(
    row_range: std::ops::Range<u32>,
    text: &str,
    order: usize,
    context_source: ContextSource,
) -> RelatedExcerpt {
    RelatedExcerpt {
        row_range,
        text: Arc::from(text),
        order,
        context_source,
    }
}

#[test]
fn test_coalesce_touching_excerpts() {
    let mut excerpts = vec![
        excerpt(2..4, "line2\nline3\n", 1, ContextSource::EditHistory),
        excerpt(0..2, "line0\nline1\n", 3, ContextSource::Bm25),
        excerpt(4..6, "line4\nline5\n", 2, ContextSource::OracleSnippet),
        excerpt(10..12, "line10\nline11\n", 0, ContextSource::Bm25),
    ];
    coalesce_touching_excerpts(&mut excerpts);

    assert_eq!(excerpts.len(), 2);
    assert_eq!(excerpts[0].row_range, 0..6);
    assert_eq!(
        excerpts[0].text.as_ref(),
        "line0\nline1\nline2\nline3\nline4\nline5\n"
    );
    assert_eq!(excerpts[0].order, 1);
    assert_eq!(excerpts[0].context_source, ContextSource::EditHistory);
    assert_eq!(excerpts[1].row_range, 10..12);
}

#[test]
fn test_coalesce_overlapping_excerpts_drops_duplicated_rows() {
    let mut excerpts = vec![
        excerpt(0..3, "line0\nline1\nline2\n", 0, ContextSource::Bm25),
        excerpt(
            2..5,
            "line2\nline3\nline4\n",
            1,
            ContextSource::OracleSnippet,
        ),
    ];
    coalesce_touching_excerpts(&mut excerpts);

    assert_eq!(excerpts.len(), 1);
    assert_eq!(excerpts[0].row_range, 0..5);
    assert_eq!(
        excerpts[0].text.as_ref(),
        "line0\nline1\nline2\nline3\nline4\n"
    );
    assert_eq!(excerpts[0].order, 0);
    assert_eq!(excerpts[0].context_source, ContextSource::Bm25);
}

#[test]
fn test_coalesce_contained_excerpt_upgrades_order() {
    let mut excerpts = vec![
        excerpt(0..4, "line0\nline1\nline2\nline3\n", 5, ContextSource::Bm25),
        excerpt(1..3, "line1\nline2\n", 2, ContextSource::OracleSnippet),
    ];
    coalesce_touching_excerpts(&mut excerpts);

    assert_eq!(excerpts.len(), 1);
    assert_eq!(excerpts[0].row_range, 0..4);
    assert_eq!(excerpts[0].text.as_ref(), "line0\nline1\nline2\nline3\n");
    assert_eq!(excerpts[0].order, 2);
    assert_eq!(excerpts[0].context_source, ContextSource::OracleSnippet);
}

#[test]
fn test_coalesce_handles_unterminated_final_line() {
    let mut excerpts = vec![
        excerpt(0..2, "line0\nline1\nline2", 0, ContextSource::GitLog),
        excerpt(2..4, "line2\nline3\n", 1, ContextSource::Bm25),
    ];
    coalesce_touching_excerpts(&mut excerpts);

    assert_eq!(excerpts.len(), 1);
    assert_eq!(excerpts[0].row_range, 0..4);
    assert_eq!(excerpts[0].text.as_ref(), "line0\nline1\nline2\nline3\n");
}

#[test]
fn test_merge_context_files_coalesces_across_sources() {
    let path: Arc<Path> = Path::new("root/src/lib.rs").into();
    let mut context_files = vec![RelatedFile {
        path: path.clone(),
        max_row: 100,
        excerpts: vec![excerpt(
            0..2,
            "line0\nline1\n",
            0,
            ContextSource::CurrentFile,
        )],
        in_open_source_repo: false,
    }];
    let new_files = vec![RelatedFile {
        path,
        max_row: 100,
        excerpts: vec![excerpt(2..4, "line2\nline3\n", 1, ContextSource::Bm25)],
        in_open_source_repo: false,
    }];
    merge_context_files(&mut context_files, new_files);

    assert_eq!(context_files.len(), 1);
    assert_eq!(context_files[0].excerpts.len(), 1);
    assert_eq!(context_files[0].excerpts[0].row_range, 0..4);
    assert_eq!(
        context_files[0].excerpts[0].text.as_ref(),
        "line0\nline1\nline2\nline3\n"
    );
}
