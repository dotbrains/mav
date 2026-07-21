use super::*;
use collections::HashMap;
use fs::Fs;
use futures::StreamExt;
use globset::{Glob, GlobBuilder, GlobMatcher, GlobSet, GlobSetBuilder};
use gpui::{Context, Task};
use lsp::{
    FileOperationFilter, FileOperationPatternKind, FileOperationRegistrationOptions,
    LanguageServerId,
};
use std::{ops::ControlFlow, path::Path, sync::Arc, time::Duration};
use util::{ResultExt, maybe};
use worktree::WorktreeId;

#[derive(Default)]
pub(super) struct RenamePathsWatchedForServer {
    did_rename: Vec<RenameActionPredicate>,
    will_rename: Vec<RenameActionPredicate>,
}

impl RenamePathsWatchedForServer {
    pub(super) fn with_did_rename_patterns(
        mut self,
        did_rename: Option<&FileOperationRegistrationOptions>,
    ) -> Self {
        if let Some(did_rename) = did_rename {
            self.did_rename = did_rename
                .filters
                .iter()
                .filter_map(|filter| filter.try_into().log_err())
                .collect();
        }
        self
    }

    pub(super) fn with_will_rename_patterns(
        mut self,
        will_rename: Option<&FileOperationRegistrationOptions>,
    ) -> Self {
        if let Some(will_rename) = will_rename {
            self.will_rename = will_rename
                .filters
                .iter()
                .filter_map(|filter| filter.try_into().log_err())
                .collect();
        }
        self
    }

    pub(super) fn should_send_did_rename(&self, path: &str, is_dir: bool) -> bool {
        self.did_rename.iter().any(|pred| pred.eval(path, is_dir))
    }

    pub(super) fn should_send_will_rename(&self, path: &str, is_dir: bool) -> bool {
        self.will_rename.iter().any(|pred| pred.eval(path, is_dir))
    }
}

impl TryFrom<&FileOperationFilter> for RenameActionPredicate {
    type Error = globset::Error;

    fn try_from(ops: &FileOperationFilter) -> Result<Self, globset::Error> {
        Ok(Self {
            kind: ops.pattern.matches.clone(),
            glob: GlobBuilder::new(&ops.pattern.glob)
                .case_insensitive(
                    ops.pattern
                        .options
                        .as_ref()
                        .is_some_and(|ops| ops.ignore_case.unwrap_or(false)),
                )
                .build()?
                .compile_matcher(),
        })
    }
}

struct RenameActionPredicate {
    glob: GlobMatcher,
    kind: Option<FileOperationPatternKind>,
}

impl RenameActionPredicate {
    fn eval(&self, path: &str, is_dir: bool) -> bool {
        self.kind.as_ref().is_none_or(|kind| {
            let expected_kind = if is_dir {
                FileOperationPatternKind::Folder
            } else {
                FileOperationPatternKind::File
            };
            kind == &expected_kind
        }) && self.glob.is_match(path)
    }
}

#[derive(Default)]
pub(super) struct LanguageServerWatchedPaths {
    pub(super) worktree_paths: HashMap<WorktreeId, LazyGlobSet>,
    pub(super) abs_paths: HashMap<Arc<Path>, (LazyGlobSet, Task<()>)>,
}

#[derive(Default)]
pub(super) struct LazyGlobSet {
    globs: HashMap<String, Vec<Glob>>,
    compiled: Option<GlobSet>,
}

impl LazyGlobSet {
    pub(super) fn add(&mut self, registration_id: &str, glob: Glob) {
        self.globs
            .entry(registration_id.to_string())
            .or_default()
            .push(glob);
        self.compiled = None;
    }

    pub(super) fn remove(&mut self, registration_id: &str) {
        if self.globs.remove(registration_id).is_some() {
            self.compiled = None;
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.globs.is_empty()
    }

    pub(super) fn is_match<P: AsRef<Path>>(&mut self, path: P) -> bool {
        let compiled = self.compiled.get_or_insert_with(|| {
            let mut builder = GlobSetBuilder::new();
            for glob in self.globs.values().flatten() {
                builder.add(glob.clone());
            }
            builder.build().log_err().unwrap_or_default()
        });
        compiled.is_match(path)
    }
}

impl LanguageServerWatchedPaths {
    pub(super) fn spawn_abs_path_watcher(
        abs_path: Arc<Path>,
        fs: Arc<dyn Fs>,
        language_server_id: LanguageServerId,
        cx: &mut Context<LspStore>,
    ) -> Task<()> {
        let lsp_store = cx.weak_entity();
        const LSP_ABS_PATH_OBSERVE: Duration = Duration::from_millis(100);

        cx.spawn({
            async move |_, cx| {
                maybe!(async move {
                    let mut push_updates = fs.watch(&abs_path, LSP_ABS_PATH_OBSERVE).await;
                    while let Some(update) = push_updates.0.next().await {
                        let action = lsp_store
                            .update(cx, |this, _| {
                                let Some(local) = this.as_local_mut() else {
                                    return ControlFlow::Break(());
                                };
                                let Some(watcher) = local
                                    .language_server_watched_paths
                                    .get_mut(&language_server_id)
                                else {
                                    return ControlFlow::Break(());
                                };
                                let Some((globs, _)) = watcher.abs_paths.get_mut(&abs_path) else {
                                    return ControlFlow::Break(());
                                };
                                let matching_entries = update
                                    .into_iter()
                                    .filter(|event| globs.is_match(&event.path))
                                    .collect::<Vec<_>>();
                                this.lsp_notify_abs_paths_changed(
                                    language_server_id,
                                    matching_entries,
                                );
                                ControlFlow::Continue(())
                            })
                            .ok()?;

                        if action.is_break() {
                            break;
                        }
                    }
                    Some(())
                })
                .await;
            }
        })
    }
}

impl LspStore {
    pub(super) async fn handle_rename_project_entry(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::RenameProjectEntry>,
        mut cx: AsyncApp,
    ) -> Result<proto::ProjectEntryResponse> {
        let entry_id = ProjectEntryId::from_proto(envelope.payload.entry_id);
        let new_worktree_id = WorktreeId::from_proto(envelope.payload.new_worktree_id);
        let new_path =
            RelPath::from_proto(&envelope.payload.new_path).context("invalid relative path")?;

        let (worktree_store, old_worktree, new_worktree, old_entry) = this
            .update(&mut cx, |this, cx| {
                let (worktree, entry) = this
                    .worktree_store
                    .read(cx)
                    .worktree_and_entry_for_id(entry_id, cx)?;
                let new_worktree = this
                    .worktree_store
                    .read(cx)
                    .worktree_for_id(new_worktree_id, cx)?;
                Some((
                    this.worktree_store.clone(),
                    worktree,
                    new_worktree,
                    entry.clone(),
                ))
            })
            .context("worktree not found")?;
        let (old_abs_path, old_worktree_id) = old_worktree.read_with(&cx, |worktree, _| {
            (worktree.absolutize(&old_entry.path), worktree.id())
        });
        let new_abs_path =
            new_worktree.read_with(&cx, |worktree, _| worktree.absolutize(&new_path));

        let _transaction = Self::will_rename_entry(
            this.downgrade(),
            old_worktree_id,
            &old_abs_path,
            &new_abs_path,
            old_entry.is_dir(),
            cx.clone(),
        )
        .await;
        let response = WorktreeStore::handle_rename_project_entry(
            worktree_store,
            envelope.payload,
            cx.clone(),
        )
        .await;
        this.read_with(&cx, |this, _| {
            this.did_rename_entry(
                old_worktree_id,
                &old_abs_path,
                &new_abs_path,
                old_entry.is_dir(),
            );
        });
        response
    }

    pub(crate) fn did_rename_entry(
        &self,
        worktree_id: WorktreeId,
        old_path: &Path,
        new_path: &Path,
        is_dir: bool,
    ) {
        maybe!({
            let local_store = self.as_local()?;

            let old_uri = lsp::Uri::from_file_path(old_path)
                .ok()
                .map(|uri| uri.to_string())?;
            let new_uri = lsp::Uri::from_file_path(new_path)
                .ok()
                .map(|uri| uri.to_string())?;

            for language_server in local_store.language_servers_for_worktree(worktree_id) {
                let Some(filter) = local_store
                    .language_server_paths_watched_for_rename
                    .get(&language_server.server_id())
                else {
                    continue;
                };

                if filter.should_send_did_rename(&old_uri, is_dir) {
                    language_server
                        .notify::<DidRenameFiles>(RenameFilesParams {
                            files: vec![FileRename {
                                old_uri: old_uri.clone(),
                                new_uri: new_uri.clone(),
                            }],
                        })
                        .ok();
                }
            }
            Some(())
        });
    }

    pub(crate) fn will_rename_entry(
        this: WeakEntity<Self>,
        worktree_id: WorktreeId,
        old_path: &Path,
        new_path: &Path,
        is_dir: bool,
        cx: AsyncApp,
    ) -> Task<ProjectTransaction> {
        let old_uri = lsp::Uri::from_file_path(old_path)
            .ok()
            .map(|uri| uri.to_string());
        let new_uri = lsp::Uri::from_file_path(new_path)
            .ok()
            .map(|uri| uri.to_string());
        cx.spawn(async move |cx| {
            let mut tasks = vec![];
            this.update(cx, |this, cx| {
                let local_store = this.as_local()?;
                let old_uri = old_uri?;
                let new_uri = new_uri?;
                for language_server in local_store.language_servers_for_worktree(worktree_id) {
                    let Some(filter) = local_store
                        .language_server_paths_watched_for_rename
                        .get(&language_server.server_id())
                    else {
                        continue;
                    };

                    if !filter.should_send_will_rename(&old_uri, is_dir) {
                        continue;
                    }
                    let request_timeout = ProjectSettings::get_global(cx)
                        .global_lsp_settings
                        .get_request_timeout();

                    let apply_edit = cx.spawn({
                        let old_uri = old_uri.clone();
                        let new_uri = new_uri.clone();
                        let language_server = language_server.clone();
                        async move |this, cx| {
                            let edit = language_server
                                .request::<WillRenameFiles>(
                                    RenameFilesParams {
                                        files: vec![FileRename { old_uri, new_uri }],
                                    },
                                    request_timeout,
                                )
                                .await
                                .into_response()
                                .context("will rename files")
                                .log_err()
                                .flatten()?;

                            LocalLspStore::deserialize_workspace_edit(
                                this.upgrade()?,
                                edit,
                                false,
                                language_server.clone(),
                                cx,
                            )
                            .await
                            .ok()
                        }
                    });
                    tasks.push(apply_edit);
                }
                Some(())
            })
            .ok()
            .flatten();
            let mut merged_transaction = ProjectTransaction::default();
            for task in tasks {
                // Await on tasks sequentially so that the order of application of edits is deterministic
                // (at least with regards to the order of registration of language servers)
                if let Some(transaction) = task.await {
                    for (buffer, buffer_transaction) in transaction.0 {
                        merged_transaction.0.insert(buffer, buffer_transaction);
                    }
                }
            }
            merged_transaction
        })
    }

    pub(super) fn lsp_notify_abs_paths_changed(
        &mut self,
        server_id: LanguageServerId,
        changes: Vec<PathEvent>,
    ) {
        maybe!({
            let server = self.language_server_for_id(server_id)?;
            let changes = changes
                .into_iter()
                .filter_map(|event| {
                    let typ = match event.kind? {
                        PathEventKind::Created => lsp::FileChangeType::CREATED,
                        PathEventKind::Removed => lsp::FileChangeType::DELETED,
                        PathEventKind::Changed | PathEventKind::Rescan => {
                            lsp::FileChangeType::CHANGED
                        }
                    };
                    Some(lsp::FileEvent {
                        uri: file_path_to_lsp_url(&event.path).log_err()?,
                        typ,
                    })
                })
                .collect::<Vec<_>>();
            if !changes.is_empty() {
                server
                    .notify::<lsp::notification::DidChangeWatchedFiles>(
                        lsp::DidChangeWatchedFilesParams { changes },
                    )
                    .ok();
            }
            Some(())
        });
    }
}
