#![allow(clippy::format_collect)]

mod agent_registry_store;
mod bookmark_store;
mod color_extractor;
mod context_server_store;
mod debugger;
mod default_workdirs;
mod diagnostic_summaries;
mod diagnostics_basic;
mod diagnostics_lifecycle;
mod editorconfig_external;
mod editorconfig_internal;
mod git_store;
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
mod project_entry_creation;
mod project_file_status;
mod project_git_worktrees;
mod project_hovers;
mod project_ignored_dirs;
mod project_line_endings;
mod project_pending_ops;
mod project_rename_entry;
mod project_rename_notifications;
mod project_repository_paths;
mod project_repository_status;
mod project_repository_subfolders;
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
mod yarn;

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

#[gpui::test]
async fn test_undo_encoding_change(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());

    // Create a file with ASCII content "Hi" - this will be detected as UTF-8
    // When reinterpreted as UTF-16LE, the bytes 0x48 0x69 become a single character
    let ascii_bytes: Vec<u8> = vec![0x48, 0x69];
    fs.insert_tree(path!("/dir"), json!({})).await;
    fs.insert_file(path!("/dir/test.txt"), ascii_bytes).await;

    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;

    let buffer = project
        .update(cx, |p, cx| p.open_local_buffer(path!("/dir/test.txt"), cx))
        .await
        .unwrap();

    let (initial_encoding, initial_text, initial_dirty) = buffer.read_with(cx, |buffer, _| {
        (buffer.encoding(), buffer.text(), buffer.is_dirty())
    });
    assert_eq!(initial_encoding, encoding_rs::UTF_8);
    assert_eq!(initial_text, "Hi");
    assert!(!initial_dirty);

    let reload_receiver = buffer.update(cx, |buffer, cx| {
        buffer.reload_with_encoding(encoding_rs::UTF_16LE, cx)
    });
    cx.executor().run_until_parked();

    // Wait for reload to complete
    let _ = reload_receiver.await;

    // Verify the encoding changed, text is different, and still not dirty (we reloaded from disk)
    let (reloaded_encoding, reloaded_text, reloaded_dirty) = buffer.read_with(cx, |buffer, _| {
        (buffer.encoding(), buffer.text(), buffer.is_dirty())
    });
    assert_eq!(reloaded_encoding, encoding_rs::UTF_16LE);
    assert_eq!(reloaded_text, "楈");
    assert!(!reloaded_dirty);

    // Undo the reload
    buffer.update(cx, |buffer, cx| {
        buffer.undo(cx);
    });

    buffer.read_with(cx, |buffer, _| {
        assert_eq!(buffer.encoding(), encoding_rs::UTF_8);
        assert_eq!(buffer.text(), "Hi");
        assert!(!buffer.is_dirty());
    });

    buffer.update(cx, |buffer, cx| {
        buffer.redo(cx);
    });

    buffer.read_with(cx, |buffer, _| {
        assert_eq!(buffer.encoding(), encoding_rs::UTF_16LE);
        assert_ne!(buffer.text(), "Hi");
        assert!(!buffer.is_dirty());
    });
}

#[gpui::test]
async fn test_initial_scan_complete(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "a": {
                ".git": {},
                ".mav": {
                    "tasks.json": r#"[{"label": "task-a", "command": "echo a"}]"#
                },
                "src": { "main.rs": "" }
            },
            "b": {
                ".git": {},
                ".mav": {
                    "tasks.json": r#"[{"label": "task-b", "command": "echo b"}]"#
                },
                "src": { "lib.rs": "" }
            },
        }),
    )
    .await;

    let repos_created = Rc::new(RefCell::new(Vec::new()));
    let _observe = {
        let repos_created = repos_created.clone();
        cx.update(|cx| {
            cx.observe_new::<Repository>(move |repo, _, cx| {
                repos_created.borrow_mut().push(cx.entity().downgrade());
                let _ = repo;
            })
        })
    };

    let project = Project::test(
        fs.clone(),
        [path!("/root/a").as_ref(), path!("/root/b").as_ref()],
        cx,
    )
    .await;

    let scan_complete = project.read_with(cx, |project, cx| project.wait_for_initial_scan(cx));
    scan_complete.await;

    project.read_with(cx, |project, cx| {
        assert!(
            project.worktree_store().read(cx).initial_scan_completed(),
            "Expected initial scan to be completed after awaiting wait_for_initial_scan"
        );
    });

    let created_repos_len = repos_created.borrow().len();
    assert_eq!(
        created_repos_len, 2,
        "Expected 2 repositories to be created during scan, got {}",
        created_repos_len
    );

    project.read_with(cx, |project, cx| {
        let git_store = project.git_store().read(cx);
        assert_eq!(
            git_store.repositories().len(),
            2,
            "Expected 2 repositories in GitStore"
        );
    });
}

pub fn init_test(cx: &mut gpui::TestAppContext) {
    zlog::init_test();

    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
        release_channel::init(semver::Version::new(0, 0, 0), cx);
    });
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

#[cfg(any())]
const GIT_STATUS_CONFLICTED: &str = "UU";

#[allow(clippy::disallowed_methods)]
fn git_cmd(work_dir: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.current_dir(work_dir)
        .env("GIT_CONFIG_GLOBAL", "")
        .env("GIT_CONFIG_SYSTEM", "")
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@mav.dev")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@mav.dev");
    cmd
}

#[allow(clippy::disallowed_methods)]
#[track_caller]
fn git_init(path: &Path) -> PathBuf {
    let output = git_cmd(path)
        .args(["init", "-b", "main"])
        .output()
        .expect("Failed to run git init");
    assert!(
        output.status.success(),
        "git init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    path.to_path_buf()
}

#[allow(clippy::disallowed_methods)]
#[track_caller]
fn git_add<P: AsRef<Path>>(path: P, work_dir: &Path) {
    let output = git_cmd(work_dir)
        .args(["add"])
        .arg(path.as_ref())
        .output()
        .expect("Failed to run git add");
    assert!(
        output.status.success(),
        "git add failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[allow(clippy::disallowed_methods)]
#[track_caller]
fn git_remove_index(path: &Path, work_dir: &Path) {
    let output = git_cmd(work_dir)
        .args(["rm", "--cached"])
        .arg(path)
        .output()
        .expect("Failed to run git rm");
    assert!(
        output.status.success(),
        "git rm --cached failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[allow(clippy::disallowed_methods)]
#[track_caller]
fn git_commit(msg: &str, work_dir: &Path) {
    let output = git_cmd(work_dir)
        .args(["commit", "-m", msg])
        .output()
        .expect("Failed to run git commit");
    assert!(
        output.status.success(),
        "git commit failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[allow(clippy::disallowed_methods)]
#[track_caller]
fn git_stash(work_dir: &Path) {
    let output = git_cmd(work_dir)
        .args(["stash"])
        .output()
        .expect("Failed to run git stash");
    assert!(
        output.status.success(),
        "git stash failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[allow(clippy::disallowed_methods)]
#[track_caller]
fn git_reset(offset: usize, work_dir: &Path) {
    let target = format!("HEAD~{}", offset + 1);
    let output = git_cmd(work_dir)
        .args(["reset", "--soft", &target])
        .output()
        .expect("Failed to run git reset");
    assert!(
        output.status.success(),
        "git reset failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(target_os = "linux")]
#[allow(clippy::disallowed_methods)]
#[track_caller]
fn git_branch(name: &str, work_dir: &Path) {
    let output = git_cmd(work_dir)
        .args(["branch", name])
        .output()
        .expect("Failed to run git branch");
    assert!(
        output.status.success(),
        "git branch failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(target_os = "linux")]
#[allow(clippy::disallowed_methods)]
#[track_caller]
fn git_checkout(name: &str, work_dir: &Path) {
    let output = git_cmd(work_dir)
        .args(["checkout", name])
        .output()
        .expect("Failed to run git checkout");
    assert!(
        output.status.success(),
        "git checkout failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(any())]
#[allow(clippy::disallowed_methods)]
#[track_caller]
fn git_rev_parse(rev: &str, work_dir: &Path) -> String {
    let output = git_cmd(work_dir)
        .args(["rev-parse", rev])
        .output()
        .expect("Failed to run git rev-parse");
    assert!(
        output.status.success(),
        "git rev-parse failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

#[cfg(any())]
#[allow(clippy::disallowed_methods)]
#[track_caller]
fn git_cherry_pick_expect_conflict(commit: &str, work_dir: &Path) {
    let output = git_cmd(work_dir)
        .args(["cherry-pick", "--no-commit", commit])
        .output()
        .expect("Failed to run git cherry-pick");
    assert!(
        !output.status.success(),
        "git cherry-pick unexpectedly succeeded"
    );
}

#[cfg(any())]
#[allow(clippy::disallowed_methods)]
#[track_caller]
fn git_status(work_dir: &Path) -> collections::HashMap<String, String> {
    let output = git_cmd(work_dir)
        .args(["status", "--porcelain=v1"])
        .output()
        .expect("Failed to run git status");
    assert!(
        output.status.success(),
        "git status failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            let status = line[..2].to_string();
            let path = line[3..].to_string();
            (path, status)
        })
        .collect()
}

#[gpui::test]
async fn test_find_project_path_abs(
    background_executor: BackgroundExecutor,
    cx: &mut gpui::TestAppContext,
) {
    // find_project_path should work with absolute paths
    init_test(cx);

    let fs = FakeFs::new(background_executor);
    fs.insert_tree(
        path!("/root"),
        json!({
            "project1": {
                "file1.txt": "content1",
                "subdir": {
                    "file2.txt": "content2"
                }
            },
            "project2": {
                "file3.txt": "content3"
            }
        }),
    )
    .await;

    let project = Project::test(
        fs.clone(),
        [
            path!("/root/project1").as_ref(),
            path!("/root/project2").as_ref(),
        ],
        cx,
    )
    .await;

    // Make sure the worktrees are fully initialized
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.run_until_parked();

    let (project1_abs_path, project1_id, project2_abs_path, project2_id) =
        project.read_with(cx, |project, cx| {
            let worktrees: Vec<_> = project.worktrees(cx).collect();
            let abs_path1 = worktrees[0].read(cx).abs_path().to_path_buf();
            let id1 = worktrees[0].read(cx).id();
            let abs_path2 = worktrees[1].read(cx).abs_path().to_path_buf();
            let id2 = worktrees[1].read(cx).id();
            (abs_path1, id1, abs_path2, id2)
        });

    project.update(cx, |project, cx| {
        let abs_path = project1_abs_path.join("file1.txt");
        let found_path = project.find_project_path(abs_path, cx).unwrap();
        assert_eq!(found_path.worktree_id, project1_id);
        assert_eq!(&*found_path.path, rel_path("file1.txt"));

        let abs_path = project1_abs_path.join("subdir").join("file2.txt");
        let found_path = project.find_project_path(abs_path, cx).unwrap();
        assert_eq!(found_path.worktree_id, project1_id);
        assert_eq!(&*found_path.path, rel_path("subdir/file2.txt"));

        let abs_path = project2_abs_path.join("file3.txt");
        let found_path = project.find_project_path(abs_path, cx).unwrap();
        assert_eq!(found_path.worktree_id, project2_id);
        assert_eq!(&*found_path.path, rel_path("file3.txt"));

        let abs_path = project1_abs_path.join("nonexistent.txt");
        let found_path = project.find_project_path(abs_path, cx);
        assert!(
            found_path.is_some(),
            "Should find project path for nonexistent file in worktree"
        );

        // Test with an absolute path outside any worktree
        let abs_path = Path::new("/some/other/path");
        let found_path = project.find_project_path(abs_path, cx);
        assert!(
            found_path.is_none(),
            "Should not find project path for path outside any worktree"
        );
    });
}

#[gpui::test]
async fn test_git_worktree_remove(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "a": {
                ".git": {},
                "src": {
                    "main.rs": "fn main() {}",
                }
            },
            "b": {
                ".git": {},
                "src": {
                    "main.rs": "fn main() {}",
                },
                "script": {
                    "run.sh": "#!/bin/bash"
                }
            }
        }),
    )
    .await;

    let project = Project::test(
        fs.clone(),
        [
            path!("/root/a").as_ref(),
            path!("/root/b/script").as_ref(),
            path!("/root/b").as_ref(),
        ],
        cx,
    )
    .await;
    let scan_complete = project.update(cx, |project, cx| project.git_scans_complete(cx));
    scan_complete.await;

    let worktrees = project.update(cx, |project, cx| project.worktrees(cx).collect::<Vec<_>>());
    assert_eq!(worktrees.len(), 3);

    let worktree_id_by_abs_path = worktrees
        .into_iter()
        .map(|worktree| worktree.read_with(cx, |w, _| (w.abs_path(), w.id())))
        .collect::<HashMap<_, _>>();
    let worktree_id = worktree_id_by_abs_path
        .get(Path::new(path!("/root/b/script")))
        .unwrap();

    let repos = project.update(cx, |p, cx| p.git_store().read(cx).repositories().clone());
    assert_eq!(repos.len(), 2);

    project.update(cx, |project, cx| {
        project.remove_worktree(*worktree_id, cx);
    });
    cx.run_until_parked();

    let mut repo_paths = project
        .update(cx, |p, cx| p.git_store().read(cx).repositories().clone())
        .values()
        .map(|repo| repo.read_with(cx, |r, _| r.work_directory_abs_path.clone()))
        .collect::<Vec<_>>();
    repo_paths.sort();

    pretty_assertions::assert_eq!(
        repo_paths,
        [
            Path::new(path!("/root/a")).into(),
            Path::new(path!("/root/b")).into(),
        ]
    );

    let active_repo_path = project
        .read_with(cx, |p, cx| {
            p.active_repository(cx)
                .map(|r| r.read(cx).work_directory_abs_path.clone())
        })
        .unwrap();
    assert_eq!(active_repo_path.as_ref(), Path::new(path!("/root/a")));

    let worktree_id = worktree_id_by_abs_path
        .get(Path::new(path!("/root/a")))
        .unwrap();
    project.update(cx, |project, cx| {
        project.remove_worktree(*worktree_id, cx);
    });
    cx.run_until_parked();

    let active_repo_path = project
        .read_with(cx, |p, cx| {
            p.active_repository(cx)
                .map(|r| r.read(cx).work_directory_abs_path.clone())
        })
        .unwrap();
    assert_eq!(active_repo_path.as_ref(), Path::new(path!("/root/b")));

    let worktree_id = worktree_id_by_abs_path
        .get(Path::new(path!("/root/b")))
        .unwrap();
    project.update(cx, |project, cx| {
        project.remove_worktree(*worktree_id, cx);
    });
    cx.run_until_parked();

    let active_repo_path = project.read_with(cx, |p, cx| {
        p.active_repository(cx)
            .map(|r| r.read(cx).work_directory_abs_path.clone())
    });
    assert!(active_repo_path.is_none());
}

#[gpui::test]
async fn test_optimistic_hunks_in_staged_files(cx: &mut gpui::TestAppContext) {
    use DiffHunkSecondaryStatus::*;
    init_test(cx);

    let committed_contents = r#"
        one
        two
        three
    "#
    .unindent();
    let file_contents = r#"
        one
        TWO
        three
    "#
    .unindent();

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/dir"),
        json!({
            ".git": {},
            "file.txt": file_contents.clone()
        }),
    )
    .await;

    fs.set_head_and_index_for_repo(
        path!("/dir/.git").as_ref(),
        &[("file.txt", committed_contents.clone())],
    );

    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;

    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/dir/file.txt"), cx)
        })
        .await
        .unwrap();
    let snapshot = buffer.read_with(cx, |buffer, _| buffer.snapshot());
    let uncommitted_diff = project
        .update(cx, |project, cx| {
            project.open_uncommitted_diff(buffer.clone(), cx)
        })
        .await
        .unwrap();

    // The hunk is initially unstaged.
    uncommitted_diff.read_with(cx, |diff, cx| {
        assert_hunks(
            diff.snapshot(cx).hunks(&snapshot),
            &snapshot,
            &diff.base_text_string(cx).unwrap(),
            &[(
                1..2,
                "two\n",
                "TWO\n",
                DiffHunkStatus::modified(HasSecondaryHunk),
            )],
        );
    });

    // Get the repository handle.
    let repo = project.read_with(cx, |project, cx| {
        project.repositories(cx).values().next().unwrap().clone()
    });

    // Stage the file.
    let stage_task = repo.update(cx, |repo, cx| {
        repo.stage_entries(vec![repo_path("file.txt")], cx)
    });

    // Run a few ticks to let the job start and mark hunks as pending,
    // but don't run_until_parked which would complete the entire operation.
    for _ in 0..10 {
        cx.executor().tick();
        let [hunk]: [_; 1] = uncommitted_diff
            .read_with(cx, |diff, cx| {
                diff.snapshot(cx).hunks(&snapshot).collect::<Vec<_>>()
            })
            .try_into()
            .unwrap();
        match hunk.secondary_status {
            HasSecondaryHunk => {}
            SecondaryHunkRemovalPending => break,
            NoSecondaryHunk => panic!("hunk was not optimistically staged"),
            _ => panic!("unexpected hunk state"),
        }
    }
    uncommitted_diff.read_with(cx, |diff, cx| {
        assert_hunks(
            diff.snapshot(cx).hunks(&snapshot),
            &snapshot,
            &diff.base_text_string(cx).unwrap(),
            &[(
                1..2,
                "two\n",
                "TWO\n",
                DiffHunkStatus::modified(SecondaryHunkRemovalPending),
            )],
        );
    });

    // Let the staging complete.
    stage_task.await.unwrap();
    cx.run_until_parked();

    // The hunk is now fully staged.
    uncommitted_diff.read_with(cx, |diff, cx| {
        assert_hunks(
            diff.snapshot(cx).hunks(&snapshot),
            &snapshot,
            &diff.base_text_string(cx).unwrap(),
            &[(
                1..2,
                "two\n",
                "TWO\n",
                DiffHunkStatus::modified(NoSecondaryHunk),
            )],
        );
    });

    // Simulate a commit by updating HEAD to match the current file contents.
    // The FakeGitRepository's commit method is a no-op, so we need to manually
    // update HEAD to simulate the commit completing.
    fs.set_head_for_repo(
        path!("/dir/.git").as_ref(),
        &[("file.txt", file_contents.clone())],
        "newhead",
    );
    cx.run_until_parked();

    // After committing, there are no more hunks.
    uncommitted_diff.read_with(cx, |diff, cx| {
        assert_hunks(
            diff.snapshot(cx).hunks(&snapshot),
            &snapshot,
            &diff.base_text_string(cx).unwrap(),
            &[] as &[(Range<u32>, &str, &str, DiffHunkStatus)],
        );
    });
}

#[gpui::test]
async fn test_read_only_files_setting(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    // Configure read_only_files setting
    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.worktree.read_only_files = Some(vec![
                    "**/generated/**".to_string(),
                    "**/*.gen.rs".to_string(),
                ]);
            });
        });
    });

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/root"),
        json!({
            "src": {
                "main.rs": "fn main() {}",
                "types.gen.rs": "// Generated file",
            },
            "generated": {
                "schema.rs": "// Auto-generated schema",
            }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;

    // Open a regular file - should be read-write
    let regular_buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/root/src/main.rs"), cx)
        })
        .await
        .unwrap();

    regular_buffer.read_with(cx, |buffer, _| {
        assert!(!buffer.read_only(), "Regular file should not be read-only");
    });

    // Open a file matching *.gen.rs pattern - should be read-only
    let gen_buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/root/src/types.gen.rs"), cx)
        })
        .await
        .unwrap();

    gen_buffer.read_with(cx, |buffer, _| {
        assert!(
            buffer.read_only(),
            "File matching *.gen.rs pattern should be read-only"
        );
    });

    // Open a file in generated directory - should be read-only
    let generated_buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/root/generated/schema.rs"), cx)
        })
        .await
        .unwrap();

    generated_buffer.read_with(cx, |buffer, _| {
        assert!(
            buffer.read_only(),
            "File in generated directory should be read-only"
        );
    });
}

#[gpui::test]
async fn test_read_only_files_empty_setting(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    // Explicitly set read_only_files to empty (default behavior)
    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.worktree.read_only_files = Some(vec![]);
            });
        });
    });

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/root"),
        json!({
            "src": {
                "main.rs": "fn main() {}",
            },
            "generated": {
                "schema.rs": "// Auto-generated schema",
            }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;

    // All files should be read-write when read_only_files is empty
    let main_buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/root/src/main.rs"), cx)
        })
        .await
        .unwrap();

    main_buffer.read_with(cx, |buffer, _| {
        assert!(
            !buffer.read_only(),
            "Files should not be read-only when read_only_files is empty"
        );
    });

    let generated_buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/root/generated/schema.rs"), cx)
        })
        .await
        .unwrap();

    generated_buffer.read_with(cx, |buffer, _| {
        assert!(
            !buffer.read_only(),
            "Generated files should not be read-only when read_only_files is empty"
        );
    });
}

#[gpui::test]
async fn test_read_only_files_with_lock_files(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    // Configure to make lock files read-only
    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.worktree.read_only_files = Some(vec![
                    "**/*.lock".to_string(),
                    "**/package-lock.json".to_string(),
                ]);
            });
        });
    });

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/root"),
        json!({
            "Cargo.lock": "# Lock file",
            "Cargo.toml": "[package]",
            "package-lock.json": "{}",
            "package.json": "{}",
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;

    // Cargo.lock should be read-only
    let cargo_lock = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/root/Cargo.lock"), cx)
        })
        .await
        .unwrap();

    cargo_lock.read_with(cx, |buffer, _| {
        assert!(buffer.read_only(), "Cargo.lock should be read-only");
    });

    // Cargo.toml should be read-write
    let cargo_toml = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/root/Cargo.toml"), cx)
        })
        .await
        .unwrap();

    cargo_toml.read_with(cx, |buffer, _| {
        assert!(!buffer.read_only(), "Cargo.toml should not be read-only");
    });

    // package-lock.json should be read-only
    let package_lock = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/root/package-lock.json"), cx)
        })
        .await
        .unwrap();

    package_lock.read_with(cx, |buffer, _| {
        assert!(buffer.read_only(), "package-lock.json should be read-only");
    });

    // package.json should be read-write
    let package_json = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/root/package.json"), cx)
        })
        .await
        .unwrap();

    package_json.read_with(cx, |buffer, _| {
        assert!(!buffer.read_only(), "package.json should not be read-only");
    });
}

mod disable_ai_settings_tests {
    use gpui::TestAppContext;
    use project::*;
    use settings::{Settings, SettingsStore};

    #[gpui::test]
    async fn test_disable_ai_settings_security(cx: &mut TestAppContext) {
        cx.update(|cx| {
            settings::init(cx);

            // Test 1: Default is false (AI enabled)
            assert!(
                !DisableAiSettings::get_global(cx).disable_ai,
                "Default should allow AI"
            );
        });

        let disable_true = serde_json::json!({
            "disable_ai": true
        })
        .to_string();
        let disable_false = serde_json::json!({
            "disable_ai": false
        })
        .to_string();

        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.set_user_settings(&disable_false, cx).unwrap();
            store.set_global_settings(&disable_true, cx).unwrap();
        });
        cx.update(|cx| {
            assert!(
                DisableAiSettings::get_global(cx).disable_ai,
                "Local false cannot override global true"
            );
        });

        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.set_global_settings(&disable_false, cx).unwrap();
            store.set_user_settings(&disable_true, cx).unwrap();
        });

        cx.update(|cx| {
            assert!(
                DisableAiSettings::get_global(cx).disable_ai,
                "Local false cannot override global true"
            );
        });
    }

    #[gpui::test]
    async fn test_disable_ai_project_level_settings(cx: &mut TestAppContext) {
        use settings::{LocalSettingsKind, LocalSettingsPath, SettingsLocation, SettingsStore};
        use worktree::WorktreeId;

        cx.update(|cx| {
            settings::init(cx);

            // Default should allow AI
            assert!(
                !DisableAiSettings::get_global(cx).disable_ai,
                "Default should allow AI"
            );
        });

        let worktree_id = WorktreeId::from_usize(1);
        let rel_path = |path: &str| -> std::sync::Arc<util::rel_path::RelPath> {
            std::sync::Arc::from(util::rel_path::RelPath::unix(path).unwrap())
        };
        let project_path = rel_path("project");
        let settings_location = SettingsLocation {
            worktree_id,
            path: project_path.as_ref(),
        };

        // Test: Project-level disable_ai=true should disable AI for files in that project
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store
                .set_local_settings(
                    worktree_id,
                    LocalSettingsPath::InWorktree(project_path.clone()),
                    LocalSettingsKind::Settings,
                    Some(r#"{ "disable_ai": true }"#),
                    cx,
                )
                .unwrap();
        });

        cx.update(|cx| {
            let settings = DisableAiSettings::get(Some(settings_location), cx);
            assert!(
                settings.disable_ai,
                "Project-level disable_ai=true should disable AI for files in that project"
            );
            // Global should now also be true since project-level disable_ai is merged into global
            assert!(
                DisableAiSettings::get_global(cx).disable_ai,
                "Global setting should be affected by project-level disable_ai=true"
            );
        });

        // Test: Setting project-level to false should allow AI for that project
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store
                .set_local_settings(
                    worktree_id,
                    LocalSettingsPath::InWorktree(project_path.clone()),
                    LocalSettingsKind::Settings,
                    Some(r#"{ "disable_ai": false }"#),
                    cx,
                )
                .unwrap();
        });

        cx.update(|cx| {
            let settings = DisableAiSettings::get(Some(settings_location), cx);
            assert!(
                !settings.disable_ai,
                "Project-level disable_ai=false should allow AI"
            );
            // Global should also be false now
            assert!(
                !DisableAiSettings::get_global(cx).disable_ai,
                "Global setting should be false when project-level is false"
            );
        });

        // Test: User-level true + project-level false = AI disabled (saturation)
        let disable_true = serde_json::json!({ "disable_ai": true }).to_string();
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.set_user_settings(&disable_true, cx).unwrap();
            store
                .set_local_settings(
                    worktree_id,
                    LocalSettingsPath::InWorktree(project_path.clone()),
                    LocalSettingsKind::Settings,
                    Some(r#"{ "disable_ai": false }"#),
                    cx,
                )
                .unwrap();
        });

        cx.update(|cx| {
            let settings = DisableAiSettings::get(Some(settings_location), cx);
            assert!(
                settings.disable_ai,
                "Project-level false cannot override user-level true (SaturatingBool)"
            );
        });
    }
}
