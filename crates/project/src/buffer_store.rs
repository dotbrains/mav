use crate::{
    ProjectPath,
    lsp_store::OpenLspBufferHandle,
    worktree_store::{WorktreeStore, WorktreeStoreEvent},
};
use anyhow::{Context as _, Result, anyhow};
use client::Client;
use collections::{HashMap, HashSet, hash_map};
use futures::{Future, FutureExt as _, channel::oneshot, future::Shared};
use gpui::{
    App, AppContext as _, AsyncApp, Context, Entity, EventEmitter, Subscription, Task, TaskExt,
    WeakEntity,
};
use language::{
    Buffer, BufferEvent, Capability, DiskState, File as _, Language, LineEnding, Operation,
    language_settings::{AllLanguageSettings, LineEndingSetting},
    proto::{
        deserialize_line_ending, deserialize_version, serialize_line_ending, serialize_version,
        split_operations,
    },
};
use rpc::{
    AnyProtoClient, ErrorCode, ErrorExt as _, TypedEnvelope,
    proto::{self, PeerId},
};

use settings::Settings;
use std::{io, sync::Arc, time::Instant};
use text::{BufferId, ReplicaId};
use util::{ResultExt as _, TryFutureExt, debug_panic, maybe, rel_path::RelPath};
use worktree::{File, PathChange, ProjectEntryId, Worktree, WorktreeId, WorktreeSettings};

/// A set of open buffers.
mod lifecycle;
mod local_store;
mod remote_store;
mod rpc_handlers;
mod sharing_search;

pub struct BufferStore {
    state: BufferStoreState,
    #[allow(clippy::type_complexity)]
    loading_buffers: HashMap<ProjectPath, Shared<Task<Result<Entity<Buffer>, Arc<anyhow::Error>>>>>,
    worktree_store: Entity<WorktreeStore>,
    opened_buffers: HashMap<BufferId, OpenBuffer>,
    path_to_buffer_id: HashMap<ProjectPath, BufferId>,
    downstream_client: Option<(AnyProtoClient, u64)>,
    shared_buffers: HashMap<proto::PeerId, HashMap<BufferId, SharedBuffer>>,
    non_searchable_buffers: HashSet<BufferId>,
    project_search: RemoteProjectSearchState,
}

#[derive(Default)]
struct RemoteProjectSearchState {
    // List of ongoing project search chunks from our remote host. Used by the side issuing a search RPC request.
    chunks: HashMap<u64, async_channel::Sender<BufferId>>,
    // Monotonously-increasing handle to hand out to remote host in order to identify the project search result chunk.
    next_id: u64,
    // Used by the side running the actual search for match candidates to potentially cancel the search prematurely.
    searches_in_progress: HashMap<(PeerId, u64), Task<Result<()>>>,
}

#[derive(Hash, Eq, PartialEq, Clone)]
struct SharedBuffer {
    buffer: Entity<Buffer>,
    lsp_handle: Option<OpenLspBufferHandle>,
}

enum BufferStoreState {
    Local(LocalBufferStore),
    Remote(RemoteBufferStore),
}

struct RemoteBufferStore {
    shared_with_me: HashSet<Entity<Buffer>>,
    upstream_client: AnyProtoClient,
    project_id: u64,
    loading_remote_buffers_by_id: HashMap<BufferId, Entity<Buffer>>,
    remote_buffer_listeners:
        HashMap<BufferId, Vec<oneshot::Sender<anyhow::Result<Entity<Buffer>>>>>,
    worktree_store: Entity<WorktreeStore>,
}

struct LocalBufferStore {
    local_buffer_ids_by_entry_id: HashMap<ProjectEntryId, BufferId>,
    worktree_store: Entity<WorktreeStore>,
    _subscription: Subscription,
}

enum OpenBuffer {
    Complete { buffer: WeakEntity<Buffer> },
    Operations(Vec<Operation>),
}

pub enum BufferStoreEvent {
    BufferAdded(Entity<Buffer>),
    SharedBufferClosed(proto::PeerId, BufferId),
    BufferDropped(BufferId),
    BufferChangedFilePath {
        buffer: Entity<Buffer>,
        old_file: Option<Arc<dyn language::File>>,
    },
}

#[derive(Default, Debug, Clone)]
pub struct ProjectTransaction(pub HashMap<Entity<Buffer>, language::Transaction>);

impl PartialEq for ProjectTransaction {
    fn eq(&self, other: &Self) -> bool {
        self.0.len() == other.0.len()
            && self.0.iter().all(|(buffer, transaction)| {
                other.0.get(buffer).is_some_and(|t| t.id == transaction.id)
            })
    }
}

impl EventEmitter<BufferStoreEvent> for BufferStore {}
impl OpenBuffer {
    fn upgrade(&self) -> Option<Entity<Buffer>> {
        match self {
            OpenBuffer::Complete { buffer, .. } => buffer.upgrade(),
            OpenBuffer::Operations(_) => None,
        }
    }
}

fn is_not_found_error(error: &anyhow::Error) -> bool {
    error
        .root_cause()
        .downcast_ref::<io::Error>()
        .is_some_and(|err| err.kind() == io::ErrorKind::NotFound)
}

fn apply_initial_line_ending(buffer: &mut Buffer, cx: &mut Context<Buffer>) {
    // Only applies for empty rope or a single line with no trailing newline.
    if buffer.max_point().row > 0 {
        return;
    }
    let location = buffer.file().map(|file| settings::SettingsLocation {
        worktree_id: file.worktree_id(cx),
        path: file.path().as_ref(),
    });
    let language = buffer.language().map(|l| l.name());
    let settings = AllLanguageSettings::get(location, cx).language(location, language.as_ref(), cx);
    let desired = match settings.line_ending {
        LineEndingSetting::Detect => return,
        LineEndingSetting::PreferLf | LineEndingSetting::EnforceLf => LineEnding::Unix,
        LineEndingSetting::PreferCrlf | LineEndingSetting::EnforceCrlf => LineEnding::Windows,
    };
    if buffer.line_ending() != desired {
        buffer.set_line_ending(desired, cx);
    }
}
