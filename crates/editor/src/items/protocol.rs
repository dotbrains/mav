use super::*;

pub(super) fn serialize_selection(selection: &Selection<Anchor>) -> proto::Selection {
    proto::Selection {
        id: selection.id as u64,
        start: Some(serialize_anchor(&selection.start)),
        end: Some(serialize_anchor(&selection.end)),
        reversed: selection.reversed,
    }
}

pub(super) fn serialize_anchor(anchor: &Anchor) -> proto::EditorAnchor {
    match anchor {
        Anchor::Min => proto::EditorAnchor {
            excerpt_id: None,
            anchor: Some(proto::Anchor {
                replica_id: 0,
                timestamp: 0,
                offset: 0,
                bias: proto::Bias::Left as i32,
                buffer_id: None,
            }),
        },
        Anchor::Excerpt(_) => proto::EditorAnchor {
            excerpt_id: None,
            anchor: anchor.raw_text_anchor().map(|a| serialize_text_anchor(&a)),
        },
        Anchor::Max => proto::EditorAnchor {
            excerpt_id: None,
            anchor: Some(proto::Anchor {
                replica_id: u32::MAX,
                timestamp: u32::MAX,
                offset: u64::MAX,
                bias: proto::Bias::Right as i32,
                buffer_id: None,
            }),
        },
    }
}

pub(super) fn serialize_excerpt_range(
    range: ExcerptRange<language::Anchor>,
) -> proto::ExcerptRange {
    let context_start = language::proto::serialize_anchor(&range.context.start);
    let context_end = language::proto::serialize_anchor(&range.context.end);
    let primary_start = language::proto::serialize_anchor(&range.primary.start);
    let primary_end = language::proto::serialize_anchor(&range.primary.end);
    proto::ExcerptRange {
        context_start: Some(context_start),
        context_end: Some(context_end),
        primary_start: Some(primary_start),
        primary_end: Some(primary_end),
    }
}

pub(super) async fn deserialize_path_excerpts_and_wait_for_anchors(
    path_excerpts: Vec<proto::PathExcerpts>,
    buffers: &[Entity<Buffer>],
    cx: &mut AsyncWindowContext,
) -> Result<Vec<(PathKey, BufferId, Vec<ExcerptRange<language::Anchor>>)>> {
    let path_excerpts = path_excerpts
        .into_iter()
        .filter_map(|path_with_ranges| {
            let path_key = path_with_ranges.path_key.and_then(deserialize_path_key)?;
            let buffer_id = BufferId::new(path_with_ranges.buffer_id).ok()?;
            let ranges = path_with_ranges
                .ranges
                .into_iter()
                .filter_map(deserialize_excerpt_range)
                .collect::<Vec<_>>();
            Some((path_key, buffer_id, ranges))
        })
        .collect::<Vec<_>>();

    let wait_for_anchors = cx.update(|_, cx| {
        buffers
            .iter()
            .map(|buffer| {
                let buffer_id = buffer.read(cx).remote_id();
                let anchors = path_excerpts
                    .iter()
                    .filter(|(_, id, _)| *id == buffer_id)
                    .flat_map(|(_, _, ranges)| {
                        ranges.iter().flat_map(|range| {
                            [
                                range.context.start,
                                range.context.end,
                                range.primary.start,
                                range.primary.end,
                            ]
                        })
                    })
                    .collect::<Vec<_>>();
                buffer.update(cx, |buffer, _| buffer.wait_for_anchors(anchors))
            })
            .collect::<Vec<_>>()
    })?;
    // Without this wait, resolving these anchors later can race ahead of the
    // leader's pending buffer ops and trip `panic_bad_anchor` on a stale
    // snapshot.
    try_join_all(wait_for_anchors).await?;

    Ok(path_excerpts)
}

pub(super) fn deserialize_excerpt_range(
    excerpt_range: proto::ExcerptRange,
) -> Option<ExcerptRange<language::Anchor>> {
    let context = {
        let start = language::proto::deserialize_anchor(excerpt_range.context_start?)?;
        let end = language::proto::deserialize_anchor(excerpt_range.context_end?)?;
        start..end
    };
    let primary = excerpt_range
        .primary_start
        .zip(excerpt_range.primary_end)
        .and_then(|(start, end)| {
            let start = language::proto::deserialize_anchor(start)?;
            let end = language::proto::deserialize_anchor(end)?;
            Some(start..end)
        })
        .unwrap_or_else(|| context.clone());
    Some(ExcerptRange { context, primary })
}

pub(super) fn deserialize_selection(
    selection: proto::Selection,
    buffer: &MultiBufferSnapshot,
) -> Option<Selection<Anchor>> {
    Some(Selection {
        id: selection.id as usize,
        start: deserialize_anchor(selection.start?, buffer)?,
        end: deserialize_anchor(selection.end?, buffer)?,
        reversed: selection.reversed,
        goal: SelectionGoal::None,
    })
}

pub(super) fn deserialize_anchor(
    anchor: proto::EditorAnchor,
    buffer: &MultiBufferSnapshot,
) -> Option<Anchor> {
    let anchor = anchor.anchor?;
    if let Some(buffer_id) = anchor.buffer_id
        && BufferId::new(buffer_id).is_ok()
    {
        let text_anchor = language::proto::deserialize_anchor(anchor)?;
        buffer.anchor_in_buffer(text_anchor)
    } else {
        match proto::Bias::from_i32(anchor.bias)? {
            proto::Bias::Left => Some(Anchor::Min),
            proto::Bias::Right => Some(Anchor::Max),
        }
    }
}
