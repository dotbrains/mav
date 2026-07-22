use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use agent::{MAV_AGENT_ID, ThreadStore};
use agent_client_protocol::schema::v1 as acp;
use anyhow::Context as _;
use chrono::{DateTime, Utc};
use collections::{HashMap, HashSet};
use db::{
    kvp::KeyValueStore,
    sqlez::{
        bindable::{Bind, Column},
        domain::Domain,
        statement::Statement,
        thread_safe_connection::ThreadSafeConnection,
    },
    sqlez_macros::sql,
};
use fs::Fs;
use futures::{FutureExt, future::Shared};
use gpui::{AppContext as _, Entity, Global, Subscription, Task, TaskExt};
pub use project::WorktreePaths;
use project::{AgentId, linked_worktree_short_name};
use remote::{RemoteConnectionOptions, same_remote_connection_identity};
use ui::{App, Context, SharedString, ThreadItemWorktreeInfo, WorktreeKind};
use util::ResultExt as _;
use workspace::{PathList, SerializedWorkspaceLocation, WorkspaceDb};

use crate::DEFAULT_THREAD_TITLE;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub struct ThreadId(uuid::Uuid);

impl ThreadId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }

    /// Stable, hyphenated string form suitable for use as a key.
    pub fn to_key_string(&self) -> String {
        self.0.hyphenated().to_string()
    }
}

impl Bind for ThreadId {
    fn bind(&self, statement: &Statement, start_index: i32) -> anyhow::Result<i32> {
        self.0.bind(statement, start_index)
    }
}

impl Column for ThreadId {
    fn column(statement: &mut Statement, start_index: i32) -> anyhow::Result<(Self, i32)> {
        let (uuid, next) = Column::column(statement, start_index)?;
        Ok((ThreadId(uuid), next))
    }
}

const THREAD_REMOTE_CONNECTION_MIGRATION_KEY: &str = "thread-metadata-remote-connection-backfill";
const THREAD_ID_MIGRATION_KEY: &str = "thread-metadata-thread-id-backfill";

/// List all sidebar thread metadata from an arbitrary SQLite connection.
///
/// This is used to read thread metadata from another release channel's
/// database without opening a full `ThreadSafeConnection`.
pub(crate) fn list_thread_metadata_from_connection(
    connection: &db::sqlez::connection::Connection,
) -> anyhow::Result<Vec<ThreadMetadata>> {
    connection.select::<ThreadMetadata>(ThreadMetadataDb::LIST_QUERY)?()
}

/// Run the `ThreadMetadataDb` migrations on a raw connection.
///
/// This is used in tests to set up the sidebar_threads schema in a
/// temporary database.
#[cfg(test)]
pub(crate) fn run_thread_metadata_migrations(connection: &db::sqlez::connection::Connection) {
    connection
        .migrate(
            ThreadMetadataDb::NAME,
            ThreadMetadataDb::MIGRATIONS,
            &mut |_, _, _| false,
        )
        .expect("thread metadata migrations should succeed");
}

#[path = "thread_metadata_store/metadata.rs"]
mod metadata;
#[path = "thread_metadata_store/migrations.rs"]
mod migrations;
pub use metadata::{ArchivedGitWorktree, ThreadMetadata, worktree_info_from_thread_paths};
#[path = "thread_metadata_store/events.rs"]
mod events;
#[path = "thread_metadata_store/store_archives.rs"]
mod store_archives;
#[path = "thread_metadata_store/store_paths.rs"]
mod store_paths;
#[path = "thread_metadata_store/store_read.rs"]
mod store_read;
#[path = "thread_metadata_store/store_runtime.rs"]
mod store_runtime;
#[path = "thread_metadata_store/store_save.rs"]
mod store_save;
pub use events::ThreadMetadataStoreEvent;
#[path = "thread_metadata_store/columns.rs"]
mod columns;
#[path = "thread_metadata_store/db_domain.rs"]
mod db_domain;
#[path = "thread_metadata_store/db_methods.rs"]
mod db_methods;
#[cfg(test)]
#[path = "thread_metadata_store/tests.rs"]
mod tests;
struct GlobalThreadMetadataStore(Entity<ThreadMetadataStore>);
impl Global for GlobalThreadMetadataStore {}

/// Lightweight metadata for any thread (native or ACP), enough to populate
/// the sidebar list and route to the correct load path when clicked.
/// The store holds all metadata needed to show threads in the sidebar/the archive.
///
/// Listens to ConversationView events and updates metadata when the root thread changes.
#[derive(Debug, Clone, PartialEq)]
pub struct ThreadMetadataStore {
    db: ThreadMetadataDb,
    threads: HashMap<ThreadId, ThreadMetadata>,
    threads_by_paths: HashMap<PathList, HashSet<ThreadId>>,
    threads_by_main_paths: HashMap<PathList, HashSet<ThreadId>>,
    threads_by_session: HashMap<acp::SessionId, ThreadId>,
    reload_task: Option<Shared<Task<()>>>,
    conversation_subscriptions: HashMap<gpui::EntityId, Subscription>,
    pending_thread_ops_tx: async_channel::Sender<DbOperation>,
    in_flight_archives: HashMap<ThreadId, (Task<()>, async_channel::Sender<()>)>,
    _db_operations_task: Task<()>,
}

#[derive(Debug, PartialEq)]
enum DbOperation {
    Upsert(ThreadMetadata),
    Delete(ThreadId),
}

impl DbOperation {
    fn id(&self) -> ThreadId {
        match self {
            DbOperation::Upsert(thread) => thread.thread_id,
            DbOperation::Delete(thread_id) => *thread_id,
        }
    }
}

/// Override for the test DB name used by `ThreadMetadataStore::init_global`.
/// When set as a GPUI global, `init_global` uses this name instead of
/// deriving one from the thread name. This prevents data from leaking
/// across proptest cases that share a thread name.
#[cfg(any(test, feature = "test-support"))]
pub struct TestMetadataDbName(pub String);
#[cfg(any(test, feature = "test-support"))]
impl gpui::Global for TestMetadataDbName {}

#[cfg(any(test, feature = "test-support"))]
impl TestMetadataDbName {
    pub fn global(cx: &App) -> String {
        cx.try_global::<Self>()
            .map(|g| g.0.clone())
            .unwrap_or_else(|| {
                let thread = std::thread::current();
                let test_name = thread.name().unwrap_or("unknown_test");
                format!("THREAD_METADATA_DB_{}", test_name)
            })
    }
}

struct ThreadMetadataDb(ThreadSafeConnection);
