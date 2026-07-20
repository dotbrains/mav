#![allow(clippy::format_collect)]

mod agent_registry_store;
mod bookmark_store;
mod bookmark_store_fs_events;
mod bookmark_store_restore;
mod bookmark_store_serialization;
mod color_extractor;
mod context_server_lifecycle;
mod context_server_remote;
mod context_server_settings;
mod context_server_status;
mod context_server_store;
mod context_server_timeouts;
mod debugger;
mod default_workdirs;
mod diagnostic_summaries;
mod diagnostics_basic;
mod diagnostics_lifecycle;
mod editorconfig_external;
mod editorconfig_internal;
mod git_store;
mod git_store_conflict_parse;
mod git_store_conflict_updates;
mod git_store_resolve;
mod git_store_traversal;
mod git_store_traversal_mtime;
mod git_store_trust;
mod git_store_worktrees;
mod grouped_diagnostics;
mod image_store;
mod language_server_management;
mod language_server_paths;
mod lsp_command;
mod lsp_completions;
mod lsp_controls;
mod lsp_definition;
mod lsp_edits;
mod lsp_format_code_actions;
mod lsp_store;
mod lsp_watch_registrations;
mod lsp_watch_reporting;
mod lsp_watch_rescan;
mod manifest_tree;
mod project_buffer_lifecycle;
mod project_buffer_saves;
mod project_code_actions;
mod project_diagnostics_transform;
mod project_diff_views;
mod project_disable_ai_settings;
mod project_entry_creation;
mod project_file_status;
mod project_git_helpers;
mod project_git_worktrees;
mod project_hovers;
mod project_ignored_dirs;
mod project_line_endings;
mod project_path_git_tests;
mod project_pending_ops;
mod project_read_only_files;
mod project_rename_entry;
mod project_rename_notifications;
mod project_repository_paths;
mod project_repository_status;
mod project_repository_subfolders;
mod project_scan_tests;
mod project_search;
mod project_search_advanced;
mod project_search_basic;
mod project_search_filters;
mod project_settings;
mod project_single_file_diffs;
mod project_staging_delayed;
mod project_staging_hunks;
mod project_staging_random;
mod project_uncommitted_diffs;
mod project_worktree_reorder;
mod project_worktree_updates;
mod search;
mod search_history;
mod signature_help;
mod task_inventory;
mod task_language_servers;
mod trusted_worktrees;
mod trusted_worktrees_basic;
mod trusted_worktrees_cycles;
mod trusted_worktrees_path_coverage;
mod yarn;

pub(crate) use project_git_helpers::*;

use anyhow::Result;
use async_trait::async_trait;
use buffer_diff::{
    BufferDiffEvent, DiffChanged, DiffHunkSecondaryStatus, DiffHunkStatus, DiffHunkStatusKind,
    assert_hunks,
};
use collections::{BTreeSet, HashMap, HashSet};
use encoding_rs;
use fs::{FakeFs, PathEventKind};
use futures::{StreamExt, future};
use git::{
    GitHostingProviderRegistry,
    repository::{RepoPath, repo_path},
    status::{DiffStat, FileStatus, StatusCode, TrackedStatus},
};
use gpui::{
    App, AppContext, BackgroundExecutor, BorrowAppContext, Entity, FutureExt, SharedString, Task,
    TestAppContext, UpdateGlobal,
};
use itertools::Itertools;
use language::{
    Buffer, BufferEvent, Diagnostic, DiagnosticEntry, DiagnosticEntryRef, DiagnosticSet,
    DiagnosticSourceKind, DiskState, FakeLspAdapter, Language, LanguageAwareStyling,
    LanguageConfig, LanguageMatcher, LanguageName, LineEnding, ManifestName, ManifestProvider,
    ManifestQuery, OffsetRangeExt, Point, ToPoint, Toolchain, ToolchainList, ToolchainLister,
    ToolchainMetadata,
    language_settings::{
        Formatter, FormatterList, LanguageSettings, LanguageSettingsContent, LineEndingSetting,
    },
    markdown_lang, rust_lang, tree_sitter_typescript,
};
use lsp::{
    CodeActionKind, DEFAULT_LSP_REQUEST_TIMEOUT, DiagnosticSeverity, DocumentChanges,
    FileOperationFilter, LanguageServerId, LanguageServerName, NumberOrString, TextDocumentEdit,
    Uri, WillRenameFiles, notification::DidRenameFiles,
};
use parking_lot::Mutex;
use paths::{config_dir, global_gitignore_path, tasks_file};
use postage::stream::Stream as _;
use pretty_assertions::assert_eq;
use project::{
    Event, TaskContexts,
    git_store::{GitStoreEvent, Repository, RepositoryEvent, StatusEntry, pending_op},
    search::{SearchQuery, SearchResult},
    task_store::{TaskSettingsLocation, TaskStore},
    *,
};
use rand::{Rng as _, rngs::StdRng};
use serde_json::json;
use settings::SettingsStore;
#[cfg(target_os = "linux")]
use settings::{LocalSettingsKind, LocalSettingsPath};
#[cfg(not(windows))]
use std::os;
use std::{
    cell::RefCell,
    env, mem,
    num::NonZeroU32,
    ops::Range,
    path::{Path, PathBuf},
    process::Command,
    rc::Rc,
    str::FromStr,
    sync::{Arc, OnceLock, atomic},
    task::Poll,
    time::Duration,
};
use sum_tree::SumTree;
use task::{ResolvedTask, ShellKind, TaskContext};
use text::{Anchor, PointUtf16, ReplicaId, ToOffset, Unclipped};
use unindent::Unindent as _;
use util::{
    TryFutureExt as _, assert_set_eq, maybe, path,
    paths::{PathMatcher, PathStyle},
    rel_path::{RelPath, rel_path},
    test::{TempTree, marked_text_offsets},
    uri,
};
use worktree::WorktreeModelHandle as _;

fn chunks_with_diagnostics<T: ToOffset + ToPoint>(
    buffer: &Buffer,
    range: Range<T>,
) -> Vec<(String, Option<DiagnosticSeverity>)> {
    let mut chunks: Vec<(String, Option<DiagnosticSeverity>)> = Vec::new();
    for chunk in buffer.snapshot().chunks(
        range,
        LanguageAwareStyling {
            tree_sitter: true,
            diagnostics: true,
        },
    ) {
        if chunks
            .last()
            .is_some_and(|prev_chunk| prev_chunk.1 == chunk.diagnostic_severity)
        {
            chunks.last_mut().unwrap().0.push_str(chunk.text);
        } else {
            chunks.push((chunk.text.to_string(), chunk.diagnostic_severity));
        }
    }
    chunks
}

pub fn init_test(cx: &mut gpui::TestAppContext) {
    zlog::init_test();

    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
        release_channel::init(semver::Version::new(0, 0, 0), cx);
    });
}

async fn search(
    project: &Entity<Project>,
    query: SearchQuery,
    cx: &mut gpui::TestAppContext,
) -> Result<HashMap<String, Vec<Range<usize>>>> {
    let search_rx = project.update(cx, |project, cx| project.search(query, cx));
    let mut results = HashMap::default();
    while let Ok(search_result) = search_rx.rx.recv().await {
        match search_result {
            SearchResult::Buffer { buffer, ranges } => {
                results.entry(buffer).or_insert(ranges);
            }
            SearchResult::LimitReached | SearchResult::WaitingForScan | SearchResult::Searching => {
            }
        }
    }
    Ok(results
        .into_iter()
        .map(|(buffer, ranges)| {
            buffer.update(cx, |buffer, cx| {
                let path = buffer
                    .file()
                    .unwrap()
                    .full_path(cx)
                    .to_string_lossy()
                    .to_string();
                let ranges = ranges
                    .into_iter()
                    .map(|range| range.to_offset(buffer))
                    .collect::<Vec<_>>();
                (path, ranges)
            })
        })
        .collect())
}

fn json_lang() -> Arc<Language> {
    Arc::new(Language::new(
        LanguageConfig {
            name: "JSON".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["json".to_string()],
                ..Default::default()
            },
            ..Default::default()
        },
        None,
    ))
}

fn js_lang() -> Arc<Language> {
    Arc::new(Language::new(
        LanguageConfig {
            name: "JavaScript".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["js".to_string()],
                ..Default::default()
            },
            ..Default::default()
        },
        None,
    ))
}

fn python_lang(fs: Arc<FakeFs>) -> Arc<Language> {
    struct PythonMootToolchainLister(Arc<FakeFs>);
    #[async_trait]
    impl ToolchainLister for PythonMootToolchainLister {
        async fn list(
            &self,
            worktree_root: PathBuf,
            subroot_relative_path: Arc<RelPath>,
            _: Option<HashMap<String, String>>,
        ) -> ToolchainList {
            // This lister will always return a path .venv directories within ancestors
            let ancestors = subroot_relative_path.ancestors().collect::<Vec<_>>();
            let mut toolchains = vec![];
            for ancestor in ancestors {
                let venv_path = worktree_root.join(ancestor.as_std_path()).join(".venv");
                if self.0.is_dir(&venv_path).await {
                    toolchains.push(Toolchain {
                        name: SharedString::new_static("Python Venv"),
                        path: venv_path.to_string_lossy().into_owned().into(),
                        language_name: LanguageName(SharedString::new_static("Python")),
                        as_json: serde_json::Value::Null,
                    })
                }
            }
            ToolchainList {
                toolchains,
                ..Default::default()
            }
        }
        async fn resolve(
            &self,
            _: PathBuf,
            _: Option<HashMap<String, String>>,
        ) -> anyhow::Result<Toolchain> {
            Err(anyhow::anyhow!("Not implemented"))
        }
        fn meta(&self) -> ToolchainMetadata {
            ToolchainMetadata {
                term: SharedString::new_static("Virtual Environment"),
                new_toolchain_placeholder: SharedString::new_static(
                    "A path to the python3 executable within a virtual environment, or path to virtual environment itself",
                ),
                manifest_name: ManifestName::from(SharedString::new_static("pyproject.toml")),
            }
        }
        fn activation_script(
            &self,
            _: &Toolchain,
            _: ShellKind,
            _: &gpui::App,
        ) -> futures::future::BoxFuture<'static, Vec<String>> {
            Box::pin(async { vec![] })
        }
    }
    Arc::new(
        Language::new(
            LanguageConfig {
                name: "Python".into(),
                matcher: LanguageMatcher {
                    path_suffixes: vec!["py".to_string()],
                    ..Default::default()
                },
                ..Default::default()
            },
            None, // We're not testing Python parsing with this language.
        )
        .with_manifest(Some(ManifestName::from(SharedString::new_static(
            "pyproject.toml",
        ))))
        .with_toolchain_lister(Some(Arc::new(PythonMootToolchainLister(fs)))),
    )
}

fn typescript_lang() -> Arc<Language> {
    Arc::new(Language::new(
        LanguageConfig {
            name: "TypeScript".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["ts".to_string()],
                ..Default::default()
            },
            ..Default::default()
        },
        Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
    ))
}

fn tsx_lang() -> Arc<Language> {
    Arc::new(Language::new(
        LanguageConfig {
            name: "tsx".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["tsx".to_string()],
                ..Default::default()
            },
            ..Default::default()
        },
        Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
    ))
}

fn get_all_tasks(
    project: &Entity<Project>,
    task_contexts: Arc<TaskContexts>,
    cx: &mut App,
) -> Task<Vec<(TaskSourceKind, ResolvedTask)>> {
    let new_tasks = project.update(cx, |project, cx| {
        project.task_store().update(cx, |task_store, cx| {
            task_store.task_inventory().unwrap().update(cx, |this, cx| {
                this.used_and_current_resolved_tasks(task_contexts, cx)
            })
        })
    });

    cx.background_spawn(async move {
        let (mut old, new) = new_tasks.await;
        old.extend(new);
        old
    })
}

#[track_caller]
fn assert_entry_git_state(
    tree: &Worktree,
    repository: &Repository,
    path: &str,
    index_status: Option<StatusCode>,
    is_ignored: bool,
) {
    assert_eq!(tree.abs_path(), repository.work_directory_abs_path);
    let entry = tree
        .entry_for_path(&rel_path(path))
        .unwrap_or_else(|| panic!("entry {path} not found"));
    let status = repository
        .status_for_path(&repo_path(path))
        .map(|entry| entry.status);
    let expected = index_status.map(|index_status| {
        TrackedStatus {
            index_status,
            worktree_status: StatusCode::Unmodified,
        }
        .into()
    });
    assert_eq!(
        status, expected,
        "expected {path} to have git status: {expected:?}"
    );
    assert_eq!(
        entry.is_ignored, is_ignored,
        "expected {path} to have is_ignored: {is_ignored}"
    );
}
