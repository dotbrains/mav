use super::*;

#[gpui::test]
fn test_path_for_file(cx: &mut App) {
    let file: Arc<dyn language::File> = Arc::new(TestFile {
        path: RelPath::empty_arc(),
        root_name: String::new(),
        local_root: None,
    });
    assert_eq!(path_for_file(&file, 0, false, cx), None);
}

#[gpui::test]
fn test_chunk_search_range_multi_line(cx: &mut App) {
    let text = "line one\nline two\nline three\nline four\nline five\nline six\n";
    let buffer = cx.new(|cx| Buffer::local(text, cx));
    let snapshot = buffer.read(cx).snapshot();

    let chunks = chunk_search_range_for_test(&snapshot, "line", 4, 0..text.len());

    assert_chunks_are_contiguous(&chunks, 0..text.len());
    assert!(
        chunks.len() <= 4,
        "got {} chunks, expected <= num_cpus (4)",
        chunks.len()
    );
    for chunk in &chunks {
        let end = chunk.end;
        assert!(
            end == text.len() || text.as_bytes()[end - 1] == b'\n',
            "chunk ending at {end} is not a line boundary",
        );
    }
}

#[gpui::test]
fn test_chunk_search_range_single_line(cx: &mut App) {
    let text = "hello world hello again";
    let buffer = cx.new(|cx| Buffer::local(text, cx));
    let snapshot = buffer.read(cx).snapshot();

    let chunks = chunk_search_range_for_test(&snapshot, "hello", 4, 0..text.len());
    assert_chunks_are_contiguous(&chunks, 0..text.len());
}

#[gpui::test]
fn test_chunk_search_range_empty_range(cx: &mut App) {
    let buffer = cx.new(|cx| Buffer::local("hello world", cx));
    let snapshot = buffer.read(cx).snapshot();

    let chunks = chunk_search_range_for_test(&snapshot, "hello", 4, 5..5);
    assert!(chunks.is_empty());
}

#[gpui::test]
fn test_chunk_search_range_does_not_start_at_zero(cx: &mut App) {
    let line = "abcdefghij\n";
    let text = line.repeat(20);
    let buffer = cx.new(|cx| Buffer::local(text.clone(), cx));
    let snapshot = buffer.read(cx).snapshot();

    let start = line.len() * 7;
    let end = line.len() * 14;
    let chunks = chunk_search_range_for_test(&snapshot, "abc", 4, start..end);

    assert_chunks_are_contiguous(&chunks, start..end);
}

fn chunk_search_range_for_test(
    snapshot: &language::BufferSnapshot,
    query: &str,
    num_cpus: u32,
    range: Range<usize>,
) -> Vec<Range<usize>> {
    let query = SearchQuery::text(
        query,
        false,
        false,
        false,
        Default::default(),
        Default::default(),
        false,
        None,
    )
    .unwrap();
    chunk_search_range(
        snapshot.text.clone(),
        &query,
        num_cpus,
        BufferOffset(range.start)..BufferOffset(range.end),
    )
    .collect()
}

#[track_caller]
fn assert_chunks_are_contiguous(chunks: &[Range<usize>], expected: Range<usize>) {
    assert!(!chunks.is_empty(), "expected at least one chunk");
    assert_eq!(
        chunks.first().unwrap().start,
        expected.start,
        "first chunk does not start at {}",
        expected.start
    );
    assert_eq!(
        chunks.last().unwrap().end,
        expected.end,
        "last chunk does not end at {}",
        expected.end
    );
    for chunk in chunks {
        assert!(chunk.start < chunk.end, "empty chunk: {:?}", chunk);
    }
    for window in chunks.windows(2) {
        assert_eq!(
            window[0].end, window[1].start,
            "gap or overlap between chunks {:?} and {:?}",
            window[0], window[1],
        );
    }
}

async fn deserialize_editor(
    item_id: ItemId,
    workspace_id: WorkspaceId,
    workspace: Entity<Workspace>,
    project: Entity<Project>,
    cx: &mut VisualTestContext,
) -> Entity<Editor> {
    workspace
        .update_in(cx, |workspace, window, cx| {
            let pane = workspace.active_pane();
            pane.update(cx, |_, cx| {
                Editor::deserialize(
                    project.clone(),
                    workspace.weak_handle(),
                    workspace_id,
                    item_id,
                    window,
                    cx,
                )
            })
        })
        .await
        .unwrap()
}
