use collections::{HashMap, HashSet};
use gpui::{App, Context, Entity, WeakEntity};
use rpc::proto;
use settings::{Settings as _, WorktreeId};
use std::{path::Path, sync::Arc};
use util::debug_panic;

use crate::{
    project_settings::ProjectSettings,
    trusted_worktrees::{
        PathTrust, TrustedWorktreesEvent, TrustedWorktreesStore, find_worktree_in_store,
    },
    worktree_store::WorktreeStore,
};

impl TrustedWorktreesStore {
    /// Adds certain entities on this host to the trusted list.
    /// This will emit [`TrustedWorktreesEvent::Trusted`] event for all passed entries
    /// and the ones that got auto trusted based on trust hierarchy (see module-level docs).
    pub fn trust(
        &mut self,
        worktree_store: &Entity<WorktreeStore>,
        mut trusted_paths: HashSet<PathTrust>,
        cx: &mut Context<Self>,
    ) {
        let weak_worktree_store = worktree_store.downgrade();
        let mut new_trusted_single_file_worktrees = HashSet::default();
        let mut new_trusted_other_worktrees = HashSet::default();
        let mut new_trusted_abs_paths = HashSet::default();
        for trusted_path in trusted_paths.iter().chain(
            self.trusted_paths
                .remove(&weak_worktree_store)
                .iter()
                .flat_map(|current_trusted| current_trusted.iter()),
        ) {
            match trusted_path {
                PathTrust::Worktree(worktree_id) => {
                    if let Some(restricted_worktrees) =
                        self.restricted.get_mut(&weak_worktree_store)
                    {
                        restricted_worktrees.remove(worktree_id);
                        if restricted_worktrees.is_empty() {
                            self.restricted.remove(&weak_worktree_store);
                        }
                    };

                    if let Some(worktree) =
                        worktree_store.read(cx).worktree_for_id(*worktree_id, cx)
                    {
                        if worktree.read(cx).is_single_file() {
                            new_trusted_single_file_worktrees.insert(*worktree_id);
                        } else {
                            new_trusted_other_worktrees
                                .insert((worktree.read(cx).abs_path(), *worktree_id));
                        }
                    }
                }
                PathTrust::AbsPath(abs_path) => {
                    debug_assert!(
                        util::paths::is_absolute(
                            &abs_path.to_string_lossy(),
                            worktree_store.read(cx).path_style()
                        ),
                        "Cannot trust non-absolute path {abs_path:?} on path style {style:?}",
                        style = worktree_store.read(cx).path_style()
                    );
                    if let Some((worktree_id, is_file)) =
                        find_worktree_in_store(worktree_store.read(cx), abs_path, cx)
                    {
                        if is_file {
                            new_trusted_single_file_worktrees.insert(worktree_id);
                        } else {
                            new_trusted_other_worktrees
                                .insert((Arc::from(abs_path.as_path()), worktree_id));
                        }
                    }
                    new_trusted_abs_paths.insert(abs_path.clone());
                }
            }
        }

        new_trusted_other_worktrees.retain(|(worktree_abs_path, _)| {
            new_trusted_abs_paths
                .iter()
                .all(|new_trusted_path| !worktree_abs_path.starts_with(new_trusted_path))
        });
        if !new_trusted_other_worktrees.is_empty() {
            new_trusted_single_file_worktrees.clear();
        }

        if let Some(restricted_worktrees) = self.restricted.remove(&weak_worktree_store) {
            let new_restricted_worktrees = restricted_worktrees
                .into_iter()
                .filter(|restricted_worktree| {
                    let Some(worktree) = worktree_store
                        .read(cx)
                        .worktree_for_id(*restricted_worktree, cx)
                    else {
                        return false;
                    };
                    let is_file = worktree.read(cx).is_single_file();

                    // When trusting an abs path on the host, we transitively trust all single file worktrees on this host too.
                    if is_file && !new_trusted_abs_paths.is_empty() {
                        trusted_paths.insert(PathTrust::Worktree(*restricted_worktree));
                        return false;
                    }

                    let restricted_worktree_path = worktree.read(cx).abs_path();
                    let retain = (!is_file || new_trusted_other_worktrees.is_empty())
                        && new_trusted_abs_paths.iter().all(|new_trusted_path| {
                            !restricted_worktree_path.starts_with(new_trusted_path)
                        });
                    if !retain {
                        trusted_paths.insert(PathTrust::Worktree(*restricted_worktree));
                    }
                    retain
                })
                .collect();
            self.restricted
                .insert(weak_worktree_store.clone(), new_restricted_worktrees);
        }

        {
            let trusted_paths = self
                .trusted_paths
                .entry(weak_worktree_store.clone())
                .or_default();
            trusted_paths.extend(new_trusted_abs_paths.into_iter().map(PathTrust::AbsPath));
            trusted_paths.extend(
                new_trusted_other_worktrees
                    .into_iter()
                    .map(|(_, worktree_id)| PathTrust::Worktree(worktree_id)),
            );
            trusted_paths.extend(
                new_trusted_single_file_worktrees
                    .into_iter()
                    .map(PathTrust::Worktree),
            );
        }

        if let Some(store_data) = self.worktree_stores.get(&weak_worktree_store) {
            if let Some((upstream_client, upstream_project_id)) = &store_data.upstream_client {
                let trusted_paths = trusted_paths
                    .iter()
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
        }
        cx.emit(TrustedWorktreesEvent::Trusted(
            weak_worktree_store,
            trusted_paths,
        ));
    }

    /// Restricts certain entities on this host.
    /// This will emit [`TrustedWorktreesEvent::Restricted`] event for all passed entries.
    pub fn restrict(
        &mut self,
        worktree_store: WeakEntity<WorktreeStore>,
        restricted_paths: HashSet<PathTrust>,
        cx: &mut Context<Self>,
    ) {
        let mut restricted = HashSet::default();
        for restricted_path in restricted_paths {
            match restricted_path {
                PathTrust::Worktree(worktree_id) => {
                    self.restricted
                        .entry(worktree_store.clone())
                        .or_default()
                        .insert(worktree_id);
                    restricted.insert(PathTrust::Worktree(worktree_id));
                }
                PathTrust::AbsPath(..) => debug_panic!("Unexpected: cannot restrict an abs path"),
            }
        }

        cx.emit(TrustedWorktreesEvent::Restricted(
            worktree_store,
            restricted,
        ));
    }

    /// Erases all trust information.
    /// Requires Mav's restart to take proper effect.
    pub fn clear_trusted_paths(&mut self) {
        self.trusted_paths.clear();
        self.db_trusted_paths.clear();
    }

    /// Checks whether a certain worktree is trusted (or on a larger trust level).
    /// If not, emits [`TrustedWorktreesEvent::Restricted`] event if for the first time and not trusted, or no corresponding worktree store was found.
    ///
    /// No events or data adjustment happens when `trust_all_worktrees` auto trust is enabled.
    pub fn can_trust(
        &mut self,
        worktree_store: &Entity<WorktreeStore>,
        worktree_id: WorktreeId,
        cx: &mut Context<Self>,
    ) -> bool {
        if ProjectSettings::get_global(cx).session.trust_all_worktrees {
            return true;
        }

        let weak_worktree_store = worktree_store.downgrade();
        let Some(worktree) = worktree_store.read(cx).worktree_for_id(worktree_id, cx) else {
            return false;
        };
        let worktree_path = worktree.read(cx).abs_path();
        // Mav opened an "internal" directory: e.g. a tmp dir for `keymap_editor.rs` needs.
        if !worktree.read(cx).is_visible() {
            log::debug!("Skipping worktree trust checks for not visible {worktree_path:?}");
            return true;
        }

        let is_file = worktree.read(cx).is_single_file();
        if self
            .restricted
            .get(&weak_worktree_store)
            .is_some_and(|restricted_worktrees| restricted_worktrees.contains(&worktree_id))
        {
            return false;
        }

        if self
            .trusted_paths
            .get(&weak_worktree_store)
            .is_some_and(|trusted_paths| trusted_paths.contains(&PathTrust::Worktree(worktree_id)))
        {
            return true;
        }

        // * Single files are auto-approved when something else (not a single file) was approved on this host already.
        // * If parent path is trusted already, this worktree is stusted also.
        //
        // See module documentation for details on trust level.
        if let Some(trusted_paths) = self.trusted_paths.get(&weak_worktree_store) {
            let auto_trusted = worktree_store.read_with(cx, |worktree_store, cx| {
                trusted_paths.iter().any(|trusted_path| match trusted_path {
                    PathTrust::Worktree(worktree_id) => worktree_store
                        .worktree_for_id(*worktree_id, cx)
                        .is_some_and(|worktree| {
                            let worktree = worktree.read(cx);
                            worktree_path.starts_with(&worktree.abs_path())
                                || (is_file && !worktree.is_single_file())
                        }),
                    PathTrust::AbsPath(trusted_path) => {
                        is_file || worktree_path.starts_with(trusted_path)
                    }
                })
            });
            if auto_trusted {
                return true;
            }
        }

        self.restricted
            .entry(weak_worktree_store.clone())
            .or_default()
            .insert(worktree_id);
        log::info!("Worktree {worktree_path:?} is not trusted");
        if let Some(store_data) = self.worktree_stores.get(&weak_worktree_store) {
            if let Some((downstream_client, downstream_project_id)) = &store_data.downstream_client
            {
                downstream_client
                    .send(proto::RestrictWorktrees {
                        project_id: downstream_project_id.0,
                        worktree_ids: vec![worktree_id.to_proto()],
                    })
                    .ok();
            }
            if let Some((upstream_client, upstream_project_id)) = &store_data.upstream_client {
                upstream_client
                    .send(proto::RestrictWorktrees {
                        project_id: upstream_project_id.0,
                        worktree_ids: vec![worktree_id.to_proto()],
                    })
                    .ok();
            }
        }
        cx.emit(TrustedWorktreesEvent::Restricted(
            weak_worktree_store,
            HashSet::from_iter([PathTrust::Worktree(worktree_id)]),
        ));
        false
    }

    /// Lists all explicitly restricted worktrees (via [`TrustedWorktreesStore::can_trust`] method calls) for a particular worktree store on a particular host.
    pub fn restricted_worktrees(
        &self,
        worktree_store: &Entity<WorktreeStore>,
        cx: &App,
    ) -> HashSet<(WorktreeId, Arc<Path>)> {
        let mut single_file_paths = HashSet::default();

        let other_paths = self
            .restricted
            .get(&worktree_store.downgrade())
            .into_iter()
            .flatten()
            .filter_map(|&restricted_worktree_id| {
                let worktree = worktree_store
                    .read(cx)
                    .worktree_for_id(restricted_worktree_id, cx)?;
                let worktree = worktree.read(cx);
                let abs_path = worktree.abs_path();
                if worktree.is_single_file() {
                    single_file_paths.insert((restricted_worktree_id, abs_path));
                    None
                } else {
                    Some((restricted_worktree_id, abs_path))
                }
            })
            .collect::<HashSet<_>>();

        if !other_paths.is_empty() {
            return other_paths;
        } else {
            single_file_paths
        }
    }

    /// Switches the "trust nothing" mode to "automatically trust everything".
    /// This does not influence already persisted data, but stops adding new worktrees there.
    pub fn auto_trust_all(&mut self, cx: &mut Context<Self>) {
        for (worktree_store, worktrees) in std::mem::take(&mut self.restricted).into_iter().fold(
            HashMap::default(),
            |mut acc, (remote_host, worktrees)| {
                acc.entry(remote_host)
                    .or_insert_with(HashSet::default)
                    .extend(worktrees.into_iter().map(PathTrust::Worktree));
                acc
            },
        ) {
            if let Some(worktree_store) = worktree_store.upgrade() {
                self.trust(&worktree_store, worktrees, cx);
            }
        }
    }
}
