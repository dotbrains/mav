use super::LspStore;
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
