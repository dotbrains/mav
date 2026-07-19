use std::{collections::HashMap, ops::Range, path::Path, sync::Arc};

use anyhow::Result;
use futures::{StreamExt, TryFutureExt, TryStreamExt, stream::FuturesUnordered};
use gpui::{AppContext, Entity};
use itertools::Itertools;
use language::Buffer;
use text::Point;

use crate::{
    ProjectPath,
    bookmark_store::{BookmarkEntry, BookmarkStore},
    buffer_store::BufferStore,
    worktree_store::WorktreeStore,
};

impl BookmarkStore {
    pub async fn all_bookmark_locations(
        this: Entity<BookmarkStore>,
        cx: &mut (impl AppContext + Clone),
    ) -> Result<HashMap<Entity<Buffer>, Vec<Range<Point>>>> {
        Self::resolve_all(&this, cx).await?;

        cx.read_entity(&this, |this, cx| {
            let mut locations: HashMap<_, Vec<_>> = HashMap::new();
            for bookmarks in this.bookmarks.values().filter_map(BookmarkEntry::loaded) {
                let snapshot = cx.read_entity(bookmarks.buffer(), |b, _| b.snapshot());
                let ranges: Vec<Range<Point>> = bookmarks
                    .bookmarks()
                    .iter()
                    .map(|bookmark| {
                        let row = snapshot.summary_for_anchor::<Point>(&bookmark.anchor).row;
                        Point::row_range(row..row)
                    })
                    .collect();

                locations
                    .entry(bookmarks.buffer().clone())
                    .or_default()
                    .extend(ranges);
            }

            Ok(locations)
        })
    }

    /// Opens buffers for all unloaded bookmark entries and resolves them to anchors. This is used to show all bookmarks in a large multi-buffer.
    async fn resolve_all(this: &Entity<Self>, cx: &mut (impl AppContext + Clone)) -> Result<()> {
        let unloaded_paths: Vec<Arc<Path>> = cx.read_entity(&this, |this, _| {
            this.bookmarks
                .iter()
                .filter_map(|(path, entry)| match entry {
                    BookmarkEntry::Unloaded(_) => Some(path.clone()),
                    BookmarkEntry::Loaded(_) => None,
                })
                .collect_vec()
        });

        if unloaded_paths.is_empty() {
            return Ok(());
        }

        let worktree_store = cx.read_entity(&this, |this, _| this.worktree_store.clone());
        let buffer_store = cx.read_entity(&this, |this, _| this.buffer_store.clone());

        let open_tasks: FuturesUnordered<_> = unloaded_paths
            .iter()
            .map(|path| {
                open_path(path, &worktree_store, &buffer_store, cx.clone())
                    .map_err(move |e| (path, e))
                    .map_ok(move |b| (path, b))
            })
            .collect();

        let opened: Vec<_> = open_tasks
            .inspect_err(|(path, error)| {
                log::warn!(
                    "Could not open buffer for bookmarked path {}: {error}",
                    path.display()
                )
            })
            .filter_map(|res| async move { res.ok() })
            .collect()
            .await;

        cx.update_entity(&this, |this, cx| {
            for (path, buffer) in opened {
                this.resolve_anchors_if_needed(path, &buffer, cx);
            }
            cx.notify();
        });

        Ok(())
    }
}

async fn open_path(
    path: &Path,
    worktree_store: &Entity<WorktreeStore>,
    buffer_store: &Entity<BufferStore>,
    mut cx: impl AppContext,
) -> Result<Entity<Buffer>> {
    let (worktree, worktree_path) = cx
        .update_entity(&worktree_store, |worktree_store, cx| {
            worktree_store.find_or_create_worktree(path, false, cx)
        })
        .await?;

    let project_path = ProjectPath {
        worktree_id: cx.read_entity(&worktree, |worktree, _| worktree.id()),
        path: worktree_path,
    };

    let buffer = cx
        .update_entity(&buffer_store, |buffer_store, cx| {
            buffer_store.open_buffer(project_path, cx)
        })
        .await?;

    Ok(buffer)
}
