#[cfg(test)]
mod syntax_map_tests;

mod parse;
mod parse_helpers;
mod parse_invariants;
mod parse_order;
mod queries;
mod snapshot;
mod tree;
mod types;

use crate::{LanguageRegistry, QUERY_CURSORS};
use std::sync::{Arc, LazyLock};
use sum_tree::SumTree;
use text::{Anchor, BufferId};

pub use queries::TreeSitterOptions;
pub use tree::ToTreeSitterPoint;
pub use types::QueryCursorHandle;
pub use types::{OwnedSyntaxLayer, SyntaxLayer};
pub use types::{SyntaxMapCapture, SyntaxMapCaptures, SyntaxMapMatch, SyntaxMapMatches};

use types::SyntaxLayerEntry;
use types::SyntaxLayerSummary;

pub const MAX_BYTES_TO_QUERY: usize = 16 * 1024;

pub struct SyntaxMap {
    snapshot: SyntaxSnapshot,
    language_registry: Option<Arc<LanguageRegistry>>,
}

#[derive(Clone)]
pub struct SyntaxSnapshot {
    layers: SumTree<SyntaxLayerEntry>,
    parsed_version: clock::Global,
    interpolated_version: clock::Global,
    language_registry_version: usize,
    update_count: usize,
}

// Dropping deep treesitter Trees can be quite slow due to deallocating lots of memory.
// To avoid blocking the main thread, we offload the drop operation to a background thread.
impl Drop for SyntaxSnapshot {
    fn drop(&mut self) {
        static DROP_TX: LazyLock<std::sync::mpsc::Sender<SumTree<SyntaxLayerEntry>>> =
            LazyLock::new(|| {
                let (tx, rx) = std::sync::mpsc::channel();
                std::thread::Builder::new()
                    .name("SyntaxSnapshot::drop".into())
                    .spawn(move || while let Ok(_) = rx.recv() {})
                    .expect("failed to spawn drop thread");
                tx
            });
        // This does allocate a new Arc, but it's cheap and avoids blocking the main thread without needing to use an `Option` or `MaybeUninit`.
        let _ = DROP_TX.send(std::mem::replace(
            &mut self.layers,
            SumTree::from_summary(SyntaxLayerSummary {
                min_depth: Default::default(),
                max_depth: Default::default(),
                // Deliberately bogus anchors, doesn't matter in this context
                range: Anchor::min_min_range_for_buffer(BufferId::new(1).unwrap()),
                last_layer_range: Anchor::min_min_range_for_buffer(BufferId::new(1).unwrap()),
                last_layer_language: Default::default(),
                contains_unknown_injections: Default::default(),
            }),
        ));
    }
}

impl std::ops::Deref for SyntaxMap {
    type Target = SyntaxSnapshot;

    fn deref(&self) -> &Self::Target {
        &self.snapshot
    }
}
