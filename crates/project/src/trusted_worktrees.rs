//! A module, responsible for managing the trust logic in Mav.
//!
//! It deals with multiple hosts, distinguished by [`RemoteHostLocation`].
//! Each [`crate::Project`] and `HeadlessProject` should call [`init_global`], if wants to establish the trust mechanism.
//! This will set up a [`gpui::Global`] with [`TrustedWorktrees`] entity that will persist, restore and allow querying for worktree trust.
//! It's also possible to subscribe on [`TrustedWorktreesEvent`] events of this entity to track trust changes dynamically.
//!
//! The implementation can synchronize trust information with the remote hosts: currently, WSL and SSH.
//! Docker and Collab remotes do not employ trust mechanism, as manage that themselves.
//!
//! Unless `trust_all_worktrees` auto trust is enabled, does not trust anything that was not persisted before.
//! When dealing with "restricted" and other related concepts in the API, it means all explicitly restricted, after any of the [`TrustedWorktreesStore::can_trust`] and [`TrustedWorktreesStore::can_trust_global`] calls.
//!
//! Mav does not consider invisible, `worktree.is_visible() == false` worktrees in Mav, as those are programmatically created inside Mav for internal needs, e.g. a tmp dir for `keymap_editor.rs` needs.
//!
//!
//! Path rust hierarchy.
//!
//! Mav has multiple layers of trust, based on the requests and [`PathTrust`] enum variants.
//! From the least to the most trusted level:
//!
//! * "single file worktree"
//!
//! After opening an empty Mav it's possible to open just a file, same as after opening a directory in Mav it's possible to open a file outside of this directory.
//! Usual scenario for both cases is opening Mav's settings.json file via `mav: open settings file` command: that starts a language server for a new file open, which originates from a newly created, single file worktree.
//!
//! Spawning a language server is potentially dangerous, and Mav needs to restrict that by default.
//! Each single file worktree requires a separate trust permission, unless a more global level is trusted.
//!
//! * "directory worktree"
//!
//! If a directory is open in Mav, it's a full worktree which may spawn multiple language servers associated with it.
//! Each such worktree requires a separate trust permission, so each separate directory worktree has to be trusted separately, unless a more global level is trusted.
//!
//! When a directory worktree is trusted and language servers are allowed to be downloaded and started, hence, "single file worktree" level of trust also.
//!
//! * "path override"
//!
//! To ease trusting multiple directory worktrees at once, it's possible to trust a parent directory of a certain directory worktree opened in Mav.
//! Trusting a directory means trusting all its subdirectories as well, including all current and potential directory worktrees.

mod trust_updates;
mod types;

use client::ProjectId;
use collections::{HashMap, HashSet};
use gpui::{App, AppContext as _, Context, Entity, EventEmitter, Global, Task, WeakEntity};
use rpc::{AnyProtoClient, proto};
use settings::WorktreeId;
use std::path::{Path, PathBuf};

use crate::worktree_store::WorktreeStore;
use types::TrustedPaths;
pub use types::{DbTrustedPaths, PathTrust, RemoteHostLocation, TrustedWorktreesEvent};

pub fn init(db_trusted_paths: DbTrustedPaths, cx: &mut App) {
    if TrustedWorktrees::try_get_global(cx).is_none() {
        let trusted_worktrees = cx.new(|_| TrustedWorktreesStore::new(db_trusted_paths));
        cx.set_global(TrustedWorktrees(trusted_worktrees))
    }
}

/// An initialization call to set up trust global for a particular project (remote or local).
pub fn track_worktree_trust(
    worktree_store: Entity<WorktreeStore>,
    remote_host: Option<RemoteHostLocation>,
    downstream_client: Option<(AnyProtoClient, ProjectId)>,
    upstream_client: Option<(AnyProtoClient, ProjectId)>,
    cx: &mut App,
) {
    match TrustedWorktrees::try_get_global(cx) {
        Some(trusted_worktrees) => {
            trusted_worktrees.update(cx, |trusted_worktrees, cx| {
                trusted_worktrees.add_worktree_store(
                    worktree_store.clone(),
                    remote_host,
                    downstream_client,
                    upstream_client.clone(),
                    cx,
                );

                if let Some((upstream_client, upstream_project_id)) = upstream_client {
                    let trusted_paths = trusted_worktrees
                        .trusted_paths
                        .get(&worktree_store.downgrade())
                        .into_iter()
                        .flatten()
                        .map(|trusted_path| trusted_path.to_proto())
                        .collect::<Vec<_>>();
                    if !trusted_paths.is_empty() {
                        upstream_client
                            .send(proto::TrustWorktrees {
                                project_id: upstream_project_id.0,
                                trusted_paths,
                            })
                            .ok();
                    }
                }
            });
        }
        None => log::debug!("No TrustedWorktrees initialized, not tracking worktree trust"),
    }
}

/// A collection of worktree trust metadata, can be accessed globally (if initialized) and subscribed to.
pub struct TrustedWorktrees(Entity<TrustedWorktreesStore>);

impl Global for TrustedWorktrees {}

impl TrustedWorktrees {
    pub fn try_get_global(cx: &App) -> Option<Entity<TrustedWorktreesStore>> {
        cx.try_global::<Self>().map(|this| this.0.clone())
    }

    /// Whether the given project store has any restricted worktrees.
    pub fn has_restricted_worktrees(worktree_store: &Entity<WorktreeStore>, cx: &App) -> bool {
        Self::try_get_global(cx)
            .map(|trusted| {
                trusted
                    .read(cx)
                    .has_restricted_worktrees(worktree_store, cx)
            })
            .unwrap_or(false)
    }
}

/// A collection of worktrees that are considered trusted and not trusted.
/// This can be used when checking for this criteria before enabling certain features.
///
/// Emits an event each time the worktree was checked and found not trusted,
/// or a certain worktree had been trusted.
#[derive(Debug)]
pub struct TrustedWorktreesStore {
    worktree_stores: HashMap<WeakEntity<WorktreeStore>, StoreData>,
    db_trusted_paths: DbTrustedPaths,
    trusted_paths: TrustedPaths,
    restricted: HashMap<WeakEntity<WorktreeStore>, HashSet<WorktreeId>>,
    worktree_trust_serialization: Task<()>,
}

#[derive(Debug, Default)]
struct StoreData {
    upstream_client: Option<(AnyProtoClient, ProjectId)>,
    downstream_client: Option<(AnyProtoClient, ProjectId)>,
    host: Option<RemoteHostLocation>,
}

impl EventEmitter<TrustedWorktreesEvent> for TrustedWorktreesStore {}

impl TrustedWorktreesStore {
    fn new(db_trusted_paths: DbTrustedPaths) -> Self {
        Self {
            db_trusted_paths,
            trusted_paths: HashMap::default(),
            worktree_stores: HashMap::default(),
            restricted: HashMap::default(),
            worktree_trust_serialization: Task::ready(()),
        }
    }

    /// Whether a particular worktree store has associated worktrees that are restricted, or an associated host is restricted.
    pub fn has_restricted_worktrees(
        &self,
        worktree_store: &Entity<WorktreeStore>,
        cx: &App,
    ) -> bool {
        self.restricted
            .get(&worktree_store.downgrade())
            .is_some_and(|restricted_worktrees| {
                restricted_worktrees.iter().any(|restricted_worktree| {
                    worktree_store
                        .read(cx)
                        .worktree_for_id(*restricted_worktree, cx)
                        .is_some()
                })
            })
    }

    #[cfg(feature = "test-support")]
    pub fn restricted_worktrees_for_store(
        &self,
        worktree_store: &Entity<WorktreeStore>,
    ) -> HashSet<WorktreeId> {
        self.restricted
            .get(&worktree_store.downgrade())
            .unwrap()
            .clone()
    }

    pub fn schedule_serialization<S>(&mut self, cx: &mut Context<Self>, serialize: S)
    where
        S: FnOnce(HashMap<Option<RemoteHostLocation>, HashSet<PathBuf>>, &App) -> Task<()>
            + 'static,
    {
        self.worktree_trust_serialization = serialize(self.trusted_paths_for_serialization(cx), cx);
    }

    fn trusted_paths_for_serialization(
        &mut self,
        cx: &mut Context<Self>,
    ) -> HashMap<Option<RemoteHostLocation>, HashSet<PathBuf>> {
        let new_trusted_paths = self
            .trusted_paths
            .iter()
            .filter_map(|(worktree_store, paths)| {
                let host = self.worktree_stores.get(&worktree_store)?.host.clone();
                let abs_paths = paths
                    .iter()
                    .flat_map(|path| match path {
                        PathTrust::Worktree(worktree_id) => worktree_store
                            .upgrade()
                            .and_then(|worktree_store| {
                                worktree_store.read(cx).worktree_for_id(*worktree_id, cx)
                            })
                            .map(|worktree| worktree.read(cx).abs_path().to_path_buf()),
                        PathTrust::AbsPath(abs_path) => Some(abs_path.clone()),
                    })
                    .collect::<HashSet<_>>();
                Some((host, abs_paths))
            })
            .chain(self.db_trusted_paths.drain())
            .fold(HashMap::default(), |mut acc, (host, paths)| {
                acc.entry(host)
                    .or_insert_with(HashSet::default)
                    .extend(paths);
                acc
            });

        self.db_trusted_paths = new_trusted_paths.clone();
        new_trusted_paths
    }

    fn add_worktree_store(
        &mut self,
        worktree_store: Entity<WorktreeStore>,
        remote_host: Option<RemoteHostLocation>,
        downstream_client: Option<(AnyProtoClient, ProjectId)>,
        upstream_client: Option<(AnyProtoClient, ProjectId)>,
        cx: &mut Context<Self>,
    ) {
        self.worktree_stores
            .retain(|worktree_store, _| worktree_store.is_upgradable());
        let weak_worktree_store = worktree_store.downgrade();
        self.worktree_stores.insert(
            weak_worktree_store.clone(),
            StoreData {
                host: remote_host.clone(),
                downstream_client,
                upstream_client,
            },
        );

        let mut new_trusted_paths = HashSet::default();
        if let Some(db_trusted_paths) = self.db_trusted_paths.get(&remote_host) {
            new_trusted_paths.extend(db_trusted_paths.clone().into_iter().map(PathTrust::AbsPath));
        }
        if let Some(trusted_paths) = self.trusted_paths.remove(&weak_worktree_store) {
            new_trusted_paths.extend(trusted_paths);
        }
        if !new_trusted_paths.is_empty() {
            self.trusted_paths.insert(
                weak_worktree_store,
                new_trusted_paths
                    .into_iter()
                    .map(|path_trust| match path_trust {
                        PathTrust::AbsPath(abs_path) => {
                            find_worktree_in_store(worktree_store.read(cx), &abs_path, cx)
                                .map(|(worktree_id, _)| PathTrust::Worktree(worktree_id))
                                .unwrap_or_else(|| PathTrust::AbsPath(abs_path))
                        }
                        other => other,
                    })
                    .collect(),
            );
        }
    }
}

fn find_worktree_in_store(
    worktree_store: &WorktreeStore,
    abs_path: &Path,
    cx: &App,
) -> Option<(WorktreeId, bool)> {
    let (worktree, path_in_worktree) = worktree_store.find_worktree(&abs_path, cx)?;
    if path_in_worktree.is_empty() {
        Some((worktree.read(cx).id(), worktree.read(cx).is_single_file()))
    } else {
        None
    }
}
