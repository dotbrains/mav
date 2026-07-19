use std::ops::Range;

use anyhow::Result;
use futures::future::join_all;
use gpui::{Context, Entity, Task};
use language::{Anchor, Buffer};
use text::OffsetRangeExt as _;

use crate::{CodeAction, Project};

impl Project {
    pub fn code_lens_actions(
        &mut self,
        buffer: &Entity<Buffer>,
        range: Range<Anchor>,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Vec<CodeAction>>>> {
        let snapshot = buffer.read(cx).snapshot();
        let range = range.to_point(&snapshot);
        let range_start = snapshot.anchor_before(range.start);
        let range_end = if range.start == range.end {
            range_start
        } else {
            snapshot.anchor_after(range.end)
        };
        let range = range_start..range_end;
        let lsp_store = self.lsp_store();
        let fetch_task =
            lsp_store.update(cx, |lsp_store, cx| lsp_store.code_lens_actions(buffer, cx));
        let buffer = buffer.clone();
        cx.spawn(async move |_, cx| {
            let Some(mut tagged) = fetch_task.await? else {
                return Ok(None);
            };
            let snapshot = buffer.read_with(cx, |buffer, _| buffer.snapshot());
            tagged.retain(|_, action| {
                range.start.cmp(&action.range.start, &snapshot).is_ge()
                    && range.end.cmp(&action.range.end, &snapshot).is_le()
            });
            let resolve_tasks = lsp_store.update(cx, |lsp_store, cx| {
                tagged
                    .iter()
                    .filter(|(_, action)| !action.resolved)
                    .map(|(id, action)| {
                        lsp_store.resolve_code_lens(&buffer, action.server_id, *id, cx)
                    })
                    .collect::<Vec<_>>()
            });
            for (resolved_id, resolved) in join_all(resolve_tasks).await.into_iter().flatten() {
                if let Some(slot) = tagged.get_mut(&resolved_id) {
                    *slot = resolved;
                }
            }
            // Sort by id to recover server-emit order at the menu boundary.
            let mut entries: Vec<_> = tagged.into_iter().collect();
            entries.sort_by_key(|(id, _)| *id);
            Ok(Some(entries.into_iter().map(|(_, a)| a).collect()))
        })
    }
}
