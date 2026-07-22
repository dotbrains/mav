mod background_scanner;
mod ignore;
mod worktree_file;
mod worktree_repository;
mod worktree_scan_state;
mod worktree_settings;
mod worktree_state;
mod worktree_types;

use ::ignore::gitignore::{Gitignore, GitignoreBuilder};
use anyhow::{Context as _, Result, anyhow};
use chardetng::EncodingDetector;
use clock::ReplicaId;
use collections::{BTreeMap, HashMap, HashSet, VecDeque};
use encoding_rs::Encoding;
use fs::{
    Fs, MTime, PathEvent, PathEventKind, RemoveOptions, TrashedEntry, Watcher, copy_recursive,
    read_dir_items,
};
use futures::{
    FutureExt as _, Stream, StreamExt,
    channel::{
        mpsc::{self, UnboundedSender},
        oneshot,
    },
    select_biased, stream,
    task::Poll,
};
use fuzzy::CharBag;
use git::{
    BISECT_LOG, COMMIT_MESSAGE, DOT_GIT, FETCH_HEAD, FSMONITOR_DAEMON, GC_PID, GITIGNORE,
    HOOKS_DIR, INFO_DIR, LFS_DIR, LOGS_DIR, LOGS_REF_STASH, OBJECTS_DIR, ORIG_HEAD,
    REBASE_APPLY_DIR, REBASE_MERGE_DIR, REPO_EXCLUDE, SEQUENCER_DIR, status::GitSummary,
};
use gpui::{
    App, AppContext as _, AsyncApp, BackgroundExecutor, Context, Entity, EventEmitter, Priority,
    Task,
};
use ignore::IgnoreStack;
use language::{ByteContent, DiskState, FILE_ANALYSIS_BYTES, analyze_byte_content};

use async_channel::{self, Sender};
use background_scanner::{BackgroundScanner, BackgroundScannerPhase};
use parking_lot::Mutex;
use paths::{local_settings_folder_name, local_vscode_folder_name};
use postage::{
    barrier,
    prelude::{Sink as _, Stream as _},
    watch,
};
use rpc::{
    AnyProtoClient,
    proto::{self, split_worktree_update},
};
pub use settings::WorktreeId;
use settings::{Settings, SettingsLocation, SettingsStore};
use smallvec::{SmallVec, smallvec};
use std::{
    any::Any,
    borrow::Borrow as _,
    cmp::Ordering,
    collections::hash_map,
    convert::TryFrom,
    ffi::OsStr,
    fmt,
    future::Future,
    mem::{self},
    ops::{Deref, DerefMut, Range},
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering::SeqCst},
    },
    time::{Duration, Instant},
};
use sum_tree::{Bias, Dimensions, Edit, KeyedItem, SeekTarget, SumTree, Summary, TreeMap, TreeSet};
use text::{LineEnding, Rope};
use util::{
    ResultExt, maybe,
    paths::{PathMatcher, PathStyle, SanitizedPath, home_dir},
    rel_path::{RelPath, RelPathBuf},
};
use worktree_repository::LocalRepositoryEntry;
use worktree_scan_state::{
    BackgroundScannerState, EventRoot, PathPrefixScanRequest, ScanRequest, ScanState,
    UpdateObservationState,
};
pub use worktree_settings::WorktreeSettings;
use worktree_state::{LocalSnapshot, LocalWorktree, RemoteWorktree, Snapshot};
pub use worktree_types::{
    CreatedEntry, Event, LoadedBinaryFile, LoadedFile, ProjectEntryId, WorkDirectory,
};

use crate::ignore::IgnoreKind;
pub use worktree_file::File;

pub const FS_WATCH_LATENCY: Duration = Duration::from_millis(100);

/// A set of local or remote files that are being opened as part of a project.
/// Responsible for tracking related FS (for local)/collab (for remote) events and corresponding updates.
/// Stores git repositories data and the diagnostics for the file(s).
///
/// Has an absolute path, and may be set to be visible in Mav UI or not.
/// May correspond to a directory or a single file.
/// Possible examples:
/// * a drag and dropped file — may be added as an invisible, "ephemeral" entry to the current worktree
/// * a directory opened in Mav — may be added as a visible entry to the current worktree
///
/// Uses [`Entry`] to track the state of each file/directory, can look up absolute paths for entries.
pub enum Worktree {
    Local(LocalWorktree),
    Remote(RemoteWorktree),
}

impl Deref for LocalRepositoryEntry {
    type Target = WorkDirectory;

    fn deref(&self) -> &Self::Target {
        &self.work_directory
    }
}

impl Deref for LocalSnapshot {
    type Target = Snapshot;

    fn deref(&self) -> &Self::Target {
        &self.snapshot
    }
}

impl DerefMut for LocalSnapshot {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.snapshot
    }
}

impl EventEmitter<Event> for Worktree {}

impl Worktree {
    pub async fn local(
        path: impl Into<Arc<Path>>,
        visible: bool,
        fs: Arc<dyn Fs>,
        next_entry_id: Arc<AtomicUsize>,
        scanning_enabled: bool,
        worktree_id: WorktreeId,
        cx: &mut AsyncApp,
    ) -> Result<Entity<Self>> {
        let abs_path = path.into();
        let metadata = fs
            .metadata(&abs_path)
            .await
            .context("failed to stat worktree path")?;

        let fs_case_sensitive = fs.is_case_sensitive().await;

        let root_file_handle = if metadata.as_ref().is_some() {
            fs.open_handle(&abs_path)
                .await
                .with_context(|| {
                    format!(
                        "failed to open local worktree root at {}",
                        abs_path.display()
                    )
                })
                .log_err()
        } else {
            None
        };

        let root_repo_common_dir = if visible {
            discover_root_repo_common_dir(&abs_path, fs.as_ref())
                .await
                .map(SanitizedPath::from_arc)
        } else {
            None
        };
        Ok(cx.new(move |cx: &mut Context<Worktree>| {
            let mut snapshot = LocalSnapshot {
                ignores_by_parent_abs_path: Default::default(),
                global_gitignore: Default::default(),
                repo_exclude_by_work_dir_abs_path: Default::default(),
                git_repositories: Default::default(),
                external_canonical_to_relative: Default::default(),
                snapshot: Snapshot::new(
                    worktree_id,
                    abs_path
                        .file_name()
                        .and_then(|f| f.to_str())
                        .map_or(RelPath::empty_arc(), |f| RelPath::unix(f).unwrap().into()),
                    abs_path.clone(),
                    PathStyle::local(),
                ),
                root_file_handle,
            };
            snapshot.root_repo_common_dir = root_repo_common_dir;

            let worktree_id = snapshot.id();
            let settings_location = Some(SettingsLocation {
                worktree_id,
                path: RelPath::empty(),
            });

            let settings = WorktreeSettings::get(settings_location, cx).clone();
            cx.observe_global::<SettingsStore>(move |this, cx| {
                if let Self::Local(this) = this {
                    let settings = WorktreeSettings::get(settings_location, cx).clone();
                    if this.settings != settings {
                        this.settings = settings;
                        this.restart_background_scanners(cx);
                    }
                }
            })
            .detach();

            let share_private_files = false;
            if let Some(metadata) = metadata {
                let mut entry = Entry::new(
                    RelPath::empty_arc(),
                    &metadata,
                    ProjectEntryId::new(&next_entry_id),
                    snapshot.root_char_bag,
                    None,
                );
                if metadata.is_dir {
                    if !scanning_enabled {
                        entry.kind = EntryKind::UnloadedDir;
                    }
                } else {
                    if let Some(file_name) = abs_path.file_name()
                        && let Some(file_name) = file_name.to_str()
                        && let Ok(path) = RelPath::unix(file_name)
                    {
                        entry.is_private = !share_private_files && settings.is_path_private(path);
                        entry.is_hidden = settings.is_path_hidden(path);
                    }
                }
                cx.foreground_executor()
                    .block_on(snapshot.insert_entry(entry, fs.as_ref()));
            }

            let (scan_requests_tx, scan_requests_rx) = async_channel::unbounded();
            let (path_prefixes_to_scan_tx, path_prefixes_to_scan_rx) = async_channel::unbounded();
            let mut worktree = LocalWorktree {
                share_private_files,
                next_entry_id,
                snapshot,
                is_scanning: watch::channel_with(true),
                snapshot_subscriptions: Default::default(),
                update_observer: None,
                scan_requests_tx,
                path_prefixes_to_scan_tx,
                _background_scanner_tasks: Vec::new(),
                fs,
                fs_case_sensitive,
                visible,
                settings,
                scanning_enabled,
                force_defer_watch: false,
            };
            worktree.start_background_scanner(scan_requests_rx, path_prefixes_to_scan_rx, cx);
            Worktree::Local(worktree)
        }))
    }

    pub fn remote(
        project_id: u64,
        replica_id: ReplicaId,
        worktree: proto::WorktreeMetadata,
        client: AnyProtoClient,
        path_style: PathStyle,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx: &mut Context<Self>| {
            let mut snapshot = Snapshot::new(
                WorktreeId::from_proto(worktree.id),
                RelPath::from_proto(&worktree.root_name).unwrap_or_else(|_| RelPath::empty_arc()),
                Path::new(&worktree.abs_path).into(),
                path_style,
            );

            snapshot.root_repo_common_dir = worktree
                .root_repo_common_dir
                .map(|p| SanitizedPath::new_arc(Path::new(&p)));

            let background_snapshot = Arc::new(Mutex::new((
                snapshot.clone(),
                Vec::<proto::UpdateWorktree>::new(),
            )));
            let (background_updates_tx, mut background_updates_rx) =
                mpsc::unbounded::<proto::UpdateWorktree>();
            let (mut snapshot_updated_tx, mut snapshot_updated_rx) = watch::channel();

            let worktree_id = snapshot.id();
            let settings_location = Some(SettingsLocation {
                worktree_id,
                path: RelPath::empty(),
            });

            let settings = WorktreeSettings::get(settings_location, cx).clone();
            let worktree = RemoteWorktree {
                client,
                project_id,
                replica_id,
                snapshot,
                file_scan_inclusions: settings.parent_dir_scan_inclusions.clone(),
                background_snapshot: background_snapshot.clone(),
                updates_tx: Some(background_updates_tx),
                update_observer: None,
                snapshot_subscriptions: Default::default(),
                visible: worktree.visible,
                disconnected: false,
                received_initial_update: false,
            };

            // Apply updates to a separate snapshot in a background task, then
            // send them to a foreground task which updates the model.
            cx.background_spawn(async move {
                while let Some(update) = background_updates_rx.next().await {
                    {
                        let mut lock = background_snapshot.lock();
                        lock.0.apply_remote_update(
                            update.clone(),
                            &settings.parent_dir_scan_inclusions,
                        );
                        lock.1.push(update);
                    }
                    snapshot_updated_tx.send(()).await.ok();
                }
            })
            .detach();

            // On the foreground task, update to the latest snapshot and notify
            // any update observer of all updates that led to that snapshot.
            cx.spawn(async move |this, cx| {
                while (snapshot_updated_rx.recv().await).is_some() {
                    this.update(cx, |this, cx| {
                        let this = this.as_remote_mut().unwrap();

                        // The watch channel delivers an initial signal before
                        // any real updates arrive. Skip these spurious wakeups.
                        if this.background_snapshot.lock().1.is_empty() {
                            return;
                        }

                        let old_root_repo_common_dir = this.snapshot.root_repo_common_dir.clone();
                        let mut changed_entries: Vec<(Arc<RelPath>, ProjectEntryId, PathChange)> =
                            Vec::new();
                        {
                            let mut lock = this.background_snapshot.lock();
                            // Replace the snapshot, keeping the previous one around so we can
                            // resolve the paths of removed entries (the new snapshot no longer
                            // contains them, and the wire format only carries their ids).
                            let old_snapshot = mem::replace(&mut this.snapshot, lock.0.clone());
                            for update in lock.1.drain(..) {
                                for entry_id in &update.removed_entries {
                                    let entry_id = ProjectEntryId::from_proto(*entry_id);
                                    if let Some(entry) = old_snapshot.entry_for_id(entry_id) {
                                        changed_entries.push((
                                            entry.path.clone(),
                                            entry_id,
                                            PathChange::Removed,
                                        ));
                                    }
                                }
                                for entry in &update.updated_entries {
                                    // Remote updates don't distinguish creation from
                                    // modification, so report `AddedOrUpdated`.
                                    if let Some(path) = RelPath::from_proto(&entry.path).log_err() {
                                        changed_entries.push((
                                            path,
                                            ProjectEntryId::from_proto(entry.id),
                                            PathChange::AddedOrUpdated,
                                        ));
                                    }
                                }
                                if let Some(tx) = &this.update_observer {
                                    tx.unbounded_send(update).ok();
                                }
                            }
                        };

                        if !changed_entries.is_empty() {
                            cx.emit(Event::UpdatedEntries(changed_entries.into()));
                        }
                        let is_first_update = !this.received_initial_update;
                        this.received_initial_update = true;
                        if this.snapshot.root_repo_common_dir != old_root_repo_common_dir
                            || (is_first_update && this.snapshot.root_repo_common_dir.is_none())
                        {
                            cx.emit(Event::UpdatedRootRepoCommonDir {
                                old: old_root_repo_common_dir,
                            });
                        }
                        cx.notify();
                        while let Some((scan_id, _)) = this.snapshot_subscriptions.front() {
                            if this.observed_snapshot(*scan_id) {
                                let (_, tx) = this.snapshot_subscriptions.pop_front().unwrap();
                                let _ = tx.send(());
                            } else {
                                break;
                            }
                        }
                    })?;
                }
                anyhow::Ok(())
            })
            .detach();

            Worktree::Remote(worktree)
        })
    }

    pub fn as_local(&self) -> Option<&LocalWorktree> {
        if let Worktree::Local(worktree) = self {
            Some(worktree)
        } else {
            None
        }
    }

    pub fn as_remote(&self) -> Option<&RemoteWorktree> {
        if let Worktree::Remote(worktree) = self {
            Some(worktree)
        } else {
            None
        }
    }

    pub fn as_local_mut(&mut self) -> Option<&mut LocalWorktree> {
        if let Worktree::Local(worktree) = self {
            Some(worktree)
        } else {
            None
        }
    }

    pub fn as_remote_mut(&mut self) -> Option<&mut RemoteWorktree> {
        if let Worktree::Remote(worktree) = self {
            Some(worktree)
        } else {
            None
        }
    }

    pub fn is_local(&self) -> bool {
        matches!(self, Worktree::Local(_))
    }

    pub fn is_remote(&self) -> bool {
        !self.is_local()
    }

    pub fn settings_location(&self, _: &Context<Self>) -> SettingsLocation<'static> {
        SettingsLocation {
            worktree_id: self.id(),
            path: RelPath::empty(),
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        match self {
            Worktree::Local(worktree) => worktree.snapshot.snapshot.clone(),
            Worktree::Remote(worktree) => worktree.snapshot.clone(),
        }
    }

    pub fn scan_id(&self) -> usize {
        match self {
            Worktree::Local(worktree) => worktree.snapshot.scan_id,
            Worktree::Remote(worktree) => worktree.snapshot.scan_id,
        }
    }

    pub fn metadata_proto(&self) -> proto::WorktreeMetadata {
        proto::WorktreeMetadata {
            id: self.id().to_proto(),
            root_name: self.root_name().to_proto(),
            visible: self.is_visible(),
            abs_path: self.abs_path().to_string_lossy().into_owned(),
            root_repo_common_dir: self
                .root_repo_common_dir()
                .map(|p| p.to_string_lossy().into_owned()),
        }
    }

    pub fn completed_scan_id(&self) -> usize {
        match self {
            Worktree::Local(worktree) => worktree.snapshot.completed_scan_id,
            Worktree::Remote(worktree) => worktree.snapshot.completed_scan_id,
        }
    }

    pub fn is_visible(&self) -> bool {
        match self {
            Worktree::Local(worktree) => worktree.visible,
            Worktree::Remote(worktree) => worktree.visible,
        }
    }

    pub fn replica_id(&self) -> ReplicaId {
        match self {
            Worktree::Local(_) => ReplicaId::LOCAL,
            Worktree::Remote(worktree) => worktree.replica_id,
        }
    }

    pub fn abs_path(&self) -> Arc<Path> {
        match self {
            Worktree::Local(worktree) => SanitizedPath::cast_arc(worktree.abs_path.clone()),
            Worktree::Remote(worktree) => SanitizedPath::cast_arc(worktree.abs_path.clone()),
        }
    }

    pub fn root_file(&self, cx: &Context<Self>) -> Option<Arc<File>> {
        let entry = self.root_entry()?;
        Some(File::for_entry(entry.clone(), cx.entity()))
    }

    pub fn observe_updates<F, Fut>(&mut self, project_id: u64, cx: &Context<Worktree>, callback: F)
    where
        F: 'static + Send + Fn(proto::UpdateWorktree) -> Fut,
        Fut: 'static + Send + Future<Output = bool>,
    {
        match self {
            Worktree::Local(this) => this.observe_updates(project_id, cx, callback),
            Worktree::Remote(this) => this.observe_updates(project_id, cx, callback),
        }
    }

    pub fn stop_observing_updates(&mut self) {
        match self {
            Worktree::Local(this) => {
                this.update_observer.take();
            }
            Worktree::Remote(this) => {
                this.update_observer.take();
            }
        }
    }

    pub fn wait_for_snapshot(
        &mut self,
        scan_id: usize,
    ) -> impl Future<Output = Result<()>> + use<> {
        match self {
            Worktree::Local(this) => this.wait_for_snapshot(scan_id).boxed(),
            Worktree::Remote(this) => this.wait_for_snapshot(scan_id).boxed(),
        }
    }

    #[cfg(feature = "test-support")]
    pub fn has_update_observer(&self) -> bool {
        match self {
            Worktree::Local(this) => this.update_observer.is_some(),
            Worktree::Remote(this) => this.update_observer.is_some(),
        }
    }

    pub fn load_file(&self, path: &RelPath, cx: &Context<Worktree>) -> Task<Result<LoadedFile>> {
        match self {
            Worktree::Local(this) => this.load_file(path, cx),
            Worktree::Remote(_) => {
                Task::ready(Err(anyhow!("remote worktrees can't yet load files")))
            }
        }
    }

    pub fn load_binary_file(
        &self,
        path: &RelPath,
        cx: &Context<Worktree>,
    ) -> Task<Result<LoadedBinaryFile>> {
        match self {
            Worktree::Local(this) => this.load_binary_file(path, cx),
            Worktree::Remote(_) => {
                Task::ready(Err(anyhow!("remote worktrees can't yet load binary files")))
            }
        }
    }

    pub fn write_file(
        &self,
        path: Arc<RelPath>,
        text: Rope,
        line_ending: LineEnding,
        encoding: &'static Encoding,
        has_bom: bool,
        cx: &Context<Worktree>,
    ) -> Task<Result<Arc<File>>> {
        match self {
            Worktree::Local(this) => {
                this.write_file(path, text, line_ending, encoding, has_bom, cx)
            }
            Worktree::Remote(_) => {
                Task::ready(Err(anyhow!("remote worktree can't yet write files")))
            }
        }
    }

    pub fn create_entry(
        &mut self,
        path: Arc<RelPath>,
        is_directory: bool,
        content: Option<Vec<u8>>,
        cx: &Context<Worktree>,
    ) -> Task<Result<CreatedEntry>> {
        let worktree_id = self.id();
        match self {
            Worktree::Local(this) => this.create_entry(path, is_directory, content, cx),
            Worktree::Remote(this) => {
                let project_id = this.project_id;
                let request = this.client.request(proto::CreateProjectEntry {
                    worktree_id: worktree_id.to_proto(),
                    project_id,
                    path: path.as_ref().to_proto(),
                    content,
                    is_directory,
                });
                cx.spawn(async move |this, cx| {
                    let response = request.await?;
                    match response.entry {
                        Some(entry) => this
                            .update(cx, |worktree, cx| {
                                worktree.as_remote_mut().unwrap().insert_entry(
                                    entry,
                                    response.worktree_scan_id as usize,
                                    cx,
                                )
                            })?
                            .await
                            .map(CreatedEntry::Included),
                        None => {
                            let abs_path =
                                this.read_with(cx, |worktree, _| worktree.absolutize(&path))?;
                            Ok(CreatedEntry::Excluded { abs_path })
                        }
                    }
                })
            }
        }
    }

    pub fn delete_entry(
        &mut self,
        entry_id: ProjectEntryId,
        trash: bool,
        cx: &mut Context<Worktree>,
    ) -> Option<Task<Result<Option<TrashedEntry>>>> {
        let task = match self {
            Worktree::Local(this) => this.delete_entry(entry_id, trash, cx),
            Worktree::Remote(this) => this.delete_entry(entry_id, trash, cx),
        }?;

        let entry = match &*self {
            Worktree::Local(this) => this.entry_for_id(entry_id),
            Worktree::Remote(this) => this.entry_for_id(entry_id),
        }?;

        let mut ids = vec![entry_id];
        let path = &*entry.path;

        self.get_children_ids_recursive(path, &mut ids);

        for id in ids {
            cx.emit(Event::DeletedEntry(id));
        }
        Some(task)
    }

    pub async fn restore_entry(
        trash_entry: TrashedEntry,
        worktree: Entity<Self>,
        cx: &mut AsyncApp,
    ) -> Result<RelPathBuf> {
        let is_local = worktree.read_with(cx, |this, _| this.is_local());
        if is_local {
            LocalWorktree::restore_entry(trash_entry, worktree, cx).await
        } else {
            // TODO(dino): Add support for restoring entries in remote worktrees.
            Err(anyhow!("Unsupported"))
        }
    }

    fn get_children_ids_recursive(&self, path: &RelPath, ids: &mut Vec<ProjectEntryId>) {
        let children_iter = self.child_entries(path);
        for child in children_iter {
            ids.push(child.id);
            self.get_children_ids_recursive(&child.path, ids);
        }
    }

    // pub fn rename_entry(
    //     &mut self,
    //     entry_id: ProjectEntryId,
    //     new_path: Arc<RelPath>,
    //     cx: &Context<Self>,
    // ) -> Task<Result<CreatedEntry>> {
    //     match self {
    //         Worktree::Local(this) => this.rename_entry(entry_id, new_path, cx),
    //         Worktree::Remote(this) => this.rename_entry(entry_id, new_path, cx),
    //     }
    // }

    pub fn copy_external_entries(
        &mut self,
        target_directory: Arc<RelPath>,
        paths: Vec<Arc<Path>>,
        fs: Arc<dyn Fs>,
        cx: &Context<Worktree>,
    ) -> Task<Result<Vec<ProjectEntryId>>> {
        match self {
            Worktree::Local(this) => this.copy_external_entries(target_directory, paths, cx),
            Worktree::Remote(this) => this.copy_external_entries(target_directory, paths, fs, cx),
        }
    }

    pub fn expand_entry(
        &mut self,
        entry_id: ProjectEntryId,
        cx: &Context<Worktree>,
    ) -> Option<Task<Result<()>>> {
        match self {
            Worktree::Local(this) => this.expand_entry(entry_id, cx),
            Worktree::Remote(this) => {
                let response = this.client.request(proto::ExpandProjectEntry {
                    project_id: this.project_id,
                    entry_id: entry_id.to_proto(),
                });
                Some(cx.spawn(async move |this, cx| {
                    let response = response.await?;
                    this.update(cx, |this, _| {
                        this.as_remote_mut()
                            .unwrap()
                            .wait_for_snapshot(response.worktree_scan_id as usize)
                    })?
                    .await?;
                    Ok(())
                }))
            }
        }
    }

    pub fn expand_all_for_entry(
        &mut self,
        entry_id: ProjectEntryId,
        cx: &Context<Worktree>,
    ) -> Option<Task<Result<()>>> {
        match self {
            Worktree::Local(this) => this.expand_all_for_entry(entry_id, cx),
            Worktree::Remote(this) => {
                let response = this.client.request(proto::ExpandAllForProjectEntry {
                    project_id: this.project_id,
                    entry_id: entry_id.to_proto(),
                });
                Some(cx.spawn(async move |this, cx| {
                    let response = response.await?;
                    this.update(cx, |this, _| {
                        this.as_remote_mut()
                            .unwrap()
                            .wait_for_snapshot(response.worktree_scan_id as usize)
                    })?
                    .await?;
                    Ok(())
                }))
            }
        }
    }

    pub async fn handle_create_entry(
        this: Entity<Self>,
        request: proto::CreateProjectEntry,
        mut cx: AsyncApp,
    ) -> Result<proto::ProjectEntryResponse> {
        let (scan_id, entry) = this.update(&mut cx, |this, cx| {
            anyhow::Ok((
                this.scan_id(),
                this.create_entry(
                    RelPath::from_proto(&request.path).with_context(|| {
                        format!("received invalid relative path {:?}", request.path)
                    })?,
                    request.is_directory,
                    request.content,
                    cx,
                ),
            ))
        })?;
        Ok(proto::ProjectEntryResponse {
            entry: match &entry.await? {
                CreatedEntry::Included(entry) => Some(entry.into()),
                CreatedEntry::Excluded { .. } => None,
            },
            worktree_scan_id: scan_id as u64,
        })
    }

    pub async fn handle_delete_entry(
        this: Entity<Self>,
        request: proto::DeleteProjectEntry,
        mut cx: AsyncApp,
    ) -> Result<proto::ProjectEntryResponse> {
        let (scan_id, task) = this.update(&mut cx, |this, cx| {
            (
                this.scan_id(),
                this.delete_entry(
                    ProjectEntryId::from_proto(request.entry_id),
                    request.use_trash,
                    cx,
                ),
            )
        });
        task.ok_or_else(|| anyhow::anyhow!("invalid entry"))?
            .await?;
        Ok(proto::ProjectEntryResponse {
            entry: None,
            worktree_scan_id: scan_id as u64,
        })
    }

    pub async fn handle_expand_entry(
        this: Entity<Self>,
        request: proto::ExpandProjectEntry,
        mut cx: AsyncApp,
    ) -> Result<proto::ExpandProjectEntryResponse> {
        let task = this.update(&mut cx, |this, cx| {
            this.expand_entry(ProjectEntryId::from_proto(request.entry_id), cx)
        });
        task.ok_or_else(|| anyhow::anyhow!("no such entry"))?
            .await?;
        let scan_id = this.read_with(&cx, |this, _| this.scan_id());
        Ok(proto::ExpandProjectEntryResponse {
            worktree_scan_id: scan_id as u64,
        })
    }

    pub async fn handle_expand_all_for_entry(
        this: Entity<Self>,
        request: proto::ExpandAllForProjectEntry,
        mut cx: AsyncApp,
    ) -> Result<proto::ExpandAllForProjectEntryResponse> {
        let task = this.update(&mut cx, |this, cx| {
            this.expand_all_for_entry(ProjectEntryId::from_proto(request.entry_id), cx)
        });
        task.ok_or_else(|| anyhow::anyhow!("no such entry"))?
            .await?;
        let scan_id = this.read_with(&cx, |this, _| this.scan_id());
        Ok(proto::ExpandAllForProjectEntryResponse {
            worktree_scan_id: scan_id as u64,
        })
    }

    pub fn is_single_file(&self) -> bool {
        self.root_dir().is_none()
    }

    /// For visible worktrees, returns the path with the worktree name as the first component.
    /// Otherwise, returns an absolute path.
    pub fn full_path(&self, worktree_relative_path: &RelPath) -> PathBuf {
        if self.is_visible() {
            self.root_name()
                .join(worktree_relative_path)
                .display(self.path_style)
                .to_string()
                .into()
        } else {
            let full_path = self.abs_path();
            let mut full_path_string = if self.is_local()
                && let Ok(stripped) = full_path.strip_prefix(home_dir())
            {
                self.path_style
                    .join("~", &*stripped.to_string_lossy())
                    .unwrap()
            } else {
                full_path.to_string_lossy().into_owned()
            };

            if worktree_relative_path.components().next().is_some() {
                full_path_string.push_str(self.path_style.primary_separator());
                full_path_string.push_str(&worktree_relative_path.display(self.path_style));
            }

            full_path_string.into()
        }
    }
}

impl LocalWorktree {
    pub fn fs(&self) -> &Arc<dyn Fs> {
        &self.fs
    }

    pub fn is_path_private(&self, path: &RelPath) -> bool {
        !self.share_private_files && self.settings.is_path_private(path)
    }

    pub fn fs_is_case_sensitive(&self) -> bool {
        self.fs_case_sensitive
    }

    fn restart_background_scanners(&mut self, cx: &Context<Worktree>) {
        let (scan_requests_tx, scan_requests_rx) = async_channel::unbounded();
        let (path_prefixes_to_scan_tx, path_prefixes_to_scan_rx) = async_channel::unbounded();
        self.scan_requests_tx = scan_requests_tx;
        self.path_prefixes_to_scan_tx = path_prefixes_to_scan_tx;

        self.start_background_scanner(scan_requests_rx, path_prefixes_to_scan_rx, cx);
        let always_included_entries = mem::take(&mut self.snapshot.always_included_entries);
        log::debug!(
            "refreshing entries for the following always included paths: {:?}",
            always_included_entries
        );

        // Cleans up old always included entries to ensure they get updated properly. Otherwise,
        // nested always included entries may not get updated and will result in out-of-date info.
        self.refresh_entries_for_paths(always_included_entries);
    }

    fn start_background_scanner(
        &mut self,
        scan_requests_rx: async_channel::Receiver<ScanRequest>,
        path_prefixes_to_scan_rx: async_channel::Receiver<PathPrefixScanRequest>,
        cx: &Context<Worktree>,
    ) {
        let snapshot = self.snapshot();
        let share_private_files = self.share_private_files;
        let next_entry_id = self.next_entry_id.clone();
        let fs = self.fs.clone();
        let scanning_enabled = self.scanning_enabled;
        let force_defer_watch = self.force_defer_watch;
        let track_git_repositories = self.visible;
        let settings = self.settings.clone();
        let (scan_states_tx, mut scan_states_rx) = mpsc::unbounded();
        let background_scanner = cx.background_spawn({
            let abs_path = snapshot.abs_path.as_path().to_path_buf();
            let background = cx.background_executor().clone();
            async move {
                let defer_watch =
                    force_defer_watch || (scanning_enabled && fs::requires_poll_watcher(&abs_path));

                let (events, watcher) = if scanning_enabled && !defer_watch {
                    fs.watch(&abs_path, FS_WATCH_LATENCY).await
                } else {
                    (Box::pin(stream::pending()) as _, Arc::new(NullWatcher) as _)
                };
                let fs_case_sensitive = fs.is_case_sensitive().await;

                let is_single_file = snapshot.snapshot.root_dir().is_none();
                let mut scanner = BackgroundScanner {
                    fs,
                    fs_case_sensitive,
                    status_updates_tx: scan_states_tx,
                    executor: background,
                    scan_requests_rx,
                    path_prefixes_to_scan_rx,
                    next_entry_id,
                    state: async_lock::Mutex::new(BackgroundScannerState {
                        prev_snapshot: snapshot.snapshot.clone(),
                        snapshot,
                        symlink_paths_by_target: Default::default(),
                        scanned_dirs: Default::default(),
                        watched_dir_abs_paths_by_entry_id: Default::default(),
                        scanning_enabled,
                        path_prefixes_to_scan: Default::default(),
                        paths_to_scan: Default::default(),
                        removed_entries: Default::default(),
                        changed_paths: Default::default(),
                    }),
                    phase: BackgroundScannerPhase::InitialScan,
                    share_private_files,
                    settings,
                    watcher,
                    track_git_repositories,
                    is_single_file,
                    defer_watch,
                };

                scanner.run(events).await;
            }
        });
        let scan_state_updater = cx.spawn(async move |this, cx| {
            while let Some((state, this)) = scan_states_rx.next().await.zip(this.upgrade()) {
                this.update(cx, |this, cx| {
                    let this = this.as_local_mut().unwrap();
                    match state {
                        ScanState::Started => {
                            *this.is_scanning.0.borrow_mut() = true;
                        }
                        ScanState::Updated {
                            snapshot,
                            changes,
                            barrier,
                            scanning,
                        } => {
                            *this.is_scanning.0.borrow_mut() = scanning;
                            this.set_snapshot(snapshot, changes, cx);
                            drop(barrier);
                        }
                        ScanState::RootUpdated { new_path } => {
                            this.update_abs_path_and_refresh(new_path, cx);
                        }
                        ScanState::RootDeleted => {
                            log::info!(
                                "worktree root {} no longer exists, closing worktree",
                                this.abs_path().display()
                            );
                            cx.emit(Event::Deleted);
                        }
                    }
                });
            }
        });
        self._background_scanner_tasks = vec![background_scanner, scan_state_updater];
        *self.is_scanning.0.borrow_mut() = true;
    }

    fn set_snapshot(
        &mut self,
        mut new_snapshot: LocalSnapshot,
        entry_changes: UpdatedEntriesSet,
        cx: &mut Context<Worktree>,
    ) {
        let repo_changes = self.changed_repos(&self.snapshot, &mut new_snapshot);

        new_snapshot.root_repo_common_dir = new_snapshot
            .local_repo_for_work_directory_path(RelPath::empty())
            .map(|repo| SanitizedPath::from_arc(repo.common_dir_abs_path.clone()));

        let old_root_repo_common_dir = (self.snapshot.root_repo_common_dir
            != new_snapshot.root_repo_common_dir)
            .then(|| self.snapshot.root_repo_common_dir.clone());
        self.snapshot = new_snapshot;

        if let Some(share) = self.update_observer.as_mut() {
            share
                .snapshots_tx
                .unbounded_send((self.snapshot.clone(), entry_changes.clone()))
                .ok();
        }

        if !entry_changes.is_empty() {
            cx.emit(Event::UpdatedEntries(entry_changes));
        }
        if !repo_changes.is_empty() {
            cx.emit(Event::UpdatedGitRepositories(repo_changes));
        }
        if let Some(old) = old_root_repo_common_dir {
            cx.emit(Event::UpdatedRootRepoCommonDir { old });
        }

        while let Some((scan_id, _)) = self.snapshot_subscriptions.front() {
            if self.snapshot.completed_scan_id >= *scan_id {
                let (_, tx) = self.snapshot_subscriptions.pop_front().unwrap();
                tx.send(()).ok();
            } else {
                break;
            }
        }
    }

    fn changed_repos(
        &self,
        old_snapshot: &LocalSnapshot,
        new_snapshot: &mut LocalSnapshot,
    ) -> UpdatedGitRepositoriesSet {
        let mut changes = Vec::new();
        let mut old_repos = old_snapshot.git_repositories.iter().peekable();
        let new_repos = new_snapshot.git_repositories.clone();
        let mut new_repos = new_repos.iter().peekable();

        loop {
            match (new_repos.peek().map(clone), old_repos.peek().map(clone)) {
                (Some((new_entry_id, new_repo)), Some((old_entry_id, old_repo))) => {
                    match Ord::cmp(&new_entry_id, &old_entry_id) {
                        Ordering::Less => {
                            changes.push(UpdatedGitRepository {
                                work_directory_id: new_entry_id,
                                old_work_directory_abs_path: None,
                                new_work_directory_abs_path: Some(
                                    new_repo.work_directory_abs_path.clone(),
                                ),
                                dot_git_abs_path: Some(new_repo.dot_git_abs_path.clone()),
                                repository_dir_abs_path: Some(
                                    new_repo.repository_dir_abs_path.clone(),
                                ),
                                common_dir_abs_path: Some(new_repo.common_dir_abs_path.clone()),
                            });
                            new_repos.next();
                        }
                        Ordering::Equal => {
                            if new_repo.git_dir_scan_id != old_repo.git_dir_scan_id
                                || new_repo.work_directory_abs_path
                                    != old_repo.work_directory_abs_path
                            {
                                changes.push(UpdatedGitRepository {
                                    work_directory_id: new_entry_id,
                                    old_work_directory_abs_path: Some(
                                        old_repo.work_directory_abs_path.clone(),
                                    ),
                                    new_work_directory_abs_path: Some(
                                        new_repo.work_directory_abs_path.clone(),
                                    ),
                                    dot_git_abs_path: Some(new_repo.dot_git_abs_path.clone()),
                                    repository_dir_abs_path: Some(
                                        new_repo.repository_dir_abs_path.clone(),
                                    ),
                                    common_dir_abs_path: Some(new_repo.common_dir_abs_path.clone()),
                                });
                            }
                            new_repos.next();
                            old_repos.next();
                        }
                        Ordering::Greater => {
                            changes.push(UpdatedGitRepository {
                                work_directory_id: old_entry_id,
                                old_work_directory_abs_path: Some(
                                    old_repo.work_directory_abs_path.clone(),
                                ),
                                new_work_directory_abs_path: None,
                                dot_git_abs_path: None,
                                repository_dir_abs_path: None,
                                common_dir_abs_path: None,
                            });
                            old_repos.next();
                        }
                    }
                }
                (Some((entry_id, repo)), None) => {
                    changes.push(UpdatedGitRepository {
                        work_directory_id: entry_id,
                        old_work_directory_abs_path: None,
                        new_work_directory_abs_path: Some(repo.work_directory_abs_path.clone()),
                        dot_git_abs_path: Some(repo.dot_git_abs_path.clone()),
                        repository_dir_abs_path: Some(repo.repository_dir_abs_path.clone()),
                        common_dir_abs_path: Some(repo.common_dir_abs_path.clone()),
                    });
                    new_repos.next();
                }
                (None, Some((entry_id, repo))) => {
                    changes.push(UpdatedGitRepository {
                        work_directory_id: entry_id,
                        old_work_directory_abs_path: Some(repo.work_directory_abs_path.clone()),
                        new_work_directory_abs_path: None,
                        dot_git_abs_path: Some(repo.dot_git_abs_path.clone()),
                        repository_dir_abs_path: Some(repo.repository_dir_abs_path.clone()),
                        common_dir_abs_path: Some(repo.common_dir_abs_path.clone()),
                    });
                    old_repos.next();
                }
                (None, None) => break,
            }
        }

        fn clone<T: Clone, U: Clone>(value: &(&T, &U)) -> (T, U) {
            (value.0.clone(), value.1.clone())
        }

        changes.into()
    }

    pub fn scan_complete(&self) -> impl Future<Output = ()> + use<> {
        let mut is_scanning_rx = self.is_scanning.1.clone();
        async move {
            let mut is_scanning = *is_scanning_rx.borrow();
            while is_scanning {
                if let Some(value) = is_scanning_rx.recv().await {
                    is_scanning = value;
                } else {
                    break;
                }
            }
        }
    }

    pub fn wait_for_snapshot(
        &mut self,
        scan_id: usize,
    ) -> impl Future<Output = Result<()>> + use<> {
        let (tx, rx) = oneshot::channel();
        if self.snapshot.completed_scan_id >= scan_id {
            tx.send(()).ok();
        } else {
            match self
                .snapshot_subscriptions
                .binary_search_by_key(&scan_id, |probe| probe.0)
            {
                Ok(ix) | Err(ix) => self.snapshot_subscriptions.insert(ix, (scan_id, tx)),
            }
        }

        async move {
            rx.await?;
            Ok(())
        }
    }

    pub fn snapshot(&self) -> LocalSnapshot {
        self.snapshot.clone()
    }

    pub fn settings(&self) -> WorktreeSettings {
        self.settings.clone()
    }

    fn load_binary_file(
        &self,
        path: &RelPath,
        cx: &Context<Worktree>,
    ) -> Task<Result<LoadedBinaryFile>> {
        let path = Arc::from(path);
        let abs_path = self.absolutize(&path);
        let fs = self.fs.clone();
        let entry = self.refresh_entry(path.clone(), None, cx);
        let is_private = self.is_path_private(&path);

        let worktree = cx.weak_entity();
        cx.background_spawn(async move {
            let content = fs.load_bytes(&abs_path).await?;

            let worktree = worktree.upgrade().context("worktree was dropped")?;
            let file = match entry.await? {
                Some(entry) => File::for_entry(entry, worktree),
                None => {
                    let metadata = fs
                        .metadata(&abs_path)
                        .await
                        .with_context(|| {
                            format!("Loading metadata for excluded file {abs_path:?}")
                        })?
                        .with_context(|| {
                            format!("Excluded file {abs_path:?} got removed during loading")
                        })?;
                    Arc::new(File {
                        entry_id: None,
                        worktree,
                        path,
                        disk_state: DiskState::Present {
                            mtime: metadata.mtime,
                            size: metadata.len,
                        },
                        is_local: true,
                        is_private,
                    })
                }
            };

            Ok(LoadedBinaryFile { file, content })
        })
    }

    #[ztracing::instrument(skip_all)]
    fn load_file(&self, path: &RelPath, cx: &Context<Worktree>) -> Task<Result<LoadedFile>> {
        let path = Arc::from(path);
        let abs_path = self.absolutize(&path);
        let fs = self.fs.clone();
        let entry = self.refresh_entry(path.clone(), None, cx);
        let is_private = self.is_path_private(path.as_ref());

        let this = cx.weak_entity();
        cx.background_spawn(async move {
            // WARN: Temporary workaround for #27283.
            //       We are not efficient with our memory usage per file, and use in excess of 64GB for a 10GB file
            //       Therefore, as a temporary workaround to prevent system freezes, we just bail before opening a file
            //       if it is too large
            //       5GB seems to be more reasonable, peaking at ~16GB, while 6GB jumps up to >24GB which seems like a
            //       reasonable limit
            {
                const FILE_SIZE_MAX: u64 = 6 * 1024 * 1024 * 1024; // 6GB
                if let Ok(Some(metadata)) = fs.metadata(&abs_path).await
                    && metadata.len >= FILE_SIZE_MAX
                {
                    anyhow::bail!("File is too large to load");
                }
            }
            let (text, encoding, has_bom) = decode_file_text(fs.as_ref(), &abs_path).await?;

            let worktree = this.upgrade().context("worktree was dropped")?;
            let file = match entry.await? {
                Some(entry) => File::for_entry(entry, worktree),
                None => {
                    let metadata = fs
                        .metadata(&abs_path)
                        .await
                        .with_context(|| {
                            format!("Loading metadata for excluded file {abs_path:?}")
                        })?
                        .with_context(|| {
                            format!("Excluded file {abs_path:?} got removed during loading")
                        })?;
                    Arc::new(File {
                        entry_id: None,
                        worktree,
                        path,
                        disk_state: DiskState::Present {
                            mtime: metadata.mtime,
                            size: metadata.len,
                        },
                        is_local: true,
                        is_private,
                    })
                }
            };

            Ok(LoadedFile {
                file,
                text,
                encoding,
                has_bom,
            })
        })
    }

    /// Find the lowest path in the worktree's datastructures that is an ancestor
    fn lowest_ancestor(&self, path: &RelPath) -> Arc<RelPath> {
        let mut lowest_ancestor = None;
        for path in path.ancestors() {
            if self.entry_for_path(path).is_some() {
                lowest_ancestor = Some(path.into());
                break;
            }
        }

        lowest_ancestor.unwrap_or_else(|| RelPath::empty_arc())
    }

    pub fn create_entry(
        &self,
        path: Arc<RelPath>,
        is_dir: bool,
        content: Option<Vec<u8>>,
        cx: &Context<Worktree>,
    ) -> Task<Result<CreatedEntry>> {
        let abs_path = self.absolutize(&path);
        let path_excluded = self.settings.is_path_excluded(&path);
        let fs = self.fs.clone();
        let task_abs_path = abs_path.clone();
        let write = cx.background_spawn(async move {
            if is_dir {
                fs.create_dir(&task_abs_path)
                    .await
                    .with_context(|| format!("creating directory {task_abs_path:?}"))
            } else {
                fs.write(&task_abs_path, content.as_deref().unwrap_or(&[]))
                    .await
                    .with_context(|| format!("creating file {task_abs_path:?}"))
            }
        });

        let lowest_ancestor = self.lowest_ancestor(&path);
        cx.spawn(async move |this, cx| {
            write.await?;
            if path_excluded {
                return Ok(CreatedEntry::Excluded { abs_path });
            }

            let (result, refreshes) = this.update(cx, |this, cx| {
                let mut refreshes = Vec::new();
                let refresh_paths = path.strip_prefix(&lowest_ancestor).unwrap();
                for refresh_path in refresh_paths.ancestors() {
                    if refresh_path == RelPath::empty() {
                        continue;
                    }
                    let refresh_full_path = lowest_ancestor.join(refresh_path);

                    refreshes.push(this.as_local_mut().unwrap().refresh_entry(
                        refresh_full_path,
                        None,
                        cx,
                    ));
                }
                (
                    this.as_local_mut().unwrap().refresh_entry(path, None, cx),
                    refreshes,
                )
            })?;
            for refresh in refreshes {
                refresh.await.log_err();
            }

            Ok(result
                .await?
                .map(CreatedEntry::Included)
                .unwrap_or_else(|| CreatedEntry::Excluded { abs_path }))
        })
    }

    pub fn write_file(
        &self,
        path: Arc<RelPath>,
        text: Rope,
        line_ending: LineEnding,
        encoding: &'static Encoding,
        has_bom: bool,
        cx: &Context<Worktree>,
    ) -> Task<Result<Arc<File>>> {
        let fs = self.fs.clone();
        let is_private = self.is_path_private(&path);
        let abs_path = self.absolutize(&path);

        let write = cx.background_spawn({
            let fs = fs.clone();
            let abs_path = abs_path.clone();
            async move {
                // For UTF-8, use the optimized `fs.save` which writes Rope chunks directly to disk
                // without allocating a contiguous string.
                if encoding == encoding_rs::UTF_8 && !has_bom {
                    return fs.save(&abs_path, &text, line_ending).await;
                }

                // For legacy encodings (e.g. Shift-JIS), we fall back to converting the entire Rope
                // to a String/Bytes in memory before writing.
                //
                // Note: This is inefficient for very large files compared to the streaming approach above,
                // but supporting streaming writes for arbitrary encodings would require a significant
                // refactor of the `fs` crate to expose a Writer interface.
                let text_string = text.to_string();
                let normalized_text = match line_ending {
                    LineEnding::Unix => text_string,
                    LineEnding::Windows => text_string.replace('\n', "\r\n"),
                };

                // Create the byte vector manually for UTF-16 encodings because encoding_rs encodes to UTF-8 by default (per WHATWG standards),
                //  which is not what we want for saving files.
                let bytes = if encoding == encoding_rs::UTF_16BE {
                    let mut data = Vec::with_capacity(normalized_text.len() * 2 + 2);
                    if has_bom {
                        data.extend_from_slice(&[0xFE, 0xFF]); // BOM
                    }
                    let utf16be_bytes =
                        normalized_text.encode_utf16().flat_map(|u| u.to_be_bytes());
                    data.extend(utf16be_bytes);
                    data.into()
                } else if encoding == encoding_rs::UTF_16LE {
                    let mut data = Vec::with_capacity(normalized_text.len() * 2 + 2);
                    if has_bom {
                        data.extend_from_slice(&[0xFF, 0xFE]); // BOM
                    }
                    let utf16le_bytes =
                        normalized_text.encode_utf16().flat_map(|u| u.to_le_bytes());
                    data.extend(utf16le_bytes);
                    data.into()
                } else {
                    // For other encodings (Shift-JIS, UTF-8 with BOM, etc.), delegate to encoding_rs.
                    let bom_bytes = if has_bom {
                        if encoding == encoding_rs::UTF_8 {
                            vec![0xEF, 0xBB, 0xBF]
                        } else {
                            vec![]
                        }
                    } else {
                        vec![]
                    };
                    let (cow, _, _) = encoding.encode(&normalized_text);
                    if !bom_bytes.is_empty() {
                        let mut bytes = bom_bytes;
                        bytes.extend_from_slice(&cow);
                        bytes.into()
                    } else {
                        cow
                    }
                };

                fs.write(&abs_path, &bytes).await
            }
        });

        cx.spawn(async move |this, cx| {
            write.await?;
            let entry = this
                .update(cx, |this, cx| {
                    this.as_local_mut()
                        .unwrap()
                        .refresh_entry(path.clone(), None, cx)
                })?
                .await?;
            let worktree = this.upgrade().context("worktree dropped")?;
            if let Some(entry) = entry {
                Ok(File::for_entry(entry, worktree))
            } else {
                let metadata = fs
                    .metadata(&abs_path)
                    .await
                    .with_context(|| {
                        format!("Fetching metadata after saving the excluded buffer {abs_path:?}")
                    })?
                    .with_context(|| {
                        format!("Excluded buffer {path:?} got removed during saving")
                    })?;
                Ok(Arc::new(File {
                    worktree,
                    path,
                    disk_state: DiskState::Present {
                        mtime: metadata.mtime,
                        size: metadata.len,
                    },
                    entry_id: None,
                    is_local: true,
                    is_private,
                }))
            }
        })
    }

    pub fn delete_entry(
        &self,
        entry_id: ProjectEntryId,
        trash: bool,
        cx: &Context<Worktree>,
    ) -> Option<Task<Result<Option<TrashedEntry>>>> {
        let entry = self.entry_for_id(entry_id)?.clone();
        let abs_path = self.absolutize(&entry.path);
        let fs = self.fs.clone();

        let delete = cx.background_spawn(async move {
            let trashed_entry = match (entry.is_file(), trash) {
                (true, true) => Some(fs.trash(&abs_path, Default::default()).await?),
                (false, true) => Some(
                    fs.trash(
                        &abs_path,
                        RemoveOptions {
                            recursive: true,
                            ignore_if_not_exists: false,
                        },
                    )
                    .await?,
                ),
                (true, false) => {
                    fs.remove_file(&abs_path, Default::default()).await?;
                    None
                }
                (false, false) => {
                    fs.remove_dir(
                        &abs_path,
                        RemoveOptions {
                            recursive: true,
                            ignore_if_not_exists: false,
                        },
                    )
                    .await?;
                    None
                }
            };

            anyhow::Ok((trashed_entry, entry.path))
        });

        Some(cx.spawn(async move |this, cx| {
            let (trashed_entry, path) = delete.await?;
            this.update(cx, |this, _| {
                this.as_local_mut()
                    .unwrap()
                    .refresh_entries_for_paths(vec![path])
            })?
            .recv()
            .await;

            Ok(trashed_entry)
        }))
    }

    pub async fn restore_entry(
        trash_entry: TrashedEntry,
        this: Entity<Worktree>,
        cx: &mut AsyncApp,
    ) -> Result<RelPathBuf> {
        let Some((fs, worktree_abs_path, path_style)) = this.read_with(cx, |this, _cx| {
            let local_worktree = match this {
                Worktree::Local(local_worktree) => local_worktree,
                Worktree::Remote(_) => return None,
            };

            let fs = local_worktree.fs.clone();
            let path_style = local_worktree.path_style();
            Some((fs, Arc::clone(local_worktree.abs_path()), path_style))
        }) else {
            return Err(anyhow!("Localworktree should not change into a remote one"));
        };

        let path_buf = fs.restore(trash_entry).await?;
        let path = path_buf
            .strip_prefix(worktree_abs_path)
            .context("Could not strip prefix")?;
        let path = RelPath::new(&path, path_style)?;
        let path = path.into_owned();

        Ok(path)
    }

    pub fn copy_external_entries(
        &self,
        target_directory: Arc<RelPath>,
        paths: Vec<Arc<Path>>,
        cx: &Context<Worktree>,
    ) -> Task<Result<Vec<ProjectEntryId>>> {
        let target_directory = self.absolutize(&target_directory);
        let worktree_path = self.abs_path().clone();
        let fs = self.fs.clone();
        let paths = paths
            .into_iter()
            .filter_map(|source| {
                let file_name = source.file_name()?;
                let mut target = target_directory.clone();
                target.push(file_name);

                // Do not allow copying the same file to itself.
                if source.as_ref() != target.as_path() {
                    Some((source, target))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        let paths_to_refresh = paths
            .iter()
            .filter_map(|(_, target)| {
                RelPath::new(
                    target.strip_prefix(&worktree_path).ok()?,
                    PathStyle::local(),
                )
                .ok()
                .map(|path| path.into_arc())
            })
            .collect::<Vec<_>>();

        cx.spawn(async move |this, cx| {
            cx.background_spawn(async move {
                for (source, target) in paths {
                    copy_recursive(
                        fs.as_ref(),
                        &source,
                        &target,
                        fs::CopyOptions {
                            overwrite: true,
                            ..Default::default()
                        },
                    )
                    .await
                    .with_context(|| {
                        format!("Failed to copy file from {source:?} to {target:?}")
                    })?;
                }
                anyhow::Ok(())
            })
            .await
            .log_err();
            let mut refresh = cx.read_entity(
                &this.upgrade().with_context(|| "Dropped worktree")?,
                |this, _| {
                    anyhow::Ok::<postage::barrier::Receiver>(
                        this.as_local()
                            .with_context(|| "Worktree is not local")?
                            .refresh_entries_for_paths(paths_to_refresh.clone()),
                    )
                },
            )?;

            cx.background_spawn(async move {
                refresh.next().await;
                anyhow::Ok(())
            })
            .await
            .log_err();

            let this = this.upgrade().with_context(|| "Dropped worktree")?;
            Ok(cx.read_entity(&this, |this, _| {
                paths_to_refresh
                    .iter()
                    .filter_map(|path| Some(this.entry_for_path(path)?.id))
                    .collect()
            }))
        })
    }

    fn expand_entry(
        &self,
        entry_id: ProjectEntryId,
        cx: &Context<Worktree>,
    ) -> Option<Task<Result<()>>> {
        let path = self.entry_for_id(entry_id)?.path.clone();
        let mut refresh = self.refresh_entries_for_paths(vec![path]);
        Some(cx.background_spawn(async move {
            refresh.next().await;
            Ok(())
        }))
    }

    fn expand_all_for_entry(
        &self,
        entry_id: ProjectEntryId,
        cx: &Context<Worktree>,
    ) -> Option<Task<Result<()>>> {
        let path = self.entry_for_id(entry_id).unwrap().path.clone();
        let mut rx = self.add_path_prefix_to_scan(path);
        Some(cx.background_spawn(async move {
            rx.next().await;
            Ok(())
        }))
    }

    pub fn refresh_entries_for_paths(&self, paths: Vec<Arc<RelPath>>) -> barrier::Receiver {
        let (tx, rx) = barrier::channel();
        self.scan_requests_tx
            .try_send(ScanRequest {
                relative_paths: paths,
                done: smallvec![tx],
            })
            .ok();
        rx
    }

    #[cfg(feature = "test-support")]
    pub fn manually_refresh_entries_for_paths(
        &self,
        paths: Vec<Arc<RelPath>>,
    ) -> barrier::Receiver {
        self.refresh_entries_for_paths(paths)
    }

    pub fn add_path_prefix_to_scan(&self, path_prefix: Arc<RelPath>) -> barrier::Receiver {
        let (tx, rx) = barrier::channel();
        self.path_prefixes_to_scan_tx
            .try_send(PathPrefixScanRequest {
                path: path_prefix,
                done: smallvec![tx],
            })
            .ok();
        rx
    }

    pub fn refresh_entry(
        &self,
        path: Arc<RelPath>,
        old_path: Option<Arc<RelPath>>,
        cx: &Context<Worktree>,
    ) -> Task<Result<Option<Entry>>> {
        if self.settings.is_path_excluded(&path) {
            return Task::ready(Ok(None));
        }
        let paths = if let Some(old_path) = old_path.as_ref() {
            vec![old_path.clone(), path.clone()]
        } else {
            vec![path.clone()]
        };
        let t0 = Instant::now();
        let mut refresh = self.refresh_entries_for_paths(paths);
        // todo(lw): Hot foreground spawn
        cx.spawn(async move |this, cx| {
            refresh.recv().await;
            log::trace!("refreshed entry {path:?} in {:?}", t0.elapsed());
            let new_entry = this.read_with(cx, |this, _| {
                this.entry_for_path(&path).cloned().with_context(|| {
                    format!("Could not find entry in worktree for {path:?} after refresh")
                })
            })??;
            Ok(Some(new_entry))
        })
    }

    pub fn observe_updates<F, Fut>(&mut self, project_id: u64, cx: &Context<Worktree>, callback: F)
    where
        F: 'static + Send + Fn(proto::UpdateWorktree) -> Fut,
        Fut: 'static + Send + Future<Output = bool>,
    {
        if let Some(observer) = self.update_observer.as_mut() {
            *observer.resume_updates.borrow_mut() = ();
            return;
        }

        let (resume_updates_tx, mut resume_updates_rx) = watch::channel::<()>();
        let (snapshots_tx, mut snapshots_rx) =
            mpsc::unbounded::<(LocalSnapshot, UpdatedEntriesSet)>();
        snapshots_tx
            .unbounded_send((self.snapshot(), Arc::default()))
            .ok();

        let worktree_id = self.id.to_proto();
        let _maintain_remote_snapshot = cx.background_spawn(async move {
            let mut is_first = true;
            while let Some((snapshot, entry_changes)) = snapshots_rx.next().await {
                let update = if is_first {
                    is_first = false;
                    snapshot.build_initial_update(project_id, worktree_id)
                } else {
                    snapshot.build_update(project_id, worktree_id, entry_changes)
                };

                for update in proto::split_worktree_update(update) {
                    let _ = resume_updates_rx.try_recv();
                    loop {
                        let result = callback(update.clone());
                        if result.await {
                            break;
                        } else {
                            log::info!("waiting to resume updates");
                            if resume_updates_rx.next().await.is_none() {
                                return Some(());
                            }
                        }
                    }
                }
            }
            Some(())
        });

        self.update_observer = Some(UpdateObservationState {
            snapshots_tx,
            resume_updates: resume_updates_tx,
            _maintain_remote_snapshot,
        });
    }

    pub fn share_private_files(&mut self, cx: &Context<Worktree>) {
        self.share_private_files = true;
        self.restart_background_scanners(cx);
    }

    pub fn update_abs_path_and_refresh(
        &mut self,
        new_path: Arc<SanitizedPath>,
        cx: &Context<Worktree>,
    ) {
        self.snapshot.git_repositories = Default::default();
        self.snapshot.ignores_by_parent_abs_path = Default::default();
        let root_name = new_path
            .as_path()
            .file_name()
            .and_then(|f| f.to_str())
            .map_or(RelPath::empty_arc(), |f| RelPath::unix(f).unwrap().into());
        self.snapshot.update_abs_path(new_path, root_name);
        self.restart_background_scanners(cx);
    }
    #[cfg(feature = "test-support")]
    pub fn set_defer_watch(&mut self, defer: bool, cx: &mut Context<Worktree>) {
        self.force_defer_watch = defer;
        self.restart_background_scanners(cx);
    }

    #[cfg(feature = "test-support")]
    pub fn repositories(&self) -> Vec<Arc<Path>> {
        self.git_repositories
            .values()
            .map(|entry| entry.work_directory_abs_path.clone())
            .collect::<Vec<_>>()
    }
}

impl RemoteWorktree {
    pub fn project_id(&self) -> u64 {
        self.project_id
    }

    pub fn client(&self) -> AnyProtoClient {
        self.client.clone()
    }

    pub fn disconnected_from_host(&mut self) {
        self.updates_tx.take();
        self.snapshot_subscriptions.clear();
        self.disconnected = true;
    }

    pub fn update_from_remote(&self, update: proto::UpdateWorktree) {
        if let Some(updates_tx) = &self.updates_tx {
            updates_tx
                .unbounded_send(update)
                .expect("consumer runs to completion");
        }
    }

    fn observe_updates<F, Fut>(&mut self, project_id: u64, cx: &Context<Worktree>, callback: F)
    where
        F: 'static + Send + Fn(proto::UpdateWorktree) -> Fut,
        Fut: 'static + Send + Future<Output = bool>,
    {
        let (tx, mut rx) = mpsc::unbounded();
        let initial_update = self
            .snapshot
            .build_initial_update(project_id, self.id().to_proto());
        self.update_observer = Some(tx);
        cx.spawn(async move |this, cx| {
            let mut update = initial_update;
            'outer: loop {
                // SSH projects use a special project ID of 0, and we need to
                // remap it to the correct one here.
                update.project_id = project_id;

                for chunk in split_worktree_update(update) {
                    if !callback(chunk).await {
                        break 'outer;
                    }
                }

                if let Some(next_update) = rx.next().await {
                    update = next_update;
                } else {
                    break;
                }
            }
            this.update(cx, |this, _| {
                let this = this.as_remote_mut().unwrap();
                this.update_observer.take();
            })
        })
        .detach();
    }

    fn observed_snapshot(&self, scan_id: usize) -> bool {
        self.completed_scan_id >= scan_id
    }

    pub fn wait_for_snapshot(
        &mut self,
        scan_id: usize,
    ) -> impl Future<Output = Result<()>> + use<> {
        let (tx, rx) = oneshot::channel();
        if self.observed_snapshot(scan_id) {
            let _ = tx.send(());
        } else if self.disconnected {
            drop(tx);
        } else {
            match self
                .snapshot_subscriptions
                .binary_search_by_key(&scan_id, |probe| probe.0)
            {
                Ok(ix) | Err(ix) => self.snapshot_subscriptions.insert(ix, (scan_id, tx)),
            }
        }

        async move {
            rx.await?;
            Ok(())
        }
    }

    pub fn insert_entry(
        &mut self,
        entry: proto::Entry,
        scan_id: usize,
        cx: &Context<Worktree>,
    ) -> Task<Result<Entry>> {
        let wait_for_snapshot = self.wait_for_snapshot(scan_id);
        cx.spawn(async move |this, cx| {
            wait_for_snapshot.await?;
            this.update(cx, |worktree, _| {
                let worktree = worktree.as_remote_mut().unwrap();
                let snapshot = &mut worktree.background_snapshot.lock().0;
                let entry = snapshot.insert_entry(entry, &worktree.file_scan_inclusions);
                worktree.snapshot = snapshot.clone();
                entry
            })?
        })
    }

    fn delete_entry(
        &self,
        entry_id: ProjectEntryId,
        trash: bool,
        cx: &Context<Worktree>,
    ) -> Option<Task<Result<Option<TrashedEntry>>>> {
        let response = self.client.request(proto::DeleteProjectEntry {
            project_id: self.project_id,
            entry_id: entry_id.to_proto(),
            use_trash: trash,
        });
        Some(cx.spawn(async move |this, cx| {
            let response = response.await?;
            let scan_id = response.worktree_scan_id as usize;

            this.update(cx, move |this, _| {
                this.as_remote_mut().unwrap().wait_for_snapshot(scan_id)
            })?
            .await?;

            this.update(cx, |this, _| {
                let this = this.as_remote_mut().unwrap();
                let snapshot = &mut this.background_snapshot.lock().0;
                snapshot.delete_entry(entry_id);
                this.snapshot = snapshot.clone();

                // TODO: How can we actually track the deleted entry when
                // working in remote? We likely only need to keep this
                // information on the remote side in order to support restoring
                // the trashed file.
                None
            })
        }))
    }

    // fn rename_entry(
    //     &self,
    //     entry_id: ProjectEntryId,
    //     new_path: impl Into<Arc<RelPath>>,
    //     cx: &Context<Worktree>,
    // ) -> Task<Result<CreatedEntry>> {
    //     let new_path: Arc<RelPath> = new_path.into();
    //     let response = self.client.request(proto::RenameProjectEntry {
    //         project_id: self.project_id,
    //         entry_id: entry_id.to_proto(),
    //         new_worktree_id: new_path.worktree_id,
    //         new_path: new_path.as_ref().to_proto(),
    //     });
    //     cx.spawn(async move |this, cx| {
    //         let response = response.await?;
    //         match response.entry {
    //             Some(entry) => this
    //                 .update(cx, |this, cx| {
    //                     this.as_remote_mut().unwrap().insert_entry(
    //                         entry,
    //                         response.worktree_scan_id as usize,
    //                         cx,
    //                     )
    //                 })?
    //                 .await
    //                 .map(CreatedEntry::Included),
    //             None => {
    //                 let abs_path =
    //                     this.read_with(cx, |worktree, _| worktree.absolutize(&new_path))?;
    //                 Ok(CreatedEntry::Excluded { abs_path })
    //             }
    //         }
    //     })
    // }

    fn copy_external_entries(
        &self,
        target_directory: Arc<RelPath>,
        paths_to_copy: Vec<Arc<Path>>,
        local_fs: Arc<dyn Fs>,
        cx: &Context<Worktree>,
    ) -> Task<anyhow::Result<Vec<ProjectEntryId>>> {
        let client = self.client.clone();
        let worktree_id = self.id().to_proto();
        let project_id = self.project_id;

        cx.background_spawn(async move {
            let mut requests = Vec::new();
            for root_path_to_copy in paths_to_copy {
                let Some(filename) = root_path_to_copy
                    .file_name()
                    .and_then(|name| name.to_str())
                    .and_then(|filename| RelPath::unix(filename).ok())
                else {
                    continue;
                };
                for (abs_path, is_directory) in
                    read_dir_items(local_fs.as_ref(), &root_path_to_copy).await?
                {
                    let Some(relative_path) = abs_path
                        .strip_prefix(&root_path_to_copy)
                        .map_err(|e| anyhow::Error::from(e))
                        .and_then(|relative_path| RelPath::new(relative_path, PathStyle::local()))
                        .log_err()
                    else {
                        continue;
                    };
                    let content = if is_directory {
                        None
                    } else {
                        Some(local_fs.load_bytes(&abs_path).await?)
                    };

                    let mut target_path = target_directory.join(filename);
                    if relative_path.file_name().is_some() {
                        target_path = target_path.join(&relative_path);
                    }

                    requests.push(proto::CreateProjectEntry {
                        project_id,
                        worktree_id,
                        path: target_path.to_proto(),
                        is_directory,
                        content,
                    });
                }
            }
            requests.sort_unstable_by(|a, b| a.path.cmp(&b.path));
            requests.dedup();

            let mut copied_entry_ids = Vec::new();
            for request in requests {
                let response = client.request(request).await?;
                copied_entry_ids.extend(response.entry.map(|e| ProjectEntryId::from_proto(e.id)));
            }

            Ok(copied_entry_ids)
        })
    }
}

impl Snapshot {
    pub fn new(
        id: WorktreeId,
        root_name: Arc<RelPath>,
        abs_path: Arc<Path>,
        path_style: PathStyle,
    ) -> Self {
        Snapshot {
            id,
            abs_path: SanitizedPath::from_arc(abs_path),
            path_style,
            root_char_bag: root_name
                .as_unix_str()
                .chars()
                .map(|c| c.to_ascii_lowercase())
                .collect(),
            root_name,
            always_included_entries: Default::default(),
            entries_by_path: Default::default(),
            entries_by_id: Default::default(),
            root_repo_common_dir: None,
            scan_id: 1,
            completed_scan_id: 0,
        }
    }

    pub fn id(&self) -> WorktreeId {
        self.id
    }

    // TODO:
    // Consider the following:
    //
    // ```rust
    // let abs_path: Arc<Path> = snapshot.abs_path(); // e.g. "C:\Users\user\Desktop\project"
    // let some_non_trimmed_path = Path::new("\\\\?\\C:\\Users\\user\\Desktop\\project\\main.rs");
    // // The caller perform some actions here:
    // some_non_trimmed_path.strip_prefix(abs_path);  // This fails
    // some_non_trimmed_path.starts_with(abs_path);   // This fails too
    // ```
    //
    // This is definitely a bug, but it's not clear if we should handle it here or not.
    pub fn abs_path(&self) -> &Arc<Path> {
        SanitizedPath::cast_arc_ref(&self.abs_path)
    }

    pub fn root_repo_common_dir(&self) -> Option<&Arc<Path>> {
        self.root_repo_common_dir
            .as_ref()
            .map(SanitizedPath::cast_arc_ref)
    }

    fn build_initial_update(&self, project_id: u64, worktree_id: u64) -> proto::UpdateWorktree {
        let mut updated_entries = self
            .entries_by_path
            .iter()
            .map(proto::Entry::from)
            .collect::<Vec<_>>();
        updated_entries.sort_unstable_by_key(|e| e.id);

        proto::UpdateWorktree {
            project_id,
            worktree_id,
            abs_path: self.abs_path().to_string_lossy().into_owned(),
            root_name: self.root_name().to_proto(),
            root_repo_common_dir: self
                .root_repo_common_dir()
                .map(|p| p.to_string_lossy().into_owned()),
            updated_entries,
            removed_entries: Vec::new(),
            scan_id: self.scan_id as u64,
            is_last_update: self.completed_scan_id == self.scan_id,
            // Sent in separate messages.
            updated_repositories: Vec::new(),
            removed_repositories: Vec::new(),
        }
    }

    pub fn work_directory_abs_path(&self, work_directory: &WorkDirectory) -> PathBuf {
        match work_directory {
            WorkDirectory::InProject { relative_path } => self.absolutize(relative_path),
            WorkDirectory::AboveProject { absolute_path, .. } => absolute_path.as_ref().to_owned(),
        }
    }

    pub fn absolutize(&self, path: &RelPath) -> PathBuf {
        if path.file_name().is_some() {
            let mut abs_path = self.abs_path.to_string();
            for component in path.components() {
                if !abs_path.ends_with(self.path_style.primary_separator()) {
                    abs_path.push_str(self.path_style.primary_separator());
                }
                abs_path.push_str(component);
            }
            PathBuf::from(abs_path)
        } else {
            self.abs_path.as_path().to_path_buf()
        }
    }

    pub fn contains_entry(&self, entry_id: ProjectEntryId) -> bool {
        self.entries_by_id.get(&entry_id, ()).is_some()
    }

    fn insert_entry(
        &mut self,
        entry: proto::Entry,
        always_included_paths: &PathMatcher,
    ) -> Result<Entry> {
        let entry = Entry::try_from((&self.root_char_bag, always_included_paths, entry))?;
        let old_entry = self.entries_by_id.insert_or_replace(
            PathEntry {
                id: entry.id,
                path: entry.path.clone(),
                is_ignored: entry.is_ignored,
                scan_id: 0,
            },
            (),
        );
        if let Some(old_entry) = old_entry {
            self.entries_by_path.remove(&PathKey(old_entry.path), ());
        }
        self.entries_by_path.insert_or_replace(entry.clone(), ());
        Ok(entry)
    }

    fn delete_entry(&mut self, entry_id: ProjectEntryId) -> Option<Arc<RelPath>> {
        let removed_entry = self.entries_by_id.remove(&entry_id, ())?;
        self.entries_by_path = {
            let mut cursor = self.entries_by_path.cursor::<TraversalProgress>(());
            let mut new_entries_by_path =
                cursor.slice(&TraversalTarget::path(&removed_entry.path), Bias::Left);
            while let Some(entry) = cursor.item() {
                if entry.path.starts_with(&removed_entry.path) {
                    self.entries_by_id.remove(&entry.id, ());
                    cursor.next();
                } else {
                    break;
                }
            }
            new_entries_by_path.append(cursor.suffix(), ());
            new_entries_by_path
        };

        Some(removed_entry.path)
    }

    fn update_abs_path(&mut self, abs_path: Arc<SanitizedPath>, root_name: Arc<RelPath>) {
        self.abs_path = abs_path;
        if root_name != self.root_name {
            self.root_char_bag = root_name
                .as_unix_str()
                .chars()
                .map(|c| c.to_ascii_lowercase())
                .collect();
            self.root_name = root_name;
        }
    }

    pub fn apply_remote_update(
        &mut self,
        update: proto::UpdateWorktree,
        always_included_paths: &PathMatcher,
    ) {
        log::debug!(
            "applying remote worktree update. {} entries updated, {} removed",
            update.updated_entries.len(),
            update.removed_entries.len()
        );
        if let Some(root_name) = RelPath::from_proto(&update.root_name).log_err() {
            self.update_abs_path(
                SanitizedPath::new_arc(&Path::new(&update.abs_path)),
                root_name,
            );
        }

        let mut entries_by_path_edits = Vec::new();
        let mut entries_by_id_edits = Vec::new();

        for entry_id in update.removed_entries {
            let entry_id = ProjectEntryId::from_proto(entry_id);
            entries_by_id_edits.push(Edit::Remove(entry_id));
            if let Some(entry) = self.entry_for_id(entry_id) {
                entries_by_path_edits.push(Edit::Remove(PathKey(entry.path.clone())));
            }
        }

        for entry in update.updated_entries {
            let Some(entry) =
                Entry::try_from((&self.root_char_bag, always_included_paths, entry)).log_err()
            else {
                continue;
            };
            if let Some(PathEntry { path, .. }) = self.entries_by_id.get(&entry.id, ()) {
                entries_by_path_edits.push(Edit::Remove(PathKey(path.clone())));
            }
            if let Some(old_entry) = self.entries_by_path.get(&PathKey(entry.path.clone()), ())
                && old_entry.id != entry.id
            {
                entries_by_id_edits.push(Edit::Remove(old_entry.id));
            }
            entries_by_id_edits.push(Edit::Insert(PathEntry {
                id: entry.id,
                path: entry.path.clone(),
                is_ignored: entry.is_ignored,
                scan_id: 0,
            }));
            entries_by_path_edits.push(Edit::Insert(entry));
        }

        self.entries_by_path.edit(entries_by_path_edits, ());
        self.entries_by_id.edit(entries_by_id_edits, ());

        if let Some(dir) = update
            .root_repo_common_dir
            .map(|p| SanitizedPath::new_arc(Path::new(&p)))
        {
            self.root_repo_common_dir = Some(dir);
        }

        self.scan_id = update.scan_id as usize;
        if update.is_last_update {
            self.completed_scan_id = update.scan_id as usize;
        }
    }

    pub fn entry_count(&self) -> usize {
        self.entries_by_path.summary().count
    }

    pub fn visible_entry_count(&self) -> usize {
        self.entries_by_path.summary().non_ignored_count
    }

    pub fn dir_count(&self) -> usize {
        let summary = self.entries_by_path.summary();
        summary.count - summary.file_count
    }

    pub fn visible_dir_count(&self) -> usize {
        let summary = self.entries_by_path.summary();
        summary.non_ignored_count - summary.non_ignored_file_count
    }

    pub fn file_count(&self) -> usize {
        self.entries_by_path.summary().file_count
    }

    pub fn visible_file_count(&self) -> usize {
        self.entries_by_path.summary().non_ignored_file_count
    }

    fn traverse_from_offset(
        &self,
        include_files: bool,
        include_dirs: bool,
        include_ignored: bool,
        start_offset: usize,
    ) -> Traversal<'_> {
        let mut cursor = self.entries_by_path.cursor(());
        cursor.seek(
            &TraversalTarget::Count {
                count: start_offset,
                include_files,
                include_dirs,
                include_ignored,
            },
            Bias::Right,
        );
        Traversal {
            snapshot: self,
            cursor,
            include_files,
            include_dirs,
            include_ignored,
        }
    }

    pub fn traverse_from_path(
        &self,
        include_files: bool,
        include_dirs: bool,
        include_ignored: bool,
        path: &RelPath,
    ) -> Traversal<'_> {
        Traversal::new(self, include_files, include_dirs, include_ignored, path)
    }

    pub fn files(&self, include_ignored: bool, start: usize) -> Traversal<'_> {
        self.traverse_from_offset(true, false, include_ignored, start)
    }

    pub fn directories(&self, include_ignored: bool, start: usize) -> Traversal<'_> {
        self.traverse_from_offset(false, true, include_ignored, start)
    }

    pub fn entries(&self, include_ignored: bool, start: usize) -> Traversal<'_> {
        self.traverse_from_offset(true, true, include_ignored, start)
    }

    pub fn paths(&self) -> impl Iterator<Item = &RelPath> {
        self.entries_by_path
            .cursor::<()>(())
            .filter(move |entry| !entry.path.is_empty())
            .map(|entry| entry.path.as_ref())
    }

    pub fn child_entries<'a>(&'a self, parent_path: &'a RelPath) -> ChildEntriesIter<'a> {
        let options = ChildEntriesOptions {
            include_files: true,
            include_dirs: true,
            include_ignored: true,
        };
        self.child_entries_with_options(parent_path, options)
    }

    pub fn child_entries_with_options<'a>(
        &'a self,
        parent_path: &'a RelPath,
        options: ChildEntriesOptions,
    ) -> ChildEntriesIter<'a> {
        let mut cursor = self.entries_by_path.cursor(());
        cursor.seek(&TraversalTarget::path(parent_path), Bias::Right);
        let traversal = Traversal {
            snapshot: self,
            cursor,
            include_files: options.include_files,
            include_dirs: options.include_dirs,
            include_ignored: options.include_ignored,
        };
        ChildEntriesIter {
            traversal,
            parent_path,
        }
    }

    pub fn root_entry(&self) -> Option<&Entry> {
        self.entries_by_path.first()
    }

    /// Returns `None` for a single file worktree, or `Some(self.abs_path())` if
    /// it is a directory.
    pub fn root_dir(&self) -> Option<Arc<Path>> {
        self.root_entry()
            .filter(|entry| entry.is_dir())
            .map(|_| self.abs_path().clone())
    }

    pub fn root_name(&self) -> &RelPath {
        &self.root_name
    }

    pub fn root_name_str(&self) -> &str {
        self.root_name.as_unix_str()
    }

    pub fn scan_id(&self) -> usize {
        self.scan_id
    }

    pub fn entry_for_path(&self, path: &RelPath) -> Option<&Entry> {
        let entry = self.traverse_from_path(true, true, true, path).entry();
        entry.and_then(|entry| {
            if entry.path.as_ref() == path {
                Some(entry)
            } else {
                None
            }
        })
    }

    /// Resolves a path to an executable using the following heuristics:
    ///
    /// 1. If the path starts with `~`, it is expanded to the user's home directory.
    /// 2. If the path is relative and contains more than one component,
    ///    it is joined to the worktree root path.
    /// 3. If the path is relative and exists in the worktree
    ///    (even if falls under an exclusion filter),
    ///    it is joined to the worktree root path.
    /// 4. Otherwise the path is returned unmodified.
    ///
    /// Relative paths that do not exist in the worktree may
    /// still be found using the `PATH` environment variable.
    pub fn resolve_relative_path(&self, path: PathBuf) -> PathBuf {
        if let Some(path_str) = path.to_str() {
            if let Some(remaining_path) = path_str.strip_prefix("~/") {
                return home_dir().join(remaining_path);
            } else if path_str == "~" {
                return home_dir().to_path_buf();
            }
        }

        if let Ok(rel_path) = RelPath::new(&path, self.path_style)
            && (path.components().count() > 1 || self.entry_for_path(&rel_path).is_some())
        {
            self.abs_path().join(path)
        } else {
            path
        }
    }

    pub fn entry_for_id(&self, id: ProjectEntryId) -> Option<&Entry> {
        let entry = self.entries_by_id.get(&id, ())?;
        self.entry_for_path(&entry.path)
    }

    pub fn path_style(&self) -> PathStyle {
        self.path_style
    }
}

impl LocalSnapshot {
    fn local_repo_for_work_directory_path(&self, path: &RelPath) -> Option<&LocalRepositoryEntry> {
        self.git_repositories
            .iter()
            .map(|(_, entry)| entry)
            .find(|entry| entry.work_directory.path_key() == PathKey(path.into()))
    }

    fn build_update(
        &self,
        project_id: u64,
        worktree_id: u64,
        entry_changes: UpdatedEntriesSet,
    ) -> proto::UpdateWorktree {
        let mut updated_entries = Vec::new();
        let mut removed_entries = Vec::new();

        for (_, entry_id, path_change) in entry_changes.iter() {
            if let PathChange::Removed = path_change {
                removed_entries.push(entry_id.0 as u64);
            } else if let Some(entry) = self.entry_for_id(*entry_id) {
                updated_entries.push(proto::Entry::from(entry));
            }
        }

        removed_entries.sort_unstable();
        updated_entries.sort_unstable_by_key(|e| e.id);

        // TODO - optimize, knowing that removed_entries are sorted.
        removed_entries.retain(|id| updated_entries.binary_search_by_key(id, |e| e.id).is_err());

        proto::UpdateWorktree {
            project_id,
            worktree_id,
            abs_path: self.abs_path().to_string_lossy().into_owned(),
            root_name: self.root_name().to_proto(),
            root_repo_common_dir: self
                .root_repo_common_dir()
                .map(|p| p.to_string_lossy().into_owned()),
            updated_entries,
            removed_entries,
            scan_id: self.scan_id as u64,
            is_last_update: self.completed_scan_id == self.scan_id,
            // Sent in separate messages.
            updated_repositories: Vec::new(),
            removed_repositories: Vec::new(),
        }
    }

    async fn insert_entry(&mut self, mut entry: Entry, fs: &dyn Fs) -> Entry {
        log::trace!("insert entry {:?}", entry.path);
        if entry.is_file() && entry.path.file_name() == Some(&GITIGNORE) {
            let abs_path = self.absolutize(&entry.path);
            match build_gitignore(&abs_path, fs).await {
                Ok(ignore) => {
                    self.ignores_by_parent_abs_path
                        .insert(abs_path.parent().unwrap().into(), (Arc::new(ignore), true));
                }
                Err(error) => {
                    log::error!(
                        "error loading .gitignore file {:?} - {:?}",
                        &entry.path,
                        error
                    );
                }
            }
        }

        if entry.kind == EntryKind::PendingDir
            && let Some(existing_entry) = self.entries_by_path.get(&PathKey(entry.path.clone()), ())
        {
            entry.kind = existing_entry.kind;
        }

        let scan_id = self.scan_id;
        let removed = self.entries_by_path.insert_or_replace(entry.clone(), ());
        if let Some(removed) = removed
            && removed.id != entry.id
        {
            self.entries_by_id.remove(&removed.id, ());
        }
        self.entries_by_id.insert_or_replace(
            PathEntry {
                id: entry.id,
                path: entry.path.clone(),
                is_ignored: entry.is_ignored,
                scan_id,
            },
            (),
        );

        entry
    }

    fn ancestor_inodes_for_path(&self, path: &RelPath) -> TreeSet<u64> {
        let mut inodes = TreeSet::default();
        for ancestor in path.ancestors().skip(1) {
            if let Some(entry) = self.entry_for_path(ancestor) {
                inodes.insert(entry.inode);
            }
        }
        inodes
    }

    async fn ignore_stack_for_abs_path(
        &self,
        abs_path: &Path,
        is_dir: bool,
        fs: &dyn Fs,
    ) -> IgnoreStack {
        let mut new_ignores = Vec::new();
        let mut repo_root = None;
        for (index, ancestor) in abs_path.ancestors().enumerate() {
            if index > 0 {
                if let Some((ignore, _)) = self.ignores_by_parent_abs_path.get(ancestor) {
                    new_ignores.push((ancestor, Some(ignore.clone())));
                } else {
                    new_ignores.push((ancestor, None));
                }
            }

            let metadata = fs.metadata(&ancestor.join(DOT_GIT)).await.ok().flatten();
            if metadata.is_some() {
                repo_root = Some(Arc::from(ancestor));
                break;
            }
        }

        let mut ignore_stack = if let Some(global_gitignore) = self.global_gitignore.clone() {
            IgnoreStack::global(global_gitignore)
        } else {
            IgnoreStack::none()
        };

        if let Some((repo_exclude, _)) = repo_root
            .as_ref()
            .and_then(|abs_path| self.repo_exclude_by_work_dir_abs_path.get(abs_path))
        {
            ignore_stack = ignore_stack.append(IgnoreKind::RepoExclude, repo_exclude.clone());
        }
        ignore_stack.repo_root = repo_root;
        for (parent_abs_path, ignore) in new_ignores.into_iter().rev() {
            if ignore_stack.is_abs_path_ignored(parent_abs_path, true) {
                ignore_stack = IgnoreStack::all();
                break;
            } else if let Some(ignore) = ignore {
                ignore_stack =
                    ignore_stack.append(IgnoreKind::Gitignore(parent_abs_path.into()), ignore);
            }
        }

        if ignore_stack.is_abs_path_ignored(abs_path, is_dir) {
            ignore_stack = IgnoreStack::all();
        }

        ignore_stack
    }

    #[cfg(feature = "test-support")]
    pub fn expanded_entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries_by_path
            .cursor::<()>(())
            .filter(|entry| entry.kind == EntryKind::Dir && (entry.is_external || entry.is_ignored))
    }

    #[cfg(feature = "test-support")]
    pub fn check_invariants(&self, git_state: bool) {
        use pretty_assertions::assert_eq;

        assert_eq!(
            self.entries_by_path
                .cursor::<()>(())
                .map(|e| (&e.path, e.id))
                .collect::<Vec<_>>(),
            self.entries_by_id
                .cursor::<()>(())
                .map(|e| (&e.path, e.id))
                .collect::<collections::BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>(),
            "entries_by_path and entries_by_id are inconsistent"
        );

        let mut files = self.files(true, 0);
        let mut visible_files = self.files(false, 0);
        for entry in self.entries_by_path.cursor::<()>(()) {
            if entry.is_file() {
                assert_eq!(files.next().unwrap().inode, entry.inode);
                if !entry.is_ignored || entry.is_always_included {
                    assert_eq!(visible_files.next().unwrap().inode, entry.inode);
                }
            }
        }

        assert!(files.next().is_none());
        assert!(visible_files.next().is_none());

        let mut bfs_paths = Vec::new();
        let mut stack = self
            .root_entry()
            .map(|e| e.path.as_ref())
            .into_iter()
            .collect::<Vec<_>>();
        while let Some(path) = stack.pop() {
            bfs_paths.push(path);
            let ix = stack.len();
            for child_entry in self.child_entries(path) {
                stack.insert(ix, &child_entry.path);
            }
        }

        let dfs_paths_via_iter = self
            .entries_by_path
            .cursor::<()>(())
            .map(|e| e.path.as_ref())
            .collect::<Vec<_>>();
        assert_eq!(bfs_paths, dfs_paths_via_iter);

        let dfs_paths_via_traversal = self
            .entries(true, 0)
            .map(|e| e.path.as_ref())
            .collect::<Vec<_>>();

        assert_eq!(dfs_paths_via_traversal, dfs_paths_via_iter);

        if git_state {
            for ignore_parent_abs_path in self.ignores_by_parent_abs_path.keys() {
                let ignore_parent_path = &RelPath::new(
                    ignore_parent_abs_path
                        .strip_prefix(self.abs_path.as_path())
                        .unwrap(),
                    PathStyle::local(),
                )
                .unwrap();
                assert!(self.entry_for_path(ignore_parent_path).is_some());
                assert!(
                    self.entry_for_path(
                        &ignore_parent_path.join(RelPath::unix(GITIGNORE).unwrap())
                    )
                    .is_some()
                );
            }
        }
    }

    #[cfg(feature = "test-support")]
    pub fn entries_without_ids(&self, include_ignored: bool) -> Vec<(&RelPath, u64, bool)> {
        let mut paths = Vec::new();
        for entry in self.entries_by_path.cursor::<()>(()) {
            if include_ignored || !entry.is_ignored {
                paths.push((entry.path.as_ref(), entry.inode, entry.is_ignored));
            }
        }
        paths.sort_by(|a, b| a.0.cmp(b.0));
        paths
    }
}

impl BackgroundScannerState {
    async fn enqueue_scan_dir(
        &self,
        abs_path: Arc<Path>,
        entry: &Entry,
        scan_job_tx: &Sender<ScanJob>,
        fs: &dyn Fs,
    ) {
        let path = entry.path.clone();
        let ignore_stack = self
            .snapshot
            .ignore_stack_for_abs_path(&abs_path, true, fs)
            .await;
        let mut ancestor_inodes = self.snapshot.ancestor_inodes_for_path(&path);

        if !ancestor_inodes.contains(&entry.inode) {
            ancestor_inodes.insert(entry.inode);
            scan_job_tx
                .try_send(ScanJob {
                    abs_path,
                    path,
                    ignore_stack,
                    scan_queue: scan_job_tx.clone(),
                    ancestor_inodes,
                    is_external: entry.is_external,
                })
                .unwrap();
        }
    }

    fn reuse_entry_id(&mut self, entry: &mut Entry) {
        if let Some(mtime) = entry.mtime {
            // If an entry with the same inode was removed from the worktree during this scan,
            // then it *might* represent the same file or directory. But the OS might also have
            // re-used the inode for a completely different file or directory.
            //
            // Conditionally reuse the old entry's id:
            // * if the mtime is the same, the file was probably been renamed.
            // * if the path is the same, the file may just have been updated
            if let Some(removed_entry) = self.removed_entries.remove(&entry.inode) {
                if removed_entry.mtime == Some(mtime) || removed_entry.path == entry.path {
                    entry.id = removed_entry.id;
                }
            } else if let Some(existing_entry) = self.snapshot.entry_for_path(&entry.path) {
                entry.id = existing_entry.id;
            }
        }
    }

    fn entry_id_for(
        &mut self,
        next_entry_id: &AtomicUsize,
        path: &RelPath,
        metadata: &fs::Metadata,
    ) -> ProjectEntryId {
        // If an entry with the same inode was removed from the worktree during this scan,
        // then it *might* represent the same file or directory. But the OS might also have
        // re-used the inode for a completely different file or directory.
        //
        // Conditionally reuse the old entry's id:
        // * if the mtime is the same, the file was probably been renamed.
        // * if the path is the same, the file may just have been updated
        if let Some(removed_entry) = self.removed_entries.remove(&metadata.inode) {
            if removed_entry.mtime == Some(metadata.mtime) || *removed_entry.path == *path {
                return removed_entry.id;
            }
        } else if let Some(existing_entry) = self.snapshot.entry_for_path(path) {
            return existing_entry.id;
        }
        ProjectEntryId::new(next_entry_id)
    }

    async fn insert_entry(&mut self, entry: Entry, fs: &dyn Fs, watcher: &dyn Watcher) -> Entry {
        let entry = self.snapshot.insert_entry(entry, fs).await;
        if entry.path.file_name() == Some(&DOT_GIT) {
            self.insert_git_repository(entry.path.clone(), fs, watcher)
                .await;
        }

        #[cfg(feature = "test-support")]
        self.snapshot.check_invariants(false);

        entry
    }

    fn populate_dir(
        &mut self,
        parent_path: Arc<RelPath>,
        entries: impl IntoIterator<Item = Entry>,
        ignore: Option<Arc<Gitignore>>,
    ) {
        let mut parent_entry = if let Some(parent_entry) = self
            .snapshot
            .entries_by_path
            .get(&PathKey(parent_path.clone()), ())
        {
            parent_entry.clone()
        } else {
            log::warn!(
                "populating a directory {:?} that has been removed",
                parent_path
            );
            return;
        };

        match parent_entry.kind {
            EntryKind::PendingDir | EntryKind::UnloadedDir => parent_entry.kind = EntryKind::Dir,
            EntryKind::Dir => {}
            _ => return,
        }

        if let Some(ignore) = ignore {
            let abs_parent_path = self
                .snapshot
                .abs_path
                .as_path()
                .join(parent_path.as_std_path())
                .into();
            self.snapshot
                .ignores_by_parent_abs_path
                .insert(abs_parent_path, (ignore, false));
        }

        let parent_entry_id = parent_entry.id;
        self.scanned_dirs.insert(parent_entry_id);
        let mut entries_by_path_edits = vec![Edit::Insert(parent_entry)];
        let mut entries_by_id_edits = Vec::new();

        for entry in entries {
            entries_by_id_edits.push(Edit::Insert(PathEntry {
                id: entry.id,
                path: entry.path.clone(),
                is_ignored: entry.is_ignored,
                scan_id: self.snapshot.scan_id,
            }));
            entries_by_path_edits.push(Edit::Insert(entry));
        }

        self.snapshot
            .entries_by_path
            .edit(entries_by_path_edits, ());
        self.snapshot.entries_by_id.edit(entries_by_id_edits, ());

        if let Err(ix) = self.changed_paths.binary_search(&parent_path) {
            self.changed_paths.insert(ix, parent_path.clone());
        }

        #[cfg(feature = "test-support")]
        self.snapshot.check_invariants(false);
    }

    fn remove_path_from_snapshot_and_unwatch(
        &mut self,
        path: &RelPath,
        watcher: &dyn Watcher,
        preserve_repository_watches: bool,
    ) {
        // When the caller preserves repository watches, it intends to re-scan
        // this subtree and keep its git repositories; pruning them here would
        // transiently drop and then re-create them with fresh `RepositoryId`s.
        let prune_repositories = !preserve_repository_watches;
        let removed_descendant_abs_paths = self.remove_path_from_snapshot(path, prune_repositories);
        self.unwatch_path(
            watcher,
            path,
            removed_descendant_abs_paths,
            preserve_repository_watches,
        );
    }

    fn unwatch_path(
        &mut self,
        watcher: &dyn Watcher,
        path: &RelPath,
        removed_descendant_abs_paths: Vec<PathBuf>,
        preserve_repository_watches: bool,
    ) {
        let mut repository_watches_to_preserve = HashSet::<Arc<Path>>::default();
        if preserve_repository_watches {
            for repository in self.snapshot.git_repositories.values() {
                repository_watches_to_preserve.insert(repository.common_dir_abs_path.clone());
                repository_watches_to_preserve.insert(repository.repository_dir_abs_path.clone());
            }
        }

        for removed_dir_abs_path in removed_descendant_abs_paths {
            if repository_watches_to_preserve.contains(removed_dir_abs_path.as_path()) {
                continue;
            }
            watcher.remove(&removed_dir_abs_path).log_err();
        }

        self.snapshot
            .external_canonical_to_relative
            .retain(|canonical, relative| {
                if relative.starts_with(path) {
                    if !repository_watches_to_preserve.contains(canonical.as_ref()) {
                        watcher.remove(canonical.as_ref()).log_err();
                    }
                    false
                } else {
                    true
                }
            });
    }

    fn remove_path_from_snapshot(
        &mut self,
        path: &RelPath,
        prune_repositories: bool,
    ) -> Vec<PathBuf> {
        log::trace!("background scanner removing path {path:?}");
        let mut new_entries;
        let removed_entries;
        {
            let mut cursor = self
                .snapshot
                .entries_by_path
                .cursor::<TraversalProgress>(());
            new_entries = cursor.slice(&TraversalTarget::path(path), Bias::Left);
            removed_entries = cursor.slice(&TraversalTarget::successor(path), Bias::Left);
            new_entries.append(cursor.suffix(), ());
        }
        self.snapshot.entries_by_path = new_entries;

        let mut removed_ids = Vec::with_capacity(removed_entries.summary().count);
        let mut removed_dir_abs_paths = Vec::new();
        for entry in removed_entries.cursor::<()>(()) {
            if entry.is_dir() {
                let watch_path = self
                    .watched_dir_abs_paths_by_entry_id
                    .remove(&entry.id)
                    .map(|path| path.as_ref().to_path_buf())
                    .unwrap_or_else(|| self.snapshot.absolutize(&entry.path));
                removed_dir_abs_paths.push(watch_path);
            }

            match self.removed_entries.entry(entry.inode) {
                hash_map::Entry::Occupied(mut e) => {
                    let prev_removed_entry = e.get_mut();
                    if entry.id > prev_removed_entry.id {
                        *prev_removed_entry = entry.clone();
                    }
                }
                hash_map::Entry::Vacant(e) => {
                    e.insert(entry.clone());
                }
            }

            if entry.path.file_name() == Some(GITIGNORE) {
                let abs_parent_path = self.snapshot.absolutize(&entry.path.parent().unwrap());
                if let Some((_, needs_update)) = self
                    .snapshot
                    .ignores_by_parent_abs_path
                    .get_mut(abs_parent_path.as_path())
                {
                    *needs_update = true;
                }
            }

            if let Err(ix) = removed_ids.binary_search(&entry.id) {
                removed_ids.insert(ix, entry.id);
            }
        }

        self.snapshot
            .entries_by_id
            .edit(removed_ids.iter().map(|&id| Edit::Remove(id)).collect(), ());

        // Only prune git repositories when the entries are being genuinely
        // removed. During a recursive refresh (e.g. a watcher-forced rescan),
        // the subtree is removed and immediately re-scanned; dropping the
        // repositories here would make them flap, causing the GitStore to
        // tear them down and re-create them with fresh `RepositoryId`s. Stale
        // repositories are instead reaped authoritatively (against the actual
        // filesystem) in `update_git_repositories`.
        if prune_repositories {
            self.snapshot
                .git_repositories
                .retain(|id, _| removed_ids.binary_search(id).is_err());
        }

        #[cfg(feature = "test-support")]
        self.snapshot.check_invariants(false);

        removed_dir_abs_paths
    }

    async fn insert_git_repository(
        &mut self,
        dot_git_path: Arc<RelPath>,
        fs: &dyn Fs,
        watcher: &dyn Watcher,
    ) {
        let work_dir_path: Arc<RelPath> = match dot_git_path.parent() {
            Some(parent_dir) => {
                // Guard against repositories inside the repository metadata
                if parent_dir
                    .components()
                    .any(|component| component == DOT_GIT)
                {
                    log::debug!(
                        "not building git repository for nested `.git` directory, `.git` path in the worktree: {dot_git_path:?}"
                    );
                    return;
                };

                parent_dir.into()
            }
            None => {
                // `dot_git_path.parent().is_none()` means `.git` directory is the opened worktree itself,
                // no files inside that directory are tracked by git, so no need to build the repo around it
                log::debug!(
                    "not building git repository for the worktree itself, `.git` path in the worktree: {dot_git_path:?}"
                );
                return;
            }
        };

        let dot_git_abs_path = Arc::from(self.snapshot.absolutize(&dot_git_path).as_ref());

        self.insert_git_repository_for_path(
            WorkDirectory::InProject {
                relative_path: work_dir_path,
            },
            dot_git_abs_path,
            fs,
            watcher,
        )
        .await
        .log_err();
    }

    async fn insert_git_repository_for_path(
        &mut self,
        work_directory: WorkDirectory,
        dot_git_abs_path: Arc<Path>,
        fs: &dyn Fs,
        watcher: &dyn Watcher,
    ) -> Result<LocalRepositoryEntry> {
        let work_dir_entry = self
            .snapshot
            .entry_for_path(&work_directory.path_key().0)
            .with_context(|| {
                format!(
                    "working directory `{}` not indexed",
                    work_directory
                        .path_key()
                        .0
                        .display(self.snapshot.path_style)
                )
            })?;
        let work_directory_abs_path = self.snapshot.work_directory_abs_path(&work_directory);

        let (repository_dir_abs_path, common_dir_abs_path) =
            discover_git_paths(&dot_git_abs_path, fs).await;
        watcher
            .add(&common_dir_abs_path)
            .context("failed to add common directory to watcher")
            .log_err();
        watcher
            .add(&repository_dir_abs_path)
            .context("failed to add repository directory to watcher")
            .log_err();

        // On Linux and FreeBSD, the native watcher is non-recursive, so subdirectories inside `.git` need explicit watching.
        // For repos using the reftable backend, watch the `.git/reftable` directory so that ref changes are detected.
        let reftable_path = common_dir_abs_path.join("reftable");
        if fs.is_dir(&reftable_path).await {
            watcher
                .add(&reftable_path)
                .context("failed to add reftable directory to watcher")
                .log_err();
        }

        let work_directory_id = work_dir_entry.id;

        let local_repository = LocalRepositoryEntry {
            work_directory_id,
            work_directory,
            work_directory_abs_path: work_directory_abs_path.as_path().into(),
            git_dir_scan_id: 0,
            dot_git_abs_path,
            common_dir_abs_path,
            repository_dir_abs_path,
        };

        self.snapshot
            .git_repositories
            .insert(work_directory_id, local_repository.clone());

        log::trace!("inserting new local git repository");
        Ok(local_repository)
    }
}

async fn is_dot_git(path: &Path, fs: &dyn Fs) -> bool {
    if let Some(file_name) = path.file_name()
        && file_name == DOT_GIT
    {
        return true;
    }

    // If we're in a bare repository, we are not inside a `.git` folder. In a
    // bare repository, the root folder contains what would normally be in the
    // `.git` folder.
    let head_metadata = fs.metadata(&path.join("HEAD")).await;
    if !matches!(head_metadata, Ok(Some(_))) {
        return false;
    }
    let config_metadata = fs.metadata(&path.join("config")).await;
    matches!(config_metadata, Ok(Some(_)))
}

async fn build_gitignore(abs_path: &Path, fs: &dyn Fs) -> Result<Gitignore> {
    let parent = abs_path.parent().unwrap_or_else(|| Path::new("/"));
    build_gitignore_with_root(abs_path, parent, fs).await
}

async fn build_gitignore_with_root(abs_path: &Path, root: &Path, fs: &dyn Fs) -> Result<Gitignore> {
    let contents = fs
        .load(abs_path)
        .await
        .with_context(|| format!("failed to load gitignore file at {}", abs_path.display()))?;
    let mut builder = GitignoreBuilder::new(root);
    for line in contents.lines() {
        builder.add_line(Some(abs_path.into()), line)?;
    }
    Ok(builder.build()?)
}

impl Deref for Worktree {
    type Target = Snapshot;

    fn deref(&self) -> &Self::Target {
        match self {
            Worktree::Local(worktree) => &worktree.snapshot,
            Worktree::Remote(worktree) => &worktree.snapshot,
        }
    }
}

impl Deref for LocalWorktree {
    type Target = LocalSnapshot;

    fn deref(&self) -> &Self::Target {
        &self.snapshot
    }
}

impl Deref for RemoteWorktree {
    type Target = Snapshot;

    fn deref(&self) -> &Self::Target {
        &self.snapshot
    }
}

impl fmt::Debug for LocalWorktree {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.snapshot.fmt(f)
    }
}

impl fmt::Debug for Snapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        struct EntriesById<'a>(&'a SumTree<PathEntry>);
        struct EntriesByPath<'a>(&'a SumTree<Entry>);

        impl fmt::Debug for EntriesByPath<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_map()
                    .entries(self.0.iter().map(|entry| (&entry.path, entry.id)))
                    .finish()
            }
        }

        impl fmt::Debug for EntriesById<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_list().entries(self.0.iter()).finish()
            }
        }

        f.debug_struct("Snapshot")
            .field("id", &self.id)
            .field("root_name", &self.root_name)
            .field("entries_by_path", &EntriesByPath(&self.entries_by_path))
            .field("entries_by_id", &EntriesById(&self.entries_by_id))
            .finish()
    }
}

async fn discover_ancestor_git_repo(
    fs: Arc<dyn Fs>,
    root_abs_path: &SanitizedPath,
) -> (
    HashMap<Arc<Path>, (Arc<Gitignore>, bool)>,
    Option<Arc<Gitignore>>,
    Option<(PathBuf, WorkDirectory)>,
) {
    let mut exclude = None;
    let mut ignores = HashMap::default();
    for (index, ancestor) in root_abs_path.as_path().ancestors().enumerate() {
        if index != 0 {
            if ancestor == paths::home_dir() {
                // Unless $HOME is itself the worktree root, don't consider it as a
                // containing git repository---expensive and likely unwanted.
                break;
            } else if let Ok(ignore) = build_gitignore(&ancestor.join(GITIGNORE), fs.as_ref()).await
            {
                ignores.insert(ancestor.into(), (ignore.into(), false));
            }
        }

        let ancestor_dot_git = ancestor.join(DOT_GIT);
        log::trace!("considering ancestor: {ancestor_dot_git:?}");
        // Check whether the directory or file called `.git` exists (in the
        // case of worktrees it's a file.)
        if fs
            .metadata(&ancestor_dot_git)
            .await
            .is_ok_and(|metadata| metadata.is_some())
        {
            let dot_git_abs_path = if index != 0 {
                // We canonicalize, since the FS events use the canonicalized path.
                match fs.canonicalize(&ancestor_dot_git).await.log_err() {
                    Some(path) => path,
                    None => continue,
                }
            } else {
                ancestor_dot_git.clone()
            };
            let dot_git_abs_path: Arc<Path> = dot_git_abs_path.as_path().into();
            let (_, common_dir_abs_path) = discover_git_paths(&dot_git_abs_path, fs.as_ref()).await;

            let repo_exclude_abs_path = common_dir_abs_path.join(REPO_EXCLUDE);
            if let Ok(repo_exclude) =
                build_gitignore_with_root(&repo_exclude_abs_path, ancestor, fs.as_ref()).await
            {
                exclude = Some(Arc::new(repo_exclude));
            }

            if index != 0 {
                let location_in_repo = root_abs_path
                    .as_path()
                    .strip_prefix(ancestor)
                    .unwrap()
                    .into();
                log::info!("inserting parent git repo for this worktree: {location_in_repo:?}");
                // We associate the external git repo with our root folder and
                // also mark where in the git repo the root folder is located.
                return (
                    ignores,
                    exclude,
                    Some((
                        dot_git_abs_path.as_ref().into(),
                        WorkDirectory::AboveProject {
                            absolute_path: ancestor.into(),
                            location_in_repo,
                        },
                    )),
                );
            }

            break;
        }
    }

    (ignores, exclude, None)
}

mod worktree_diff;
use worktree_diff::{build_diff, char_bag_for_path, merge_event_roots, swap_to_front};

mod worktree_entry;
use worktree_entry::PathEntry;
pub use worktree_entry::{
    Entry, EntryKind, EntrySummary, PathChange, PathKey, PathProgress, PathSummary,
    UpdatedEntriesSet, UpdatedGitRepositoriesSet, UpdatedGitRepository,
};

mod worktree_file_decoding;
use worktree_file_decoding::decode_file_text;

mod worktree_git_discovery;
pub use worktree_git_discovery::discover_root_repo_common_dir;
use worktree_git_discovery::{NullWatcher, discover_git_paths};

mod worktree_model_handle;
pub use worktree_model_handle::WorktreeModelHandle;

mod worktree_scan_jobs;
use worktree_scan_jobs::{ScanJob, UpdateIgnoreStatusJob};

mod worktree_traversal;
pub use worktree_traversal::{ChildEntriesIter, ChildEntriesOptions, PathTarget, Traversal};
use worktree_traversal::{TraversalProgress, TraversalTarget};
impl TryFrom<(&CharBag, &PathMatcher, proto::Entry)> for Entry {
    type Error = anyhow::Error;

    fn try_from(
        (root_char_bag, always_included, entry): (&CharBag, &PathMatcher, proto::Entry),
    ) -> Result<Self> {
        let kind = if entry.is_dir {
            EntryKind::Dir
        } else {
            EntryKind::File
        };

        let path =
            RelPath::from_proto(&entry.path).context("invalid relative path in proto message")?;
        let char_bag = char_bag_for_path(*root_char_bag, &path);
        let is_always_included = always_included.is_match(&path);
        Ok(Entry {
            id: ProjectEntryId::from_proto(entry.id),
            kind,
            path,
            inode: entry.inode,
            mtime: entry.mtime.map(|time| time.into()),
            size: entry.size.unwrap_or(0),
            canonical_path: entry
                .canonical_path
                .map(|path_string| Arc::from(PathBuf::from(path_string))),
            is_ignored: entry.is_ignored,
            is_hidden: entry.is_hidden,
            is_always_included,
            is_external: entry.is_external,
            is_private: false,
            char_bag,
            is_fifo: entry.is_fifo,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// reproduction of issue #50785
    fn build_pcm16_wav_bytes() -> Vec<u8> {
        let header: Vec<u8> = vec![
            /*  RIFF header  */
            0x52, 0x49, 0x46, 0x46, // "RIFF"
            0xc6, 0xcf, 0x00, 0x00, // file size: 8
            0x57, 0x41, 0x56, 0x45, // "WAVE"
            /*  fmt chunk  */
            0x66, 0x6d, 0x74, 0x20, // "fmt "
            0x10, 0x00, 0x00, 0x00, // chunk size: 16
            0x01, 0x00, // format: PCM (1)
            0x01, 0x00, // channels: 1 (mono)
            0x80, 0x3e, 0x00, 0x00, // sample rate: 16000
            0x00, 0x7d, 0x00, 0x00, // byte rate: 32000
            0x02, 0x00, // block align: 2
            0x10, 0x00, // bits per sample: 16
            /*  LIST chunk  */
            0x4c, 0x49, 0x53, 0x54, // "LIST"
            0x1a, 0x00, 0x00, 0x00, // chunk size: 26
            0x49, 0x4e, 0x46, 0x4f, // "INFO"
            0x49, 0x53, 0x46, 0x54, // "ISFT"
            0x0d, 0x00, 0x00, 0x00, // sub-chunk size: 13
            0x4c, 0x61, 0x76, 0x66, 0x36, 0x32, 0x2e, 0x33, // "Lavf62.3"
            0x2e, 0x31, 0x30, 0x30, 0x00, // ".100\0"
            /* padding byte for word alignment */
            0x00, // data chunk header
            0x64, 0x61, 0x74, 0x61, // "data"
            0x80, 0xcf, 0x00, 0x00, // chunk size
        ];

        let mut bytes = header;

        // fill remaining space up to `FILE_ANALYSIS_BYTES` with synthetic PCM
        let audio_bytes_needed = FILE_ANALYSIS_BYTES - bytes.len();
        for i in 0..(audio_bytes_needed / 2) {
            let sample = (i & 0xFF) as u8;
            bytes.push(sample); // low byte: varies
            bytes.push(0x00); // high byte: zero for small values
        }

        bytes
    }

    #[test]
    fn test_pcm16_wav_detected_as_binary() {
        let wav_bytes = build_pcm16_wav_bytes();
        assert_eq!(wav_bytes.len(), FILE_ANALYSIS_BYTES);

        let result = analyze_byte_content(&wav_bytes);
        assert_eq!(
            result,
            ByteContent::Binary,
            "PCM 16-bit WAV should be detected as Binary via RIFF header"
        );
    }

    #[test]
    fn test_le16_binary_not_misdetected_as_utf16le() {
        let mut bytes = b"FAKE".to_vec();
        while bytes.len() < FILE_ANALYSIS_BYTES {
            let sample = (bytes.len() & 0xFF) as u8;
            bytes.push(sample);
            bytes.push(0x00);
        }
        bytes.truncate(FILE_ANALYSIS_BYTES);

        let result = analyze_byte_content(&bytes);
        assert_eq!(
            result,
            ByteContent::Binary,
            "LE 16-bit binary with control characters should be detected as Binary"
        );
    }

    #[test]
    fn test_be16_binary_not_misdetected_as_utf16be() {
        let mut bytes = b"FAKE".to_vec();
        while bytes.len() < FILE_ANALYSIS_BYTES {
            bytes.push(0x00);
            let sample = (bytes.len() & 0xFF) as u8;
            bytes.push(sample);
        }
        bytes.truncate(FILE_ANALYSIS_BYTES);

        let result = analyze_byte_content(&bytes);
        assert_eq!(
            result,
            ByteContent::Binary,
            "BE 16-bit binary with control characters should be detected as Binary"
        );
    }

    #[test]
    fn test_utf16le_text_detected_as_utf16le() {
        let text = "Hello, world! This is a UTF-16 test string. ";
        let mut bytes = Vec::new();
        while bytes.len() < FILE_ANALYSIS_BYTES {
            bytes.extend(text.encode_utf16().flat_map(|u| u.to_le_bytes()));
        }
        bytes.truncate(FILE_ANALYSIS_BYTES);

        assert_eq!(analyze_byte_content(&bytes), ByteContent::Utf16Le);
    }

    #[test]
    fn test_utf16be_text_detected_as_utf16be() {
        let text = "Hello, world! This is a UTF-16 test string. ";
        let mut bytes = Vec::new();
        while bytes.len() < FILE_ANALYSIS_BYTES {
            bytes.extend(text.encode_utf16().flat_map(|u| u.to_be_bytes()));
        }
        bytes.truncate(FILE_ANALYSIS_BYTES);

        assert_eq!(analyze_byte_content(&bytes), ByteContent::Utf16Be);
    }

    #[test]
    fn test_known_binary_headers() {
        let cases: &[(&[u8], &str)] = &[
            (b"RIFF\x00\x00\x00\x00WAVE", "WAV"),
            (b"RIFF\x00\x00\x00\x00AVI ", "AVI"),
            (b"OggS\x00\x02", "OGG"),
            (b"fLaC\x00\x00", "FLAC"),
            (b"ID3\x03\x00", "MP3 ID3v2"),
            (b"\xFF\xFB\x90\x00", "MP3 MPEG1 Layer3"),
            (b"\xFF\xF3\x90\x00", "MP3 MPEG2 Layer3"),
        ];

        for (header, label) in cases {
            let mut bytes = header.to_vec();
            bytes.resize(FILE_ANALYSIS_BYTES, 0x41); // pad with 'A'
            assert_eq!(
                analyze_byte_content(&bytes),
                ByteContent::Binary,
                "{label} should be detected as Binary"
            );
        }
    }
}
