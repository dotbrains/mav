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
mod project_entry_creation;
mod project_hovers;
mod project_line_endings;
mod project_rename_entry;
mod project_rename_notifications;
mod project_search;
mod project_search_advanced;
mod project_search_basic;
mod project_search_filters;
mod project_settings;
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

#[gpui::test]
async fn test_unstaged_diff_for_buffer(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let staged_contents = r#"
        fn main() {
            println!("hello world");
        }
    "#
    .unindent();
    let file_contents = r#"
        // print goodbye
        fn main() {
            println!("goodbye world");
        }
    "#
    .unindent();

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/dir",
        json!({
            ".git": {},
           "src": {
               "main.rs": file_contents,
           }
        }),
    )
    .await;

    fs.set_index_for_repo(Path::new("/dir/.git"), &[("src/main.rs", staged_contents)]);

    let project = Project::test(fs.clone(), ["/dir".as_ref()], cx).await;

    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer("/dir/src/main.rs", cx)
        })
        .await
        .unwrap();
    let unstaged_diff = project
        .update(cx, |project, cx| {
            project.open_unstaged_diff(buffer.clone(), cx)
        })
        .await
        .unwrap();

    cx.run_until_parked();
    unstaged_diff.update(cx, |unstaged_diff, cx| {
        let snapshot = buffer.read(cx).snapshot();
        assert_hunks(
            unstaged_diff.snapshot(cx).hunks(&snapshot),
            &snapshot,
            &unstaged_diff.base_text_string(cx).unwrap(),
            &[
                (0..1, "", "// print goodbye\n", DiffHunkStatus::added_none()),
                (
                    2..3,
                    "    println!(\"hello world\");\n",
                    "    println!(\"goodbye world\");\n",
                    DiffHunkStatus::modified_none(),
                ),
            ],
        );
    });

    let staged_contents = r#"
        // print goodbye
        fn main() {
        }
    "#
    .unindent();

    fs.set_index_for_repo(Path::new("/dir/.git"), &[("src/main.rs", staged_contents)]);

    cx.run_until_parked();
    unstaged_diff.update(cx, |unstaged_diff, cx| {
        let snapshot = buffer.read(cx).snapshot();
        assert_hunks(
            unstaged_diff.snapshot(cx).hunks_intersecting_range(
                Anchor::min_max_range_for_buffer(snapshot.remote_id()),
                &snapshot,
            ),
            &snapshot,
            &unstaged_diff.base_text(cx).text(),
            &[(
                2..3,
                "",
                "    println!(\"goodbye world\");\n",
                DiffHunkStatus::added_none(),
            )],
        );
    });
}

#[gpui::test]
async fn test_reopening_unstaged_diff_after_drop(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let staged_contents = r#"
        fn main() {
            println!("hello world");
        }
    "#
    .unindent();
    let file_contents = r#"
        // print goodbye
        fn main() {
            println!("goodbye world");
        }
    "#
    .unindent();

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/dir",
        json!({
            ".git": {},
           "src": {
               "main.rs": file_contents,
           }
        }),
    )
    .await;
    fs.set_index_for_repo(Path::new("/dir/.git"), &[("src/main.rs", staged_contents)]);

    let project = Project::test(fs.clone(), ["/dir".as_ref()], cx).await;
    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer("/dir/src/main.rs", cx)
        })
        .await
        .unwrap();
    let buffer_id = buffer.read_with(cx, |buffer, _| buffer.remote_id());

    let unstaged_diff = project
        .update(cx, |project, cx| {
            project.open_unstaged_diff(buffer.clone(), cx)
        })
        .await
        .unwrap();
    cx.run_until_parked();

    // Drop the diff while the buffer (and its git state) stays alive.
    drop(unstaged_diff);
    cx.run_until_parked();
    project.read_with(cx, |project, cx| {
        assert!(
            project
                .git_store()
                .read(cx)
                .get_unstaged_diff(buffer_id, cx)
                .is_none(),
            "unstaged diff should have been released"
        );
    });

    // Reopen the diff. The new entity must be registered in the git store,
    // and its hunks must be recalculated.
    let unstaged_diff = project
        .update(cx, |project, cx| {
            project.open_unstaged_diff(buffer.clone(), cx)
        })
        .await
        .unwrap();
    cx.run_until_parked();

    project.read_with(cx, |project, cx| {
        let registered = project
            .git_store()
            .read(cx)
            .get_unstaged_diff(buffer_id, cx);
        assert_eq!(
            registered.as_ref(),
            Some(&unstaged_diff),
            "reopened unstaged diff should be registered in the git store"
        );
    });

    unstaged_diff.update(cx, |unstaged_diff, cx| {
        let snapshot = buffer.read(cx).snapshot();
        assert_hunks(
            unstaged_diff.snapshot(cx).hunks(&snapshot),
            &snapshot,
            &unstaged_diff.base_text_string(cx).unwrap(),
            &[
                (0..1, "", "// print goodbye\n", DiffHunkStatus::added_none()),
                (
                    2..3,
                    "    println!(\"hello world\");\n",
                    "    println!(\"goodbye world\");\n",
                    DiffHunkStatus::modified_none(),
                ),
            ],
        );
    });
}

#[gpui::test]
async fn test_staged_diff_for_buffer(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let committed_contents = r#"
        fn main() {
            println!("hello world");
        }
    "#
    .unindent();
    let staged_contents = r#"
        // print goodbye
        fn main() {
            println!("goodbye world");
        }
    "#
    .unindent();
    let file_contents = r#"
        // print goodbye
        fn main() {
            println!("working copy only");
        }
    "#
    .unindent();

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/dir",
        json!({
            ".git": {},
           "src": {
               "main.rs": file_contents,
           }
        }),
    )
    .await;
    fs.set_head_for_repo(
        Path::new("/dir/.git"),
        &[("src/main.rs", committed_contents)],
        "deadbeef",
    );
    fs.set_index_for_repo(Path::new("/dir/.git"), &[("src/main.rs", staged_contents)]);

    let project = Project::test(fs.clone(), ["/dir".as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    let language = rust_lang();
    language_registry.add(language.clone());

    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer("/dir/src/main.rs", cx)
        })
        .await
        .unwrap();
    let unstaged_diff = project
        .update(cx, |project, cx| {
            project.open_unstaged_diff(buffer.clone(), cx)
        })
        .await
        .unwrap();
    cx.run_until_parked();
    unstaged_diff.read_with(cx, |diff, cx| {
        assert_eq!(diff.base_text(cx).language().cloned(), None);
    });

    let staged_diff = project
        .update(cx, |project, cx| {
            project.open_staged_diff(buffer.clone(), cx)
        })
        .await
        .unwrap();

    cx.run_until_parked();
    unstaged_diff.read_with(cx, |diff, cx| {
        assert_eq!(
            diff.base_text(cx).language().cloned(),
            Some(language.clone())
        );
    });
    staged_diff.update(cx, |staged_diff, cx| {
        let snapshot = staged_diff.snapshot(cx);
        let buffer_snapshot = snapshot.buffer_snapshot();
        assert_hunks(
            snapshot.hunks(buffer_snapshot),
            buffer_snapshot,
            &staged_diff.base_text_string(cx).unwrap(),
            &[
                (0..1, "", "// print goodbye\n", DiffHunkStatus::added_none()),
                (
                    2..3,
                    "    println!(\"hello world\");\n",
                    "    println!(\"goodbye world\");\n",
                    DiffHunkStatus::modified_none(),
                ),
            ],
        );
    });

    let staged_contents = r#"
        // print goodbye
        fn main() {
        }
    "#
    .unindent();
    fs.set_index_for_repo(Path::new("/dir/.git"), &[("src/main.rs", staged_contents)]);

    cx.run_until_parked();
    staged_diff.update(cx, |staged_diff, cx| {
        let snapshot = staged_diff.snapshot(cx);
        let buffer_snapshot = snapshot.buffer_snapshot();
        assert_hunks(
            snapshot.hunks(buffer_snapshot),
            buffer_snapshot,
            &staged_diff.base_text_string(cx).unwrap(),
            &[
                (0..1, "", "// print goodbye\n", DiffHunkStatus::added_none()),
                (
                    2..2,
                    "    println!(\"hello world\");\n",
                    "",
                    DiffHunkStatus::deleted_none(),
                ),
            ],
        );
    });
}

#[gpui::test]
async fn test_base_text_buffers_released_when_diffs_dropped(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let committed_contents = "one\ntwo\nthree\n";
    let staged_contents = "one\nTWO\nthree\n";
    let file_contents = "one\nTWO\nTHREE\n";

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/dir",
        json!({
            ".git": {},
            "src": {
                "main.rs": file_contents,
            }
        }),
    )
    .await;
    fs.set_head_for_repo(
        Path::new("/dir/.git"),
        &[("src/main.rs", committed_contents.to_owned())],
        "deadbeef",
    );
    fs.set_index_for_repo(
        Path::new("/dir/.git"),
        &[("src/main.rs", staged_contents.to_owned())],
    );

    let project = Project::test(fs.clone(), ["/dir".as_ref()], cx).await;
    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer("/dir/src/main.rs", cx)
        })
        .await
        .unwrap();

    let uncommitted_diff = project
        .update(cx, |project, cx| {
            project.open_uncommitted_diff(buffer.clone(), cx)
        })
        .await
        .unwrap();
    let unstaged_diff = project
        .update(cx, |project, cx| {
            project.open_unstaged_diff(buffer.clone(), cx)
        })
        .await
        .unwrap();
    cx.run_until_parked();

    let weak_head_text_buffer =
        uncommitted_diff.read_with(cx, |diff, _| diff.base_text_buffer().downgrade());
    let weak_index_text_buffer =
        unstaged_diff.read_with(cx, |diff, _| diff.base_text_buffer().downgrade());

    drop(uncommitted_diff);
    cx.run_until_parked();
    cx.update(|_| {});
    weak_head_text_buffer.assert_released();
    assert!(
        weak_index_text_buffer.upgrade().is_some(),
        "index text buffer should stay alive while the unstaged diff is open"
    );

    drop(unstaged_diff);
    cx.run_until_parked();
    cx.update(|_| {});
    weak_index_text_buffer.assert_released();

    let uncommitted_diff = project
        .update(cx, |project, cx| {
            project.open_uncommitted_diff(buffer.clone(), cx)
        })
        .await
        .unwrap();
    cx.run_until_parked();
    uncommitted_diff.update(cx, |uncommitted_diff, cx| {
        assert_eq!(
            uncommitted_diff.base_text_string(cx).as_deref(),
            Some(committed_contents),
        );
        let snapshot = buffer.read(cx).snapshot();
        assert_hunks(
            uncommitted_diff.snapshot(cx).hunks(&snapshot),
            &snapshot,
            &uncommitted_diff.base_text_string(cx).unwrap(),
            &[(
                1..3,
                "two\nthree\n",
                "TWO\nTHREE\n",
                DiffHunkStatus::modified(DiffHunkSecondaryStatus::OverlapsWithSecondaryHunk),
            )],
        );
    });
}

#[gpui::test]
async fn test_staged_diff_without_unstaged_diff(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let committed_contents = "one\ntwo\nthree\n";
    let staged_contents = "one\nTWO\nthree\n";
    let file_contents = "one\nTWO\nthree\n";

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/dir",
        json!({
            ".git": {},
            "src": {
                "main.rs": file_contents,
            }
        }),
    )
    .await;
    fs.set_head_for_repo(
        Path::new("/dir/.git"),
        &[("src/main.rs", committed_contents.to_owned())],
        "deadbeef",
    );
    fs.set_index_for_repo(
        Path::new("/dir/.git"),
        &[("src/main.rs", staged_contents.to_owned())],
    );

    let project = Project::test(fs.clone(), ["/dir".as_ref()], cx).await;
    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer("/dir/src/main.rs", cx)
        })
        .await
        .unwrap();

    let staged_diff = project
        .update(cx, |project, cx| {
            project.open_staged_diff(buffer.clone(), cx)
        })
        .await
        .unwrap();
    cx.run_until_parked();

    staged_diff.update(cx, |staged_diff, cx| {
        let snapshot = staged_diff.snapshot(cx);
        let buffer_snapshot = snapshot.buffer_snapshot();
        assert_hunks(
            snapshot.hunks(buffer_snapshot),
            buffer_snapshot,
            &staged_diff.base_text_string(cx).unwrap(),
            &[(1..2, "two\n", "TWO\n", DiffHunkStatus::modified_none())],
        );
    });

    fs.set_index_for_repo(
        Path::new("/dir/.git"),
        &[("src/main.rs", "one\nTWO\nTHREE\n".to_owned())],
    );
    cx.run_until_parked();

    staged_diff.update(cx, |staged_diff, cx| {
        let snapshot = staged_diff.snapshot(cx);
        let buffer_snapshot = snapshot.buffer_snapshot();
        assert_hunks(
            snapshot.hunks(buffer_snapshot),
            buffer_snapshot,
            &staged_diff.base_text_string(cx).unwrap(),
            &[(
                1..3,
                "two\nthree\n",
                "TWO\nTHREE\n",
                DiffHunkStatus::modified_none(),
            )],
        );
    });
}

#[gpui::test]
async fn test_uncommitted_diff_for_buffer(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let committed_contents = r#"
        fn main() {
            println!("hello world");
        }
    "#
    .unindent();
    let staged_contents = r#"
        fn main() {
            println!("goodbye world");
        }
    "#
    .unindent();
    let file_contents = r#"
        // print goodbye
        fn main() {
            println!("goodbye world");
        }
    "#
    .unindent();

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/dir",
        json!({
            ".git": {},
           "src": {
               "modification.rs": file_contents,
           }
        }),
    )
    .await;

    fs.set_head_for_repo(
        Path::new("/dir/.git"),
        &[
            ("src/modification.rs", committed_contents),
            ("src/deletion.rs", "// the-deleted-contents\n".into()),
        ],
        "deadbeef",
    );
    fs.set_index_for_repo(
        Path::new("/dir/.git"),
        &[
            ("src/modification.rs", staged_contents),
            ("src/deletion.rs", "// the-deleted-contents\n".into()),
        ],
    );

    let project = Project::test(fs.clone(), ["/dir".as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    let language = rust_lang();
    language_registry.add(language.clone());

    let buffer_1 = project
        .update(cx, |project, cx| {
            project.open_local_buffer("/dir/src/modification.rs", cx)
        })
        .await
        .unwrap();
    let diff_1 = project
        .update(cx, |project, cx| {
            project.open_uncommitted_diff(buffer_1.clone(), cx)
        })
        .await
        .unwrap();
    diff_1.read_with(cx, |diff, cx| {
        assert_eq!(diff.base_text(cx).language().cloned(), Some(language))
    });
    cx.run_until_parked();
    diff_1.update(cx, |diff, cx| {
        let snapshot = buffer_1.read(cx).snapshot();
        assert_hunks(
            diff.snapshot(cx).hunks_intersecting_range(
                Anchor::min_max_range_for_buffer(snapshot.remote_id()),
                &snapshot,
            ),
            &snapshot,
            &diff.base_text_string(cx).unwrap(),
            &[
                (
                    0..1,
                    "",
                    "// print goodbye\n",
                    DiffHunkStatus::added(DiffHunkSecondaryStatus::HasSecondaryHunk),
                ),
                (
                    2..3,
                    "    println!(\"hello world\");\n",
                    "    println!(\"goodbye world\");\n",
                    DiffHunkStatus::modified_none(),
                ),
            ],
        );
    });

    // Reset HEAD to a version that differs from both the buffer and the index.
    let committed_contents = r#"
        // print goodbye
        fn main() {
        }
    "#
    .unindent();
    fs.set_head_for_repo(
        Path::new("/dir/.git"),
        &[
            ("src/modification.rs", committed_contents.clone()),
            ("src/deletion.rs", "// the-deleted-contents\n".into()),
        ],
        "deadbeef",
    );

    // Buffer now has an unstaged hunk.
    cx.run_until_parked();
    diff_1.update(cx, |diff, cx| {
        let snapshot = buffer_1.read(cx).snapshot();
        assert_hunks(
            diff.snapshot(cx).hunks_intersecting_range(
                Anchor::min_max_range_for_buffer(snapshot.remote_id()),
                &snapshot,
            ),
            &snapshot,
            &diff.base_text(cx).text(),
            &[(
                2..3,
                "",
                "    println!(\"goodbye world\");\n",
                DiffHunkStatus::added_none(),
            )],
        );
    });

    // Open a buffer for a file that's been deleted.
    let buffer_2 = project
        .update(cx, |project, cx| {
            project.open_local_buffer("/dir/src/deletion.rs", cx)
        })
        .await
        .unwrap();
    let diff_2 = project
        .update(cx, |project, cx| {
            project.open_uncommitted_diff(buffer_2.clone(), cx)
        })
        .await
        .unwrap();
    cx.run_until_parked();
    diff_2.update(cx, |diff, cx| {
        let snapshot = buffer_2.read(cx).snapshot();
        assert_hunks(
            diff.snapshot(cx).hunks_intersecting_range(
                Anchor::min_max_range_for_buffer(snapshot.remote_id()),
                &snapshot,
            ),
            &snapshot,
            &diff.base_text_string(cx).unwrap(),
            &[(
                0..0,
                "// the-deleted-contents\n",
                "",
                DiffHunkStatus::deleted(DiffHunkSecondaryStatus::HasSecondaryHunk),
            )],
        );
    });

    // Stage the deletion of this file
    fs.set_index_for_repo(
        Path::new("/dir/.git"),
        &[("src/modification.rs", committed_contents.clone())],
    );
    cx.run_until_parked();
    diff_2.update(cx, |diff, cx| {
        let snapshot = buffer_2.read(cx).snapshot();
        assert_hunks(
            diff.snapshot(cx).hunks_intersecting_range(
                Anchor::min_max_range_for_buffer(snapshot.remote_id()),
                &snapshot,
            ),
            &snapshot,
            &diff.base_text_string(cx).unwrap(),
            &[(
                0..0,
                "// the-deleted-contents\n",
                "",
                DiffHunkStatus::deleted(DiffHunkSecondaryStatus::NoSecondaryHunk),
            )],
        );
    });
}

#[gpui::test]
async fn test_staging_hunks(cx: &mut gpui::TestAppContext) {
    use DiffHunkSecondaryStatus::*;
    init_test(cx);

    let committed_contents = r#"
        zero
        one
        two
        three
        four
        five
    "#
    .unindent();
    let file_contents = r#"
        one
        TWO
        three
        FOUR
        five
    "#
    .unindent();

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/dir",
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

    let project = Project::test(fs.clone(), ["/dir".as_ref()], cx).await;

    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer("/dir/file.txt", cx)
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
    let mut diff_events = cx.events(&uncommitted_diff);

    // The hunks are initially unstaged.
    uncommitted_diff.read_with(cx, |diff, cx| {
        assert_hunks(
            diff.snapshot(cx).hunks(&snapshot),
            &snapshot,
            &diff.base_text_string(cx).unwrap(),
            &[
                (
                    0..0,
                    "zero\n",
                    "",
                    DiffHunkStatus::deleted(HasSecondaryHunk),
                ),
                (
                    1..2,
                    "two\n",
                    "TWO\n",
                    DiffHunkStatus::modified(HasSecondaryHunk),
                ),
                (
                    3..4,
                    "four\n",
                    "FOUR\n",
                    DiffHunkStatus::modified(HasSecondaryHunk),
                ),
            ],
        );
    });

    // Stage a hunk. It appears as optimistically staged.
    uncommitted_diff.update(cx, |diff, cx| {
        let range =
            snapshot.anchor_before(Point::new(1, 0))..snapshot.anchor_before(Point::new(2, 0));
        let hunks = diff
            .snapshot(cx)
            .hunks_intersecting_range(range, &snapshot)
            .collect::<Vec<_>>();
        diff.stage_or_unstage_hunks(true, &hunks, &snapshot, true, cx);

        assert_hunks(
            diff.snapshot(cx).hunks(&snapshot),
            &snapshot,
            &diff.base_text_string(cx).unwrap(),
            &[
                (
                    0..0,
                    "zero\n",
                    "",
                    DiffHunkStatus::deleted(HasSecondaryHunk),
                ),
                (
                    1..2,
                    "two\n",
                    "TWO\n",
                    DiffHunkStatus::modified(SecondaryHunkRemovalPending),
                ),
                (
                    3..4,
                    "four\n",
                    "FOUR\n",
                    DiffHunkStatus::modified(HasSecondaryHunk),
                ),
            ],
        );
    });

    // The diff emits a change event for the range of the staged hunk.
    assert!(matches!(
        diff_events.next().await.unwrap(),
        BufferDiffEvent::HunksStagedOrUnstaged(_)
    ));
    let event = diff_events.next().await.unwrap();
    if let BufferDiffEvent::DiffChanged(DiffChanged {
        changed_range: Some(changed_range),
        base_text_changed_range: _,
        extended_range: _,
        base_text_changed: _,
    }) = event
    {
        let changed_range = changed_range.to_point(&snapshot);
        assert_eq!(changed_range, Point::new(1, 0)..Point::new(2, 0));
    } else {
        panic!("Unexpected event {event:?}");
    }

    // When the write to the index completes, it appears as staged.
    cx.run_until_parked();
    uncommitted_diff.update(cx, |diff, cx| {
        assert_hunks(
            diff.snapshot(cx).hunks(&snapshot),
            &snapshot,
            &diff.base_text_string(cx).unwrap(),
            &[
                (
                    0..0,
                    "zero\n",
                    "",
                    DiffHunkStatus::deleted(HasSecondaryHunk),
                ),
                (
                    1..2,
                    "two\n",
                    "TWO\n",
                    DiffHunkStatus::modified(NoSecondaryHunk),
                ),
                (
                    3..4,
                    "four\n",
                    "FOUR\n",
                    DiffHunkStatus::modified(HasSecondaryHunk),
                ),
            ],
        );
    });

    // The diff emits a change event for the changed index text.
    let event = diff_events.next().await.unwrap();
    if let BufferDiffEvent::DiffChanged(DiffChanged {
        changed_range: Some(changed_range),
        base_text_changed_range: _,
        extended_range: _,
        base_text_changed: _,
    }) = event
    {
        let changed_range = changed_range.to_point(&snapshot);
        assert_eq!(changed_range, Point::new(1, 0)..Point::new(2, 0));
    } else {
        panic!("Unexpected event {event:?}");
    }

    // Simulate a problem writing to the git index.
    fs.set_error_message_for_index_write(
        "/dir/.git".as_ref(),
        Some("failed to write git index".into()),
    );

    // Stage another hunk.
    uncommitted_diff.update(cx, |diff, cx| {
        let range =
            snapshot.anchor_before(Point::new(3, 0))..snapshot.anchor_before(Point::new(4, 0));
        let hunks = diff
            .snapshot(cx)
            .hunks_intersecting_range(range, &snapshot)
            .collect::<Vec<_>>();
        diff.stage_or_unstage_hunks(true, &hunks, &snapshot, true, cx);

        assert_hunks(
            diff.snapshot(cx).hunks(&snapshot),
            &snapshot,
            &diff.base_text_string(cx).unwrap(),
            &[
                (
                    0..0,
                    "zero\n",
                    "",
                    DiffHunkStatus::deleted(HasSecondaryHunk),
                ),
                (
                    1..2,
                    "two\n",
                    "TWO\n",
                    DiffHunkStatus::modified(NoSecondaryHunk),
                ),
                (
                    3..4,
                    "four\n",
                    "FOUR\n",
                    DiffHunkStatus::modified(SecondaryHunkRemovalPending),
                ),
            ],
        );
    });
    assert!(matches!(
        diff_events.next().await.unwrap(),
        BufferDiffEvent::HunksStagedOrUnstaged(_)
    ));
    let event = diff_events.next().await.unwrap();
    if let BufferDiffEvent::DiffChanged(DiffChanged {
        changed_range: Some(changed_range),
        base_text_changed_range: _,
        extended_range: _,
        base_text_changed: _,
    }) = event
    {
        let changed_range = changed_range.to_point(&snapshot);
        assert_eq!(changed_range, Point::new(3, 0)..Point::new(4, 0));
    } else {
        panic!("Unexpected event {event:?}");
    }

    // When the write fails, the hunk returns to being unstaged.
    cx.run_until_parked();
    uncommitted_diff.update(cx, |diff, cx| {
        assert_hunks(
            diff.snapshot(cx).hunks(&snapshot),
            &snapshot,
            &diff.base_text_string(cx).unwrap(),
            &[
                (
                    0..0,
                    "zero\n",
                    "",
                    DiffHunkStatus::deleted(HasSecondaryHunk),
                ),
                (
                    1..2,
                    "two\n",
                    "TWO\n",
                    DiffHunkStatus::modified(NoSecondaryHunk),
                ),
                (
                    3..4,
                    "four\n",
                    "FOUR\n",
                    DiffHunkStatus::modified(HasSecondaryHunk),
                ),
            ],
        );
    });

    let event = diff_events.next().await.unwrap();
    if let BufferDiffEvent::DiffChanged(DiffChanged {
        changed_range: Some(changed_range),
        base_text_changed_range: _,
        extended_range: _,
        base_text_changed: _,
    }) = event
    {
        let changed_range = changed_range.to_point(&snapshot);
        assert_eq!(changed_range, Point::new(0, 0)..Point::new(5, 0));
    } else {
        panic!("Unexpected event {event:?}");
    }

    // Allow writing to the git index to succeed again.
    fs.set_error_message_for_index_write("/dir/.git".as_ref(), None);

    // Stage two hunks with separate operations.
    uncommitted_diff.update(cx, |diff, cx| {
        let hunks = diff.snapshot(cx).hunks(&snapshot).collect::<Vec<_>>();
        diff.stage_or_unstage_hunks(true, &hunks[0..1], &snapshot, true, cx);
        diff.stage_or_unstage_hunks(true, &hunks[2..3], &snapshot, true, cx);
    });

    // Both staged hunks appear as pending.
    uncommitted_diff.update(cx, |diff, cx| {
        assert_hunks(
            diff.snapshot(cx).hunks(&snapshot),
            &snapshot,
            &diff.base_text_string(cx).unwrap(),
            &[
                (
                    0..0,
                    "zero\n",
                    "",
                    DiffHunkStatus::deleted(SecondaryHunkRemovalPending),
                ),
                (
                    1..2,
                    "two\n",
                    "TWO\n",
                    DiffHunkStatus::modified(NoSecondaryHunk),
                ),
                (
                    3..4,
                    "four\n",
                    "FOUR\n",
                    DiffHunkStatus::modified(SecondaryHunkRemovalPending),
                ),
            ],
        );
    });

    // Both staging operations take effect.
    cx.run_until_parked();
    uncommitted_diff.update(cx, |diff, cx| {
        assert_hunks(
            diff.snapshot(cx).hunks(&snapshot),
            &snapshot,
            &diff.base_text_string(cx).unwrap(),
            &[
                (0..0, "zero\n", "", DiffHunkStatus::deleted(NoSecondaryHunk)),
                (
                    1..2,
                    "two\n",
                    "TWO\n",
                    DiffHunkStatus::modified(NoSecondaryHunk),
                ),
                (
                    3..4,
                    "four\n",
                    "FOUR\n",
                    DiffHunkStatus::modified(NoSecondaryHunk),
                ),
            ],
        );
    });
}

#[gpui::test(iterations = 10)]
async fn test_uncommitted_diff_opened_before_unstaged_diff(cx: &mut gpui::TestAppContext) {
    use DiffHunkSecondaryStatus::*;
    init_test(cx);

    let committed_contents = "one\ntwo\nthree\n";
    let file_contents = "one\nTWO\nthree\n";

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/dir",
        json!({
            ".git": {},
            "file.txt": file_contents,
        }),
    )
    .await;
    fs.set_head_and_index_for_repo(
        path!("/dir/.git").as_ref(),
        &[("file.txt", committed_contents.into())],
    );

    let project = Project::test(fs.clone(), ["/dir".as_ref()], cx).await;
    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer("/dir/file.txt", cx)
        })
        .await
        .unwrap();

    let uncommitted_diff_task = project.update(cx, |project, cx| {
        project.open_uncommitted_diff(buffer.clone(), cx)
    });
    let unstaged_diff_task = project.update(cx, |project, cx| {
        project.open_unstaged_diff(buffer.clone(), cx)
    });
    let (uncommitted_diff, unstaged_diff) =
        futures::future::join(uncommitted_diff_task, unstaged_diff_task).await;
    let uncommitted_diff = uncommitted_diff.unwrap();
    let unstaged_diff = unstaged_diff.unwrap();

    cx.run_until_parked();

    uncommitted_diff.read_with(cx, |diff, _| {
        assert_eq!(
            diff.secondary_diff(),
            Some(unstaged_diff.clone()),
            "the unstaged diff returned to callers should be the uncommitted diff's secondary"
        );
    });
    project.read_with(cx, |project, cx| {
        let buffer_id = buffer.read(cx).remote_id();
        assert_eq!(
            project
                .git_store()
                .read(cx)
                .get_unstaged_diff(buffer_id, cx),
            Some(unstaged_diff.clone()),
            "the unstaged diff returned to callers should be the registered one"
        );
    });

    uncommitted_diff.read_with(cx, |diff, cx| {
        let snapshot = buffer.read(cx).snapshot();
        assert_hunks(
            diff.snapshot(cx).hunks_intersecting_range(
                Anchor::min_max_range_for_buffer(snapshot.remote_id()),
                &snapshot,
            ),
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
}

#[gpui::test(seeds(340, 472))]
async fn test_staging_hunks_with_delayed_fs_event(cx: &mut gpui::TestAppContext) {
    use DiffHunkSecondaryStatus::*;
    init_test(cx);

    let committed_contents = r#"
        zero
        one
        two
        three
        four
        five
    "#
    .unindent();
    let file_contents = r#"
        one
        TWO
        three
        FOUR
        five
    "#
    .unindent();

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/dir",
        json!({
            ".git": {},
            "file.txt": file_contents.clone()
        }),
    )
    .await;

    fs.set_head_for_repo(
        "/dir/.git".as_ref(),
        &[("file.txt", committed_contents.clone())],
        "deadbeef",
    );
    fs.set_index_for_repo(
        "/dir/.git".as_ref(),
        &[("file.txt", committed_contents.clone())],
    );

    let project = Project::test(fs.clone(), ["/dir".as_ref()], cx).await;

    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer("/dir/file.txt", cx)
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

    // The hunks are initially unstaged.
    uncommitted_diff.read_with(cx, |diff, cx| {
        assert_hunks(
            diff.snapshot(cx).hunks(&snapshot),
            &snapshot,
            &diff.base_text_string(cx).unwrap(),
            &[
                (
                    0..0,
                    "zero\n",
                    "",
                    DiffHunkStatus::deleted(HasSecondaryHunk),
                ),
                (
                    1..2,
                    "two\n",
                    "TWO\n",
                    DiffHunkStatus::modified(HasSecondaryHunk),
                ),
                (
                    3..4,
                    "four\n",
                    "FOUR\n",
                    DiffHunkStatus::modified(HasSecondaryHunk),
                ),
            ],
        );
    });

    // Pause IO events
    fs.pause_events();

    // Stage the first hunk.
    uncommitted_diff.update(cx, |diff, cx| {
        let hunk = diff.snapshot(cx).hunks(&snapshot).next().unwrap();
        diff.stage_or_unstage_hunks(true, &[hunk], &snapshot, true, cx);
        assert_hunks(
            diff.snapshot(cx).hunks(&snapshot),
            &snapshot,
            &diff.base_text_string(cx).unwrap(),
            &[
                (
                    0..0,
                    "zero\n",
                    "",
                    DiffHunkStatus::deleted(SecondaryHunkRemovalPending),
                ),
                (
                    1..2,
                    "two\n",
                    "TWO\n",
                    DiffHunkStatus::modified(HasSecondaryHunk),
                ),
                (
                    3..4,
                    "four\n",
                    "FOUR\n",
                    DiffHunkStatus::modified(HasSecondaryHunk),
                ),
            ],
        );
    });

    // Stage the second hunk *before* receiving the FS event for the first hunk.
    cx.run_until_parked();
    uncommitted_diff.update(cx, |diff, cx| {
        let hunk = diff.snapshot(cx).hunks(&snapshot).nth(1).unwrap();
        diff.stage_or_unstage_hunks(true, &[hunk], &snapshot, true, cx);
        assert_hunks(
            diff.snapshot(cx).hunks(&snapshot),
            &snapshot,
            &diff.base_text_string(cx).unwrap(),
            &[
                (
                    0..0,
                    "zero\n",
                    "",
                    DiffHunkStatus::deleted(SecondaryHunkRemovalPending),
                ),
                (
                    1..2,
                    "two\n",
                    "TWO\n",
                    DiffHunkStatus::modified(SecondaryHunkRemovalPending),
                ),
                (
                    3..4,
                    "four\n",
                    "FOUR\n",
                    DiffHunkStatus::modified(HasSecondaryHunk),
                ),
            ],
        );
    });

    // Process the FS event for staging the first hunk (second event is still pending).
    fs.flush_events(1);
    cx.run_until_parked();

    // Stage the third hunk before receiving the second FS event.
    uncommitted_diff.update(cx, |diff, cx| {
        let hunk = diff.snapshot(cx).hunks(&snapshot).nth(2).unwrap();
        diff.stage_or_unstage_hunks(true, &[hunk], &snapshot, true, cx);
    });

    // Wait for all remaining IO.
    cx.run_until_parked();
    fs.flush_events(fs.buffered_event_count());

    // Now all hunks are staged.
    cx.run_until_parked();
    uncommitted_diff.update(cx, |diff, cx| {
        assert_hunks(
            diff.snapshot(cx).hunks(&snapshot),
            &snapshot,
            &diff.base_text_string(cx).unwrap(),
            &[
                (0..0, "zero\n", "", DiffHunkStatus::deleted(NoSecondaryHunk)),
                (
                    1..2,
                    "two\n",
                    "TWO\n",
                    DiffHunkStatus::modified(NoSecondaryHunk),
                ),
                (
                    3..4,
                    "four\n",
                    "FOUR\n",
                    DiffHunkStatus::modified(NoSecondaryHunk),
                ),
            ],
        );
    });
}

#[gpui::test(iterations = 25)]
async fn test_staging_random_hunks(
    mut rng: StdRng,
    _executor: BackgroundExecutor,
    cx: &mut gpui::TestAppContext,
) {
    let operations = env::var("OPERATIONS")
        .map(|i| i.parse().expect("invalid `OPERATIONS` variable"))
        .unwrap_or(20);

    use DiffHunkSecondaryStatus::*;
    init_test(cx);

    let committed_text = (0..30).map(|i| format!("line {i}\n")).collect::<String>();
    let index_text = committed_text.clone();
    let buffer_text = (0..30)
        .map(|i| match i % 5 {
            0 => format!("line {i} (modified)\n"),
            _ => format!("line {i}\n"),
        })
        .collect::<String>();

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/dir"),
        json!({
            ".git": {},
            "file.txt": buffer_text.clone()
        }),
    )
    .await;
    fs.set_head_for_repo(
        path!("/dir/.git").as_ref(),
        &[("file.txt", committed_text.clone())],
        "deadbeef",
    );
    fs.set_index_for_repo(
        path!("/dir/.git").as_ref(),
        &[("file.txt", index_text.clone())],
    );
    let repo = fs
        .open_repo(path!("/dir/.git").as_ref(), Some("git".as_ref()))
        .unwrap();

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

    let mut hunks = uncommitted_diff.update(cx, |diff, cx| {
        diff.snapshot(cx).hunks(&snapshot).collect::<Vec<_>>()
    });
    assert_eq!(hunks.len(), 6);

    for _i in 0..operations {
        let hunk_ix = rng.random_range(0..hunks.len());
        let hunk = &mut hunks[hunk_ix];
        let row = hunk.range.start.row;

        if hunk.status().has_secondary_hunk() {
            log::info!("staging hunk at {row}");
            uncommitted_diff.update(cx, |diff, cx| {
                diff.stage_or_unstage_hunks(true, std::slice::from_ref(hunk), &snapshot, true, cx);
            });
            hunk.secondary_status = SecondaryHunkRemovalPending;
        } else {
            log::info!("unstaging hunk at {row}");
            uncommitted_diff.update(cx, |diff, cx| {
                diff.stage_or_unstage_hunks(false, std::slice::from_ref(hunk), &snapshot, true, cx);
            });
            hunk.secondary_status = SecondaryHunkAdditionPending;
        }

        for _ in 0..rng.random_range(0..10) {
            log::info!("yielding");
            cx.executor().simulate_random_delay().await;
        }
    }

    cx.executor().run_until_parked();

    for hunk in &mut hunks {
        if hunk.secondary_status == SecondaryHunkRemovalPending {
            hunk.secondary_status = NoSecondaryHunk;
        } else if hunk.secondary_status == SecondaryHunkAdditionPending {
            hunk.secondary_status = HasSecondaryHunk;
        }
    }

    log::info!(
        "index text:\n{}",
        repo.load_index_text(RepoPath::from_rel_path(rel_path("file.txt")))
            .await
            .unwrap()
    );

    uncommitted_diff.update(cx, |diff, cx| {
        let expected_hunks = hunks
            .iter()
            .map(|hunk| (hunk.range.start.row, hunk.secondary_status))
            .collect::<Vec<_>>();
        let actual_hunks = diff
            .snapshot(cx)
            .hunks(&snapshot)
            .map(|hunk| (hunk.range.start.row, hunk.secondary_status))
            .collect::<Vec<_>>();
        assert_eq!(actual_hunks, expected_hunks);
    });
}

#[gpui::test]
async fn test_single_file_diffs(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let committed_contents = r#"
        fn main() {
            println!("hello from HEAD");
        }
    "#
    .unindent();
    let file_contents = r#"
        fn main() {
            println!("hello from the working copy");
        }
    "#
    .unindent();

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/dir",
        json!({
            ".git": {},
           "src": {
               "main.rs": file_contents,
           }
        }),
    )
    .await;

    fs.set_head_for_repo(
        Path::new("/dir/.git"),
        &[("src/main.rs", committed_contents.clone())],
        "deadbeef",
    );
    fs.set_index_for_repo(
        Path::new("/dir/.git"),
        &[("src/main.rs", committed_contents.clone())],
    );

    let project = Project::test(fs.clone(), ["/dir/src/main.rs".as_ref()], cx).await;

    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer("/dir/src/main.rs", cx)
        })
        .await
        .unwrap();
    let uncommitted_diff = project
        .update(cx, |project, cx| {
            project.open_uncommitted_diff(buffer.clone(), cx)
        })
        .await
        .unwrap();

    cx.run_until_parked();
    uncommitted_diff.update(cx, |uncommitted_diff, cx| {
        let snapshot = buffer.read(cx).snapshot();
        assert_hunks(
            uncommitted_diff.snapshot(cx).hunks(&snapshot),
            &snapshot,
            &uncommitted_diff.base_text_string(cx).unwrap(),
            &[(
                1..2,
                "    println!(\"hello from HEAD\");\n",
                "    println!(\"hello from the working copy\");\n",
                DiffHunkStatus {
                    kind: DiffHunkStatusKind::Modified,
                    secondary: DiffHunkSecondaryStatus::HasSecondaryHunk,
                },
            )],
        );
    });
}

// TODO: Should we test this on Windows also?
#[gpui::test]
#[cfg(not(windows))]
async fn test_staging_hunk_preserve_executable_permission(cx: &mut gpui::TestAppContext) {
    use std::os::unix::fs::PermissionsExt;
    init_test(cx);
    cx.executor().allow_parking();
    let committed_contents = "bar\n";
    let file_contents = "baz\n";
    let root = TempTree::new(json!({
        "project": {
            "foo": committed_contents
        },
    }));

    let work_dir = root.path().join("project");
    let file_path = work_dir.join("foo");
    let repo = git_init(work_dir.as_path());
    let mut perms = std::fs::metadata(&file_path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&file_path, perms).unwrap();
    git_add("foo", &repo);
    git_commit("Initial commit", &repo);
    std::fs::write(&file_path, file_contents).unwrap();

    let project = Project::test(
        Arc::new(RealFs::new(None, cx.executor())),
        [root.path()],
        cx,
    )
    .await;

    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(file_path.as_path(), cx)
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

    uncommitted_diff.update(cx, |diff, cx| {
        let hunks = diff.snapshot(cx).hunks(&snapshot).collect::<Vec<_>>();
        diff.stage_or_unstage_hunks(true, &hunks, &snapshot, true, cx);
    });

    cx.run_until_parked();

    let output = smol::process::Command::new("git")
        .current_dir(&work_dir)
        .args(["diff", "--staged"])
        .output()
        .await
        .unwrap();

    let staged_diff = String::from_utf8_lossy(&output.stdout);

    assert!(
        !staged_diff.contains("new mode 100644"),
        "Staging should not change file mode from 755 to 644.\ngit diff --staged:\n{}",
        staged_diff
    );

    let output = smol::process::Command::new("git")
        .current_dir(&work_dir)
        .args(["ls-files", "-s"])
        .output()
        .await
        .unwrap();
    let index_contents = String::from_utf8_lossy(&output.stdout);

    assert!(
        index_contents.contains("100755"),
        "Index should show file as executable (100755).\ngit ls-files -s:\n{}",
        index_contents
    );
}

#[gpui::test]
async fn test_repository_and_path_for_project_path(
    background_executor: BackgroundExecutor,
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx);
    let fs = FakeFs::new(background_executor);
    fs.insert_tree(
        path!("/root"),
        json!({
            "c.txt": "",
            "dir1": {
                ".git": {},
                "deps": {
                    "dep1": {
                        ".git": {},
                        "src": {
                            "a.txt": ""
                        }
                    }
                },
                "src": {
                    "b.txt": ""
                }
            },
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let tree = project.read_with(cx, |project, cx| project.worktrees(cx).next().unwrap());
    let tree_id = tree.read_with(cx, |tree, _| tree.id());
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.run_until_parked();

    project.read_with(cx, |project, cx| {
        let git_store = project.git_store().read(cx);
        let pairs = [
            ("c.txt", None),
            ("dir1/src/b.txt", Some((path!("/root/dir1"), "src/b.txt"))),
            (
                "dir1/deps/dep1/src/a.txt",
                Some((path!("/root/dir1/deps/dep1"), "src/a.txt")),
            ),
        ];
        let expected = pairs
            .iter()
            .map(|(path, result)| {
                (
                    path,
                    result.map(|(repo, repo_path)| {
                        (Path::new(repo).into(), RepoPath::new(repo_path).unwrap())
                    }),
                )
            })
            .collect::<Vec<_>>();
        let actual = pairs
            .iter()
            .map(|(path, _)| {
                let project_path = (tree_id, rel_path(path)).into();
                let result = maybe!({
                    let (repo, repo_path) =
                        git_store.repository_and_path_for_project_path(&project_path, cx)?;
                    Some((repo.read(cx).work_directory_abs_path.clone(), repo_path))
                });
                (path, result)
            })
            .collect::<Vec<_>>();
        pretty_assertions::assert_eq!(expected, actual);
    });

    fs.remove_dir(path!("/root/dir1/.git").as_ref(), RemoveOptions::default())
        .await
        .unwrap();
    cx.run_until_parked();

    project.read_with(cx, |project, cx| {
        let git_store = project.git_store().read(cx);
        assert_eq!(
            git_store.repository_and_path_for_project_path(
                &(tree_id, rel_path("dir1/src/b.txt")).into(),
                cx
            ),
            None
        );
    });
}

#[gpui::test]
async fn test_home_dir_as_git_repository(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    let home = paths::home_dir();
    fs.insert_tree(
        home,
        json!({
            ".git": {},
            "project": {
                "a.txt": "A"
            },
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [home.join("project").as_ref()], cx).await;
    let tree = project.read_with(cx, |project, cx| project.worktrees(cx).next().unwrap());
    let tree_id = tree.read_with(cx, |tree, _| tree.id());

    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    tree.flush_fs_events(cx).await;

    project.read_with(cx, |project, cx| {
        let containing = project
            .git_store()
            .read(cx)
            .repository_and_path_for_project_path(&(tree_id, rel_path("a.txt")).into(), cx);
        assert!(containing.is_none());
    });

    let project = Project::test(fs.clone(), [home.as_ref()], cx).await;
    let tree = project.read_with(cx, |project, cx| project.worktrees(cx).next().unwrap());
    let tree_id = tree.read_with(cx, |tree, _| tree.id());
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    tree.flush_fs_events(cx).await;

    project.read_with(cx, |project, cx| {
        let containing = project
            .git_store()
            .read(cx)
            .repository_and_path_for_project_path(&(tree_id, rel_path("project/a.txt")).into(), cx);
        assert_eq!(
            containing
                .unwrap()
                .0
                .read(cx)
                .work_directory_abs_path
                .as_ref(),
            home,
        );
    });
}

#[gpui::test]
async fn test_git_repository_status(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();

    let root = TempTree::new(json!({
        "project": {
            "a.txt": "a",    // Modified
            "b.txt": "bb",   // Added
            "c.txt": "ccc",  // Unchanged
            "d.txt": "dddd", // Deleted
        },
    }));

    // Set up git repository before creating the project.
    let work_dir = root.path().join("project");
    let repo = git_init(work_dir.as_path());
    git_add("a.txt", &repo);
    git_add("c.txt", &repo);
    git_add("d.txt", &repo);
    git_commit("Initial commit", &repo);
    std::fs::remove_file(work_dir.join("d.txt")).unwrap();
    std::fs::write(work_dir.join("a.txt"), "aa").unwrap();

    let project = Project::test(
        Arc::new(RealFs::new(None, cx.executor())),
        [root.path()],
        cx,
    )
    .await;

    let tree = project.read_with(cx, |project, cx| project.worktrees(cx).next().unwrap());
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    let repository = project.read_with(cx, |project, cx| {
        project.repositories(cx).values().next().unwrap().clone()
    });

    // Check that the right git state is observed on startup
    repository.read_with(cx, |repository, _| {
        let entries = repository.cached_status().collect::<Vec<_>>();
        assert_eq!(
            entries,
            [
                StatusEntry {
                    repo_path: repo_path("a.txt"),
                    status: StatusCode::Modified.worktree(),
                    diff_stat: Some(DiffStat {
                        added: 1,
                        deleted: 1,
                    }),
                },
                StatusEntry {
                    repo_path: repo_path("b.txt"),
                    status: FileStatus::Untracked,
                    diff_stat: None,
                },
                StatusEntry {
                    repo_path: repo_path("d.txt"),
                    status: StatusCode::Deleted.worktree(),
                    diff_stat: Some(DiffStat {
                        added: 0,
                        deleted: 1,
                    }),
                },
            ]
        );
    });

    std::fs::write(work_dir.join("c.txt"), "some changes").unwrap();

    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    repository.read_with(cx, |repository, _| {
        let entries = repository.cached_status().collect::<Vec<_>>();
        assert_eq!(
            entries,
            [
                StatusEntry {
                    repo_path: repo_path("a.txt"),
                    status: StatusCode::Modified.worktree(),
                    diff_stat: Some(DiffStat {
                        added: 1,
                        deleted: 1,
                    }),
                },
                StatusEntry {
                    repo_path: repo_path("b.txt"),
                    status: FileStatus::Untracked,
                    diff_stat: None,
                },
                StatusEntry {
                    repo_path: repo_path("c.txt"),
                    status: StatusCode::Modified.worktree(),
                    diff_stat: Some(DiffStat {
                        added: 1,
                        deleted: 1,
                    }),
                },
                StatusEntry {
                    repo_path: repo_path("d.txt"),
                    status: StatusCode::Deleted.worktree(),
                    diff_stat: Some(DiffStat {
                        added: 0,
                        deleted: 1,
                    }),
                },
            ]
        );
    });

    git_add("a.txt", &repo);
    git_add("c.txt", &repo);
    git_remove_index(Path::new("d.txt"), &repo);
    git_commit("Another commit", &repo);
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    std::fs::remove_file(work_dir.join("a.txt")).unwrap();
    std::fs::remove_file(work_dir.join("b.txt")).unwrap();
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    repository.read_with(cx, |repository, _cx| {
        let entries = repository.cached_status().collect::<Vec<_>>();

        // Deleting an untracked entry, b.txt, should leave no status
        // a.txt was tracked, and so should have a status
        assert_eq!(
            entries,
            [StatusEntry {
                repo_path: repo_path("a.txt"),
                status: StatusCode::Deleted.worktree(),
                diff_stat: Some(DiffStat {
                    added: 0,
                    deleted: 1,
                }),
            }]
        );
    });
}

#[gpui::test]
async fn test_git_repository_status_removes_directory_descendants(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/root"),
        json!({
            "project": {
                ".git": {},
                "ci2": {
                    "Dockerfile.namespace": "untracked",
                },
            },
        }),
    )
    .await;
    fs.set_status_for_repo(
        path!("/root/project/.git").as_ref(),
        &[("ci2/Dockerfile.namespace", FileStatus::Untracked)],
    );

    let project = Project::test(fs.clone(), [path!("/root/project").as_ref()], cx).await;

    let tree = project.read_with(cx, |project, cx| project.worktrees(cx).next().unwrap());
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    let repository = project.read_with(cx, |project, cx| {
        project.repositories(cx).values().next().unwrap().clone()
    });

    repository.read_with(cx, |repository, _| {
        assert_eq!(
            repository.cached_status().collect::<Vec<_>>(),
            [StatusEntry {
                repo_path: repo_path("ci2/Dockerfile.namespace"),
                status: FileStatus::Untracked,
                diff_stat: None,
            }]
        );
    });

    fs.pause_events();
    fs.create_dir(path!("/root/project/ci3").as_ref())
        .await
        .unwrap();
    fs.copy_file(
        path!("/root/project/ci2/Dockerfile.namespace").as_ref(),
        path!("/root/project/ci3/Dockerfile.namespace").as_ref(),
        Default::default(),
    )
    .await
    .unwrap();
    fs.remove_dir(
        path!("/root/project/ci2").as_ref(),
        RemoveOptions {
            recursive: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    fs.clear_buffered_events();
    fs.unpause_events_and_flush();
    fs.emit_fs_event(path!("/root/project/ci2"), Some(PathEventKind::Removed));
    fs.emit_fs_event(path!("/root/project/ci3"), Some(PathEventKind::Created));

    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    repository.read_with(cx, |repository, _| {
        assert_eq!(
            repository.cached_status().collect::<Vec<_>>(),
            [StatusEntry {
                repo_path: repo_path("ci3/Dockerfile.namespace"),
                status: FileStatus::Untracked,
                diff_stat: None,
            }]
        );
    });
}

#[cfg(target_os = "linux")]
#[gpui::test(retries = 5)]
async fn test_git_events_after_project_excludes_dot_git(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();

    cx.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.worktree.file_scan_exclusions = Some(vec!["foo".to_string()]);
            });
        });
    });

    let root = TempTree::new(json!({
        "project": {
            "a.txt": "a",
        },
    }));

    let work_dir = root.path().join("project");
    let repo = git_init(&work_dir);
    git_add("a.txt", &repo);
    git_commit("Initial commit", &repo);
    git_branch("other-branch", &repo);

    let project = Project::test(
        Arc::new(RealFs::new(None, cx.executor())),
        [work_dir.as_path()],
        cx,
    )
    .await;

    let tree = project.read_with(cx, |project, cx| project.worktrees(cx).next().unwrap());
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    let repository = project.read_with(cx, |project, cx| {
        project.repositories(cx).values().next().unwrap().clone()
    });
    let branch = repository.read_with(cx, |repository, _| {
        repository
            .snapshot()
            .branch
            .as_ref()
            .map(|branch| branch.ref_name.to_string())
    });
    assert_eq!(branch.as_deref(), Some("refs/heads/main"));

    let worktree_id = tree.read_with(cx, |tree, _| tree.id());
    cx.update_global::<SettingsStore, _>(|store, cx| {
        store
            .set_local_settings(
                worktree_id,
                LocalSettingsPath::InWorktree(Arc::from(RelPath::empty())),
                LocalSettingsKind::Settings,
                Some(r#"{ "file_scan_exclusions": ["**/.git"] }"#),
                cx,
            )
            .unwrap();
    });
    cx.read(|cx| tree.read(cx).as_local().unwrap().scan_complete())
        .await;
    cx.executor().run_until_parked();

    cx.update(|cx| {
        assert!(tree.read(cx).entry_for_path(rel_path(".git")).is_none());
    });

    git_checkout("other-branch", &repo);

    let mut events = cx.events::<RepositoryEvent, _>(&repository);
    let timeout = futures::FutureExt::fuse(cx.background_executor.timer(Duration::from_secs(5)));
    futures::pin_mut!(timeout);
    loop {
        let branch = repository.read_with(cx, |repository, _| {
            repository
                .snapshot()
                .branch
                .as_ref()
                .map(|branch| branch.ref_name.to_string())
        });
        if branch.as_deref() == Some("refs/heads/other-branch") {
            break;
        }

        futures::select_biased! {
            _ = events.next() => {}
            _ = timeout => panic!("timed out waiting for repository HEAD update after .git was excluded"),
        }
    }
}

#[gpui::test]
#[ignore]
async fn test_git_status_postprocessing(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();

    let root = TempTree::new(json!({
        "project": {
            "sub": {},
            "a.txt": "",
        },
    }));

    let work_dir = root.path().join("project");
    let repo = git_init(work_dir.as_path());
    // a.txt exists in HEAD and the working copy but is deleted in the index.
    git_add("a.txt", &repo);
    git_commit("Initial commit", &repo);
    git_remove_index("a.txt".as_ref(), &repo);
    // `sub` is a nested git repository.
    let _sub = git_init(&work_dir.join("sub"));

    let project = Project::test(
        Arc::new(RealFs::new(None, cx.executor())),
        [root.path()],
        cx,
    )
    .await;

    let tree = project.read_with(cx, |project, cx| project.worktrees(cx).next().unwrap());
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    let repository = project.read_with(cx, |project, cx| {
        project
            .repositories(cx)
            .values()
            .find(|repo| repo.read(cx).work_directory_abs_path.ends_with("project"))
            .unwrap()
            .clone()
    });

    repository.read_with(cx, |repository, _cx| {
        let entries = repository.cached_status().collect::<Vec<_>>();

        // `sub` doesn't appear in our computed statuses.
        // a.txt appears with a combined `DA` status.
        assert_eq!(
            entries,
            [StatusEntry {
                repo_path: repo_path("a.txt"),
                status: TrackedStatus {
                    index_status: StatusCode::Deleted,
                    worktree_status: StatusCode::Added
                }
                .into(),
                diff_stat: None,
            }]
        )
    });
}

#[track_caller]
/// We merge lhs into rhs.
fn merge_pending_ops_snapshots(
    source: Vec<pending_op::PendingOps>,
    mut target: Vec<pending_op::PendingOps>,
) -> Vec<pending_op::PendingOps> {
    for s_ops in source {
        if let Some(idx) = target.iter().zip(0..).find_map(|(ops, idx)| {
            if ops.repo_path == s_ops.repo_path {
                Some(idx)
            } else {
                None
            }
        }) {
            let t_ops = &mut target[idx];
            for s_op in s_ops.ops {
                if let Some(op_idx) = t_ops
                    .ops
                    .iter()
                    .zip(0..)
                    .find_map(|(op, idx)| if op.id == s_op.id { Some(idx) } else { None })
                {
                    let t_op = &mut t_ops.ops[op_idx];
                    match (s_op.job_status, t_op.job_status) {
                        (pending_op::JobStatus::Running, _) => {}
                        (s_st, pending_op::JobStatus::Running) => t_op.job_status = s_st,
                        (s_st, t_st) if s_st == t_st => {}
                        _ => unreachable!(),
                    }
                } else {
                    t_ops.ops.push(s_op);
                }
            }
            t_ops.ops.sort_by_key(|op| op.id);
        } else {
            target.push(s_ops);
        }
    }
    target
}

#[gpui::test]
async fn test_repository_pending_ops_staging(
    executor: gpui::BackgroundExecutor,
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(executor);
    fs.insert_tree(
        path!("/root"),
        json!({
            "my-repo": {
                ".git": {},
                "a.txt": "a",
            }

        }),
    )
    .await;

    fs.set_status_for_repo(
        path!("/root/my-repo/.git").as_ref(),
        &[("a.txt", FileStatus::Untracked)],
    );

    let project = Project::test(fs.clone(), [path!("/root/my-repo").as_ref()], cx).await;
    let pending_ops_all = Arc::new(Mutex::new(SumTree::default()));
    project.update(cx, |project, cx| {
        let pending_ops_all = pending_ops_all.clone();
        cx.subscribe(project.git_store(), move |_, _, e, _| {
            if let GitStoreEvent::RepositoryUpdated(
                _,
                RepositoryEvent::PendingOpsChanged { pending_ops },
                _,
            ) = e
            {
                let merged = merge_pending_ops_snapshots(
                    pending_ops.items(()),
                    pending_ops_all.lock().items(()),
                );
                *pending_ops_all.lock() = SumTree::from_iter(merged.into_iter(), ());
            }
        })
        .detach();
    });
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;

    let repo = project.read_with(cx, |project, cx| {
        project.repositories(cx).values().next().unwrap().clone()
    });

    // Ensure we have no pending ops for any of the untracked files
    repo.read_with(cx, |repo, _cx| {
        assert!(repo.pending_ops().next().is_none());
    });

    let mut id = 1u16;

    let mut assert_stage = async |path: RepoPath, stage| {
        let git_status = if stage {
            pending_op::GitStatus::Staged
        } else {
            pending_op::GitStatus::Unstaged
        };
        repo.update(cx, |repo, cx| {
            let task = if stage {
                repo.stage_entries(vec![path.clone()], cx)
            } else {
                repo.unstage_entries(vec![path.clone()], cx)
            };
            let ops = repo.pending_ops_for_path(&path).unwrap();
            assert_eq!(
                ops.ops.last(),
                Some(&pending_op::PendingOp {
                    id: id.into(),
                    git_status,
                    job_status: pending_op::JobStatus::Running
                })
            );
            task
        })
        .await
        .unwrap();

        repo.read_with(cx, |repo, _cx| {
            let ops = repo.pending_ops_for_path(&path).unwrap();
            assert_eq!(
                ops.ops.last(),
                Some(&pending_op::PendingOp {
                    id: id.into(),
                    git_status,
                    job_status: pending_op::JobStatus::Finished
                })
            );
        });

        id += 1;
    };

    assert_stage(repo_path("a.txt"), true).await;
    assert_stage(repo_path("a.txt"), false).await;
    assert_stage(repo_path("a.txt"), true).await;
    assert_stage(repo_path("a.txt"), false).await;
    assert_stage(repo_path("a.txt"), true).await;

    cx.run_until_parked();

    assert_eq!(
        pending_ops_all
            .lock()
            .get(&worktree::PathKey(repo_path("a.txt").as_ref().clone()), ())
            .unwrap()
            .ops,
        vec![
            pending_op::PendingOp {
                id: 1u16.into(),
                git_status: pending_op::GitStatus::Staged,
                job_status: pending_op::JobStatus::Finished
            },
            pending_op::PendingOp {
                id: 2u16.into(),
                git_status: pending_op::GitStatus::Unstaged,
                job_status: pending_op::JobStatus::Finished
            },
            pending_op::PendingOp {
                id: 3u16.into(),
                git_status: pending_op::GitStatus::Staged,
                job_status: pending_op::JobStatus::Finished
            },
            pending_op::PendingOp {
                id: 4u16.into(),
                git_status: pending_op::GitStatus::Unstaged,
                job_status: pending_op::JobStatus::Finished
            },
            pending_op::PendingOp {
                id: 5u16.into(),
                git_status: pending_op::GitStatus::Staged,
                job_status: pending_op::JobStatus::Finished
            }
        ],
    );

    repo.update(cx, |repo, _cx| {
        let git_statuses = repo.cached_status().collect::<Vec<_>>();

        assert_eq!(
            git_statuses,
            [StatusEntry {
                repo_path: repo_path("a.txt"),
                status: TrackedStatus {
                    index_status: StatusCode::Added,
                    worktree_status: StatusCode::Unmodified
                }
                .into(),
                diff_stat: Some(DiffStat {
                    added: 1,
                    deleted: 0,
                }),
            }]
        );
    });
}

#[gpui::test]
async fn test_repository_pending_ops_long_running_staging(
    executor: gpui::BackgroundExecutor,
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(executor);
    fs.insert_tree(
        path!("/root"),
        json!({
            "my-repo": {
                ".git": {},
                "a.txt": "a",
            }

        }),
    )
    .await;

    fs.set_status_for_repo(
        path!("/root/my-repo/.git").as_ref(),
        &[("a.txt", FileStatus::Untracked)],
    );

    let project = Project::test(fs.clone(), [path!("/root/my-repo").as_ref()], cx).await;
    let pending_ops_all = Arc::new(Mutex::new(SumTree::default()));
    project.update(cx, |project, cx| {
        let pending_ops_all = pending_ops_all.clone();
        cx.subscribe(project.git_store(), move |_, _, e, _| {
            if let GitStoreEvent::RepositoryUpdated(
                _,
                RepositoryEvent::PendingOpsChanged { pending_ops },
                _,
            ) = e
            {
                let merged = merge_pending_ops_snapshots(
                    pending_ops.items(()),
                    pending_ops_all.lock().items(()),
                );
                *pending_ops_all.lock() = SumTree::from_iter(merged.into_iter(), ());
            }
        })
        .detach();
    });

    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;

    let repo = project.read_with(cx, |project, cx| {
        project.repositories(cx).values().next().unwrap().clone()
    });

    repo.update(cx, |repo, cx| {
        repo.stage_entries(vec![repo_path("a.txt")], cx)
    })
    .detach();

    repo.update(cx, |repo, cx| {
        repo.stage_entries(vec![repo_path("a.txt")], cx)
    })
    .unwrap()
    .with_timeout(Duration::from_secs(1), &cx.executor())
    .await
    .unwrap();

    cx.run_until_parked();

    assert_eq!(
        pending_ops_all
            .lock()
            .get(&worktree::PathKey(repo_path("a.txt").as_ref().clone()), ())
            .unwrap()
            .ops,
        vec![
            pending_op::PendingOp {
                id: 1u16.into(),
                git_status: pending_op::GitStatus::Staged,
                job_status: pending_op::JobStatus::Skipped
            },
            pending_op::PendingOp {
                id: 2u16.into(),
                git_status: pending_op::GitStatus::Staged,
                job_status: pending_op::JobStatus::Finished
            }
        ],
    );

    repo.update(cx, |repo, _cx| {
        let git_statuses = repo.cached_status().collect::<Vec<_>>();

        assert_eq!(
            git_statuses,
            [StatusEntry {
                repo_path: repo_path("a.txt"),
                status: TrackedStatus {
                    index_status: StatusCode::Added,
                    worktree_status: StatusCode::Unmodified
                }
                .into(),
                diff_stat: Some(DiffStat {
                    added: 1,
                    deleted: 0,
                }),
            }]
        );
    });
}

#[gpui::test]
async fn test_repository_pending_ops_stage_all(
    executor: gpui::BackgroundExecutor,
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(executor);
    fs.insert_tree(
        path!("/root"),
        json!({
            "my-repo": {
                ".git": {},
                "a.txt": "a",
                "b.txt": "b"
            }

        }),
    )
    .await;

    fs.set_status_for_repo(
        path!("/root/my-repo/.git").as_ref(),
        &[
            ("a.txt", FileStatus::Untracked),
            ("b.txt", FileStatus::Untracked),
        ],
    );

    let project = Project::test(fs.clone(), [path!("/root/my-repo").as_ref()], cx).await;
    let pending_ops_all = Arc::new(Mutex::new(SumTree::default()));
    project.update(cx, |project, cx| {
        let pending_ops_all = pending_ops_all.clone();
        cx.subscribe(project.git_store(), move |_, _, e, _| {
            if let GitStoreEvent::RepositoryUpdated(
                _,
                RepositoryEvent::PendingOpsChanged { pending_ops },
                _,
            ) = e
            {
                let merged = merge_pending_ops_snapshots(
                    pending_ops.items(()),
                    pending_ops_all.lock().items(()),
                );
                *pending_ops_all.lock() = SumTree::from_iter(merged.into_iter(), ());
            }
        })
        .detach();
    });
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;

    let repo = project.read_with(cx, |project, cx| {
        project.repositories(cx).values().next().unwrap().clone()
    });

    repo.update(cx, |repo, cx| {
        repo.stage_entries(vec![repo_path("a.txt")], cx)
    })
    .await
    .unwrap();
    repo.update(cx, |repo, cx| repo.stage_all(cx))
        .await
        .unwrap();
    repo.update(cx, |repo, cx| repo.unstage_all(cx))
        .await
        .unwrap();

    cx.run_until_parked();

    assert_eq!(
        pending_ops_all
            .lock()
            .get(&worktree::PathKey(repo_path("a.txt").as_ref().clone()), ())
            .unwrap()
            .ops,
        vec![
            pending_op::PendingOp {
                id: 1u16.into(),
                git_status: pending_op::GitStatus::Staged,
                job_status: pending_op::JobStatus::Finished
            },
            pending_op::PendingOp {
                id: 2u16.into(),
                git_status: pending_op::GitStatus::Unstaged,
                job_status: pending_op::JobStatus::Finished
            },
        ],
    );
    assert_eq!(
        pending_ops_all
            .lock()
            .get(&worktree::PathKey(repo_path("b.txt").as_ref().clone()), ())
            .unwrap()
            .ops,
        vec![
            pending_op::PendingOp {
                id: 1u16.into(),
                git_status: pending_op::GitStatus::Staged,
                job_status: pending_op::JobStatus::Finished
            },
            pending_op::PendingOp {
                id: 2u16.into(),
                git_status: pending_op::GitStatus::Unstaged,
                job_status: pending_op::JobStatus::Finished
            },
        ],
    );

    repo.update(cx, |repo, _cx| {
        let git_statuses = repo.cached_status().collect::<Vec<_>>();

        assert_eq!(
            git_statuses,
            [
                StatusEntry {
                    repo_path: repo_path("a.txt"),
                    status: FileStatus::Untracked,
                    diff_stat: None,
                },
                StatusEntry {
                    repo_path: repo_path("b.txt"),
                    status: FileStatus::Untracked,
                    diff_stat: None,
                },
            ]
        );
    });
}

#[gpui::test]
async fn test_project_group_keys_remain_distinct_for_sibling_repo_subdirectories(
    executor: gpui::BackgroundExecutor,
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(executor);
    fs.insert_tree(
        path!("/root"),
        json!({
            "my-repo": {
                ".git": {},
                "packages": {
                    "a": { "file.txt": "a" },
                    "b": { "file.txt": "b" },
                },
            },
        }),
    )
    .await;

    let project_a =
        Project::test(fs.clone(), [path!("/root/my-repo/packages/a").as_ref()], cx).await;
    let project_b = Project::test(fs, [path!("/root/my-repo/packages/b").as_ref()], cx).await;

    project_a
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    project_b
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.run_until_parked();

    let key_a = project_a.read_with(cx, |project, cx| ProjectGroupKey::from_project(project, cx));
    let key_b = project_b.read_with(cx, |project, cx| ProjectGroupKey::from_project(project, cx));

    assert_ne!(key_a, key_b);
    assert_eq!(
        key_a
            .path_list()
            .ordered_paths()
            .map(|path| path.as_path())
            .collect::<Vec<_>>(),
        vec![Path::new(path!("/root/my-repo/packages/a"))]
    );
    assert_eq!(
        key_b
            .path_list()
            .ordered_paths()
            .map(|path| path.as_path())
            .collect::<Vec<_>>(),
        vec![Path::new(path!("/root/my-repo/packages/b"))]
    );
}

#[gpui::test]
async fn test_repository_subfolder_git_status(
    executor: gpui::BackgroundExecutor,
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(executor);
    fs.insert_tree(
        path!("/root"),
        json!({
            "my-repo": {
                ".git": {},
                "a.txt": "a",
                "sub-folder-1": {
                    "sub-folder-2": {
                        "c.txt": "cc",
                        "d": {
                            "e.txt": "eee"
                        }
                    },
                }
            },
        }),
    )
    .await;

    const C_TXT: &str = "sub-folder-1/sub-folder-2/c.txt";
    const E_TXT: &str = "sub-folder-1/sub-folder-2/d/e.txt";

    fs.set_status_for_repo(
        path!("/root/my-repo/.git").as_ref(),
        &[(E_TXT, FileStatus::Untracked)],
    );

    let project = Project::test(
        fs.clone(),
        [path!("/root/my-repo/sub-folder-1/sub-folder-2").as_ref()],
        cx,
    )
    .await;

    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.run_until_parked();

    let repository = project.read_with(cx, |project, cx| {
        project.repositories(cx).values().next().unwrap().clone()
    });

    let worktree_paths = project.read_with(cx, |project, cx| project.worktree_paths(cx));
    assert_eq!(
        worktree_paths
            .main_worktree_path_list()
            .ordered_paths()
            .map(|path| path.as_path())
            .collect::<Vec<_>>(),
        vec![Path::new(path!("/root/my-repo/sub-folder-1/sub-folder-2"))]
    );

    // Ensure that the git status is loaded correctly
    repository.read_with(cx, |repository, _cx| {
        assert_eq!(
            repository.work_directory_abs_path,
            Path::new(path!("/root/my-repo")).into()
        );

        assert_eq!(repository.status_for_path(&repo_path(C_TXT)), None);
        assert_eq!(
            repository
                .status_for_path(&repo_path(E_TXT))
                .unwrap()
                .status,
            FileStatus::Untracked
        );
    });

    fs.set_status_for_repo(path!("/root/my-repo/.git").as_ref(), &[]);
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.run_until_parked();

    repository.read_with(cx, |repository, _cx| {
        assert_eq!(repository.status_for_path(&repo_path(C_TXT)), None);
        assert_eq!(repository.status_for_path(&repo_path(E_TXT)), None);
    });
}

// TODO: this test is flaky (especially on Windows but at least sometimes on all platforms).
#[cfg(any())]
#[gpui::test]
async fn test_conflicted_cherry_pick(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();

    let root = TempTree::new(json!({
        "project": {
            "a.txt": "a",
        },
    }));
    let root_path = root.path();

    let work_dir = &root_path.join("project");
    let repo = git_init(work_dir);
    git_add("a.txt", &repo);
    git_commit("init", &repo);

    let project = Project::test(Arc::new(RealFs::new(None, cx.executor())), [root_path], cx).await;

    let tree = project.read_with(cx, |project, cx| project.worktrees(cx).next().unwrap());
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    let repository = project.read_with(cx, |project, cx| {
        project.repositories(cx).values().next().unwrap().clone()
    });

    git_branch("other-branch", &repo);
    git_checkout("other-branch", &repo);
    std::fs::write(root_path.join("project/a.txt"), "A").unwrap();
    git_add("a.txt", &repo);
    git_commit("capitalize", &repo);
    let commit = git_rev_parse("HEAD", &repo);
    git_checkout("main", &repo);
    std::fs::write(root_path.join("project/a.txt"), "b").unwrap();
    git_add("a.txt", &repo);
    git_commit("improve letter", &repo);
    git_cherry_pick_expect_conflict(&commit, &repo);
    std::fs::read_to_string(root_path.join("project/.git/CHERRY_PICK_HEAD"))
        .expect("No CHERRY_PICK_HEAD");
    pretty_assertions::assert_eq!(
        git_status(&repo),
        collections::HashMap::from_iter([("a.txt".to_owned(), GIT_STATUS_CONFLICTED.to_owned())])
    );
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();
    let conflicts = repository.update(cx, |repository, _| {
        repository
            .merge_conflicts
            .iter()
            .cloned()
            .collect::<Vec<_>>()
    });
    pretty_assertions::assert_eq!(conflicts, [RepoPath::from("a.txt")]);

    git_add("a.txt", &repo);
    // Attempt to manually simulate what `git cherry-pick --continue` would do.
    git_commit("whatevs", &repo);
    std::fs::remove_file(root.path().join("project/.git/CHERRY_PICK_HEAD"))
        .expect("Failed to remove CHERRY_PICK_HEAD");
    pretty_assertions::assert_eq!(git_status(&repo), collections::HashMap::default());
    tree.flush_fs_events(cx).await;
    let conflicts = repository.update(cx, |repository, _| {
        repository
            .merge_conflicts
            .iter()
            .cloned()
            .collect::<Vec<_>>()
    });
    pretty_assertions::assert_eq!(conflicts, []);
}

#[gpui::test]
async fn test_update_gitignore(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/root"),
        json!({
            ".git": {},
            ".gitignore": "*.txt\n",
            "a.xml": "<a></a>",
            "b.txt": "Some text"
        }),
    )
    .await;

    fs.set_head_and_index_for_repo(
        path!("/root/.git").as_ref(),
        &[
            (".gitignore", "*.txt\n".into()),
            ("a.xml", "<a></a>".into()),
        ],
    );

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;

    let tree = project.read_with(cx, |project, cx| project.worktrees(cx).next().unwrap());
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    let repository = project.read_with(cx, |project, cx| {
        project.repositories(cx).values().next().unwrap().clone()
    });

    // One file is unmodified, the other is ignored.
    cx.read(|cx| {
        assert_entry_git_state(tree.read(cx), repository.read(cx), "a.xml", None, false);
        assert_entry_git_state(tree.read(cx), repository.read(cx), "b.txt", None, true);
    });

    // Change the gitignore, and stage the newly non-ignored file.
    fs.atomic_write(path!("/root/.gitignore").into(), "*.xml\n".into())
        .await
        .unwrap();
    fs.set_index_for_repo(
        Path::new(path!("/root/.git")),
        &[
            (".gitignore", "*.txt\n".into()),
            ("a.xml", "<a></a>".into()),
            ("b.txt", "Some text".into()),
        ],
    );

    cx.executor().run_until_parked();
    cx.read(|cx| {
        assert_entry_git_state(tree.read(cx), repository.read(cx), "a.xml", None, true);
        assert_entry_git_state(
            tree.read(cx),
            repository.read(cx),
            "b.txt",
            Some(StatusCode::Added),
            false,
        );
    });
}

// NOTE:
// This test always fails on Windows, because on Windows, unlike on Unix, you can't rename
// a directory which some program has already open.
// This is a limitation of the Windows.
// See: https://stackoverflow.com/questions/41365318/access-is-denied-when-renaming-folder
// See: https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/ns-ntifs-_file_rename_information
#[gpui::test]
#[cfg_attr(target_os = "windows", ignore)]
async fn test_rename_work_directory(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();
    let root = TempTree::new(json!({
        "projects": {
            "project1": {
                "a": "",
                "b": "",
            }
        },

    }));
    let root_path = root.path();

    let repo = git_init(&root_path.join("projects/project1"));
    git_add("a", &repo);
    git_commit("init", &repo);
    std::fs::write(root_path.join("projects/project1/a"), "aa").unwrap();

    let project = Project::test(Arc::new(RealFs::new(None, cx.executor())), [root_path], cx).await;

    let tree = project.read_with(cx, |project, cx| project.worktrees(cx).next().unwrap());
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    let repository = project.read_with(cx, |project, cx| {
        project.repositories(cx).values().next().unwrap().clone()
    });

    repository.read_with(cx, |repository, _| {
        assert_eq!(
            repository.work_directory_abs_path.as_ref(),
            root_path.join("projects/project1").as_path()
        );
        assert_eq!(
            repository
                .status_for_path(&repo_path("a"))
                .map(|entry| entry.status),
            Some(StatusCode::Modified.worktree()),
        );
        assert_eq!(
            repository
                .status_for_path(&repo_path("b"))
                .map(|entry| entry.status),
            Some(FileStatus::Untracked),
        );
    });

    std::fs::rename(
        root_path.join("projects/project1"),
        root_path.join("projects/project2"),
    )
    .unwrap();
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    repository.read_with(cx, |repository, _| {
        assert_eq!(
            repository.work_directory_abs_path.as_ref(),
            root_path.join("projects/project2").as_path()
        );
        assert_eq!(
            repository.status_for_path(&repo_path("a")).unwrap().status,
            StatusCode::Modified.worktree(),
        );
        assert_eq!(
            repository.status_for_path(&repo_path("b")).unwrap().status,
            FileStatus::Untracked,
        );
    });
}

// NOTE: This test always fails on Windows, because on Windows, unlike on Unix,
// you can't rename a directory which some program has already open. This is a
// limitation of the Windows. See:
// See: https://stackoverflow.com/questions/41365318/access-is-denied-when-renaming-folder
// See: https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/ns-ntifs-_file_rename_information
#[gpui::test]
#[cfg_attr(target_os = "windows", ignore)]
async fn test_file_status(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();
    const IGNORE_RULE: &str = "**/target";

    let root = TempTree::new(json!({
        "project": {
            "a.txt": "a",
            "b.txt": "bb",
            "c": {
                "d": {
                    "e.txt": "eee"
                }
            },
            "f.txt": "ffff",
            "target": {
                "build_file": "???"
            },
            ".gitignore": IGNORE_RULE
        },

    }));
    let root_path = root.path();

    const A_TXT: &str = "a.txt";
    const B_TXT: &str = "b.txt";
    const E_TXT: &str = "c/d/e.txt";
    const F_TXT: &str = "f.txt";
    const DOTGITIGNORE: &str = ".gitignore";
    const BUILD_FILE: &str = "target/build_file";

    // Set up git repository before creating the worktree.
    let work_dir = root.path().join("project");
    let repo = git_init(work_dir.as_path());
    git_add(A_TXT, &repo);
    git_add(E_TXT, &repo);
    git_add(DOTGITIGNORE, &repo);
    git_commit("Initial commit", &repo);

    let project = Project::test(Arc::new(RealFs::new(None, cx.executor())), [root_path], cx).await;

    let tree = project.read_with(cx, |project, cx| project.worktrees(cx).next().unwrap());
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    let repository = project.read_with(cx, |project, cx| {
        project.repositories(cx).values().next().unwrap().clone()
    });

    // Check that the right git state is observed on startup
    repository.read_with(cx, |repository, _cx| {
        assert_eq!(
            repository.work_directory_abs_path.as_ref(),
            root_path.join("project").as_path()
        );

        assert_eq!(
            repository
                .status_for_path(&repo_path(B_TXT))
                .unwrap()
                .status,
            FileStatus::Untracked,
        );
        assert_eq!(
            repository
                .status_for_path(&repo_path(F_TXT))
                .unwrap()
                .status,
            FileStatus::Untracked,
        );
    });

    // Modify a file in the working copy.
    std::fs::write(work_dir.join(A_TXT), "aa").unwrap();
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    // The worktree detects that the file's git status has changed.
    repository.read_with(cx, |repository, _| {
        assert_eq!(
            repository
                .status_for_path(&repo_path(A_TXT))
                .unwrap()
                .status,
            StatusCode::Modified.worktree(),
        );
    });

    // Create a commit in the git repository.
    git_add(A_TXT, &repo);
    git_add(B_TXT, &repo);
    git_commit("Committing modified and added", &repo);
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    // The worktree detects that the files' git status have changed.
    repository.read_with(cx, |repository, _cx| {
        assert_eq!(
            repository
                .status_for_path(&repo_path(F_TXT))
                .unwrap()
                .status,
            FileStatus::Untracked,
        );
        assert_eq!(repository.status_for_path(&repo_path(B_TXT)), None);
        assert_eq!(repository.status_for_path(&repo_path(A_TXT)), None);
    });

    // Modify files in the working copy and perform git operations on other files.
    git_reset(0, &repo);
    git_remove_index(Path::new(B_TXT), &repo);
    git_stash(&repo);
    std::fs::write(work_dir.join(E_TXT), "eeee").unwrap();
    std::fs::write(work_dir.join(BUILD_FILE), "this should be ignored").unwrap();
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    // Check that more complex repo changes are tracked
    repository.read_with(cx, |repository, _cx| {
        assert_eq!(repository.status_for_path(&repo_path(A_TXT)), None);
        assert_eq!(
            repository
                .status_for_path(&repo_path(B_TXT))
                .unwrap()
                .status,
            FileStatus::Untracked,
        );
        assert_eq!(
            repository
                .status_for_path(&repo_path(E_TXT))
                .unwrap()
                .status,
            StatusCode::Modified.worktree(),
        );
    });

    std::fs::remove_file(work_dir.join(B_TXT)).unwrap();
    std::fs::remove_dir_all(work_dir.join("c")).unwrap();
    std::fs::write(
        work_dir.join(DOTGITIGNORE),
        [IGNORE_RULE, "f.txt"].join("\n"),
    )
    .unwrap();

    git_add(Path::new(DOTGITIGNORE), &repo);
    git_commit("Committing modified git ignore", &repo);

    tree.flush_fs_events(cx).await;
    cx.executor().run_until_parked();

    let mut renamed_dir_name = "first_directory/second_directory";
    const RENAMED_FILE: &str = "rf.txt";

    std::fs::create_dir_all(work_dir.join(renamed_dir_name)).unwrap();
    std::fs::write(
        work_dir.join(renamed_dir_name).join(RENAMED_FILE),
        "new-contents",
    )
    .unwrap();

    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    repository.read_with(cx, |repository, _cx| {
        assert_eq!(
            repository
                .status_for_path(&RepoPath::from_rel_path(
                    &rel_path(renamed_dir_name).join(rel_path(RENAMED_FILE))
                ))
                .unwrap()
                .status,
            FileStatus::Untracked,
        );
    });

    renamed_dir_name = "new_first_directory/second_directory";

    std::fs::rename(
        work_dir.join("first_directory"),
        work_dir.join("new_first_directory"),
    )
    .unwrap();

    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    repository.read_with(cx, |repository, _cx| {
        assert_eq!(
            repository
                .status_for_path(&RepoPath::from_rel_path(
                    &rel_path(renamed_dir_name).join(rel_path(RENAMED_FILE))
                ))
                .unwrap()
                .status,
            FileStatus::Untracked,
        );
    });
}

#[gpui::test]
#[ignore]
async fn test_ignored_dirs_events(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();

    const IGNORE_RULE: &str = "**/target";

    let root = TempTree::new(json!({
        "project": {
            "src": {
                "main.rs": "fn main() {}"
            },
            "target": {
                "debug": {
                    "important_text.txt": "important text",
                },
            },
            ".gitignore": IGNORE_RULE
        },

    }));
    let root_path = root.path();

    // Set up git repository before creating the worktree.
    let work_dir = root.path().join("project");
    let repo = git_init(work_dir.as_path());
    git_add("src/main.rs", &repo);
    git_add(".gitignore", &repo);
    git_commit("Initial commit", &repo);

    let project = Project::test(Arc::new(RealFs::new(None, cx.executor())), [root_path], cx).await;
    let repository_updates = Arc::new(Mutex::new(Vec::new()));
    let project_events = Arc::new(Mutex::new(Vec::new()));
    project.update(cx, |project, cx| {
        let repo_events = repository_updates.clone();
        cx.subscribe(project.git_store(), move |_, _, e, _| {
            if let GitStoreEvent::RepositoryUpdated(_, e, _) = e {
                repo_events.lock().push(e.clone());
            }
        })
        .detach();
        let project_events = project_events.clone();
        cx.subscribe_self(move |_, e, _| {
            if let Event::WorktreeUpdatedEntries(_, updates) = e {
                project_events.lock().extend(
                    updates
                        .iter()
                        .map(|(path, _, change)| (path.as_unix_str().to_string(), *change))
                        .filter(|(path, _)| path != "fs-event-sentinel"),
                );
            }
        })
        .detach();
    });

    let tree = project.read_with(cx, |project, cx| project.worktrees(cx).next().unwrap());
    tree.flush_fs_events(cx).await;
    tree.update(cx, |tree, cx| {
        tree.load_file(rel_path("project/target/debug/important_text.txt"), cx)
    })
    .await
    .unwrap();
    tree.update(cx, |tree, _| {
        assert_eq!(
            tree.entries(true, 0)
                .map(|entry| (entry.path.as_ref(), entry.is_ignored))
                .collect::<Vec<_>>(),
            vec![
                (rel_path(""), false),
                (rel_path("project/"), false),
                (rel_path("project/.gitignore"), false),
                (rel_path("project/src"), false),
                (rel_path("project/src/main.rs"), false),
                (rel_path("project/target"), true),
                (rel_path("project/target/debug"), true),
                (rel_path("project/target/debug/important_text.txt"), true),
            ]
        );
    });

    assert_eq!(
        repository_updates.lock().drain(..).collect::<Vec<_>>(),
        vec![RepositoryEvent::StatusesChanged,],
        "Initial worktree scan should produce a repo update event"
    );
    assert_eq!(
        project_events.lock().drain(..).collect::<Vec<_>>(),
        vec![
            ("project/target".to_string(), PathChange::Loaded),
            ("project/target/debug".to_string(), PathChange::Loaded),
            (
                "project/target/debug/important_text.txt".to_string(),
                PathChange::Loaded
            ),
        ],
        "Initial project changes should show that all not-ignored and all opened files are loaded"
    );

    let deps_dir = work_dir.join("target").join("debug").join("deps");
    std::fs::create_dir_all(&deps_dir).unwrap();
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();
    std::fs::write(deps_dir.join("aa.tmp"), "something tmp").unwrap();
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();
    std::fs::remove_dir_all(&deps_dir).unwrap();
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    tree.update(cx, |tree, _| {
        assert_eq!(
            tree.entries(true, 0)
                .map(|entry| (entry.path.as_ref(), entry.is_ignored))
                .collect::<Vec<_>>(),
            vec![
                (rel_path(""), false),
                (rel_path("project/"), false),
                (rel_path("project/.gitignore"), false),
                (rel_path("project/src"), false),
                (rel_path("project/src/main.rs"), false),
                (rel_path("project/target"), true),
                (rel_path("project/target/debug"), true),
                (rel_path("project/target/debug/important_text.txt"), true),
            ],
            "No stray temp files should be left after the flycheck changes"
        );
    });

    assert_eq!(
        repository_updates
            .lock()
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        Vec::new(),
        "No further RepositoryUpdated events should happen, as only ignored dirs' contents was changed",
    );
    assert_eq!(
        project_events.lock().as_slice(),
        vec![
            ("project/target/debug/deps".to_string(), PathChange::Added),
            ("project/target/debug/deps".to_string(), PathChange::Removed),
        ],
        "Due to `debug` directory being tracked, it should get updates for entries inside it.
        No updates for more nested directories should happen as those are ignored",
    );
}

// todo(jk): turning this test off until we rework it in such a way so that it is not so susceptible
// to different timings/ordering of events.
#[ignore]
#[gpui::test]
async fn test_odd_events_for_ignored_dirs(
    executor: BackgroundExecutor,
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx);
    let fs = FakeFs::new(executor);
    fs.insert_tree(
        path!("/root"),
        json!({
            ".git": {},
            ".gitignore": "**/target/",
            "src": {
                "main.rs": "fn main() {}",
            },
            "target": {
                "debug": {
                    "foo.txt": "foo",
                    "deps": {}
                }
            }
        }),
    )
    .await;
    fs.set_head_and_index_for_repo(
        path!("/root/.git").as_ref(),
        &[
            (".gitignore", "**/target/".into()),
            ("src/main.rs", "fn main() {}".into()),
        ],
    );

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let repository_updates = Arc::new(Mutex::new(Vec::new()));
    let project_events = Arc::new(Mutex::new(Vec::new()));
    project.update(cx, |project, cx| {
        let repository_updates = repository_updates.clone();
        cx.subscribe(project.git_store(), move |_, _, e, _| {
            if let GitStoreEvent::RepositoryUpdated(_, e, _) = e {
                repository_updates.lock().push(e.clone());
            }
        })
        .detach();
        let project_events = project_events.clone();
        cx.subscribe_self(move |_, e, _| {
            if let Event::WorktreeUpdatedEntries(_, updates) = e {
                project_events.lock().extend(
                    updates
                        .iter()
                        .map(|(path, _, change)| (path.as_unix_str().to_string(), *change))
                        .filter(|(path, _)| path != "fs-event-sentinel"),
                );
            }
        })
        .detach();
    });

    let tree = project.read_with(cx, |project, cx| project.worktrees(cx).next().unwrap());
    tree.update(cx, |tree, cx| {
        tree.load_file(rel_path("target/debug/foo.txt"), cx)
    })
    .await
    .unwrap();
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.run_until_parked();
    tree.update(cx, |tree, _| {
        assert_eq!(
            tree.entries(true, 0)
                .map(|entry| (entry.path.as_ref(), entry.is_ignored))
                .collect::<Vec<_>>(),
            vec![
                (rel_path(""), false),
                (rel_path(".gitignore"), false),
                (rel_path("src"), false),
                (rel_path("src/main.rs"), false),
                (rel_path("target"), true),
                (rel_path("target/debug"), true),
                (rel_path("target/debug/deps"), true),
                (rel_path("target/debug/foo.txt"), true),
            ]
        );
    });

    assert_eq!(
        repository_updates.lock().drain(..).collect::<Vec<_>>(),
        vec![
            RepositoryEvent::HeadChanged,
            RepositoryEvent::StatusesChanged,
            RepositoryEvent::StatusesChanged,
        ],
        "Initial worktree scan should produce a repo update event"
    );
    assert_eq!(
        project_events.lock().drain(..).collect::<Vec<_>>(),
        vec![
            ("target".to_string(), PathChange::Loaded),
            ("target/debug".to_string(), PathChange::Loaded),
            ("target/debug/deps".to_string(), PathChange::Loaded),
            ("target/debug/foo.txt".to_string(), PathChange::Loaded),
        ],
        "All non-ignored entries and all opened firs should be getting a project event",
    );

    // Emulate a flycheck spawn: it emits a `INODE_META_MOD`-flagged FS event on target/debug/deps, then creates and removes temp files inside.
    // This may happen multiple times during a single flycheck, but once is enough for testing.
    fs.emit_fs_event("/root/target/debug/deps", None);
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    assert_eq!(
        repository_updates
            .lock()
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        Vec::new(),
        "No further RepositoryUpdated events should happen, as only ignored dirs received FS events",
    );
    assert_eq!(
        project_events.lock().as_slice(),
        Vec::new(),
        "No further project events should happen, as only ignored dirs received FS events",
    );
}

#[gpui::test]
async fn test_repos_in_invisible_worktrees(
    executor: BackgroundExecutor,
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx);
    let fs = FakeFs::new(executor);
    fs.insert_tree(
        path!("/root"),
        json!({
            "dir1": {
                ".git": {},
                "dep1": {
                    ".git": {},
                    "src": {
                        "a.txt": "",
                    },
                },
                "b.txt": "",
            },
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/root/dir1/dep1").as_ref()], cx).await;
    let _visible_worktree =
        project.read_with(cx, |project, cx| project.worktrees(cx).next().unwrap());
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;

    let repos = project.read_with(cx, |project, cx| {
        project
            .repositories(cx)
            .values()
            .map(|repo| repo.read(cx).work_directory_abs_path.clone())
            .collect::<Vec<_>>()
    });
    pretty_assertions::assert_eq!(repos, [Path::new(path!("/root/dir1/dep1")).into()]);

    let (_invisible_worktree, _) = project
        .update(cx, |project, cx| {
            project.worktree_store().update(cx, |worktree_store, cx| {
                worktree_store.find_or_create_worktree(path!("/root/dir1/b.txt"), false, cx)
            })
        })
        .await
        .expect("failed to create worktree");
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;

    let repos = project.read_with(cx, |project, cx| {
        project
            .repositories(cx)
            .values()
            .map(|repo| repo.read(cx).work_directory_abs_path.clone())
            .collect::<Vec<_>>()
    });
    pretty_assertions::assert_eq!(repos, [Path::new(path!("/root/dir1/dep1")).into()]);
}

#[gpui::test(iterations = 10)]
async fn test_rescan_with_gitignore(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.worktree.file_scan_exclusions = Some(Vec::new());
            });
        });
    });
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/root"),
        json!({
            ".gitignore": "ancestor-ignored-file1\nancestor-ignored-file2\n",
            "tree": {
                ".git": {},
                ".gitignore": "ignored-dir\n",
                "tracked-dir": {
                    "tracked-file1": "",
                    "ancestor-ignored-file1": "",
                },
                "ignored-dir": {
                    "ignored-file1": ""
                }
            }
        }),
    )
    .await;
    fs.set_head_and_index_for_repo(
        path!("/root/tree/.git").as_ref(),
        &[
            (".gitignore", "ignored-dir\n".into()),
            ("tracked-dir/tracked-file1", "".into()),
        ],
    );

    let project = Project::test(fs.clone(), [path!("/root/tree").as_ref()], cx).await;

    let tree = project.read_with(cx, |project, cx| project.worktrees(cx).next().unwrap());
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    let repository = project.read_with(cx, |project, cx| {
        project.repositories(cx).values().next().unwrap().clone()
    });

    tree.read_with(cx, |tree, _| {
        tree.as_local()
            .unwrap()
            .manually_refresh_entries_for_paths(vec![rel_path("ignored-dir").into()])
    })
    .recv()
    .await;

    cx.read(|cx| {
        assert_entry_git_state(
            tree.read(cx),
            repository.read(cx),
            "tracked-dir/tracked-file1",
            None,
            false,
        );
        assert_entry_git_state(
            tree.read(cx),
            repository.read(cx),
            "tracked-dir/ancestor-ignored-file1",
            None,
            false,
        );
        assert_entry_git_state(
            tree.read(cx),
            repository.read(cx),
            "ignored-dir/ignored-file1",
            None,
            true,
        );
    });

    fs.create_file(
        path!("/root/tree/tracked-dir/tracked-file2").as_ref(),
        Default::default(),
    )
    .await
    .unwrap();
    fs.set_index_for_repo(
        path!("/root/tree/.git").as_ref(),
        &[
            (".gitignore", "ignored-dir\n".into()),
            ("tracked-dir/tracked-file1", "".into()),
            ("tracked-dir/tracked-file2", "".into()),
        ],
    );
    fs.create_file(
        path!("/root/tree/tracked-dir/ancestor-ignored-file2").as_ref(),
        Default::default(),
    )
    .await
    .unwrap();
    fs.create_file(
        path!("/root/tree/ignored-dir/ignored-file2").as_ref(),
        Default::default(),
    )
    .await
    .unwrap();

    cx.executor().run_until_parked();
    cx.read(|cx| {
        assert_entry_git_state(
            tree.read(cx),
            repository.read(cx),
            "tracked-dir/tracked-file2",
            Some(StatusCode::Added),
            false,
        );
        assert_entry_git_state(
            tree.read(cx),
            repository.read(cx),
            "tracked-dir/ancestor-ignored-file2",
            None,
            false,
        );
        assert_entry_git_state(
            tree.read(cx),
            repository.read(cx),
            "ignored-dir/ignored-file2",
            None,
            true,
        );
        assert!(
            tree.read(cx)
                .entry_for_path(&rel_path(".git"))
                .unwrap()
                .is_ignored
        );
    });
}

#[gpui::test]
async fn test_git_worktrees_and_submodules(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({
            ".git": {
                "worktrees": {
                    "some-worktree": {
                        "commondir": "../..\n",
                        // For is_git_dir
                        "HEAD": "",
                        "config": ""
                    }
                },
                "modules": {
                    "subdir": {
                        "some-submodule": {
                            // For is_git_dir
                            "HEAD": "",
                            "config": "",
                        }
                    }
                }
            },
            "src": {
                "a.txt": "A",
            },
            "some-worktree": {
                ".git": "gitdir: ../.git/worktrees/some-worktree\n",
                "src": {
                    "b.txt": "B",
                }
            },
            "subdir": {
                "some-submodule": {
                    ".git": "gitdir: ../../.git/modules/subdir/some-submodule\n",
                    "c.txt": "C",
                }
            }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/project").as_ref()], cx).await;
    let scan_complete = project.update(cx, |project, cx| project.git_scans_complete(cx));
    scan_complete.await;

    let mut repositories = project.update(cx, |project, cx| {
        project
            .repositories(cx)
            .values()
            .map(|repo| repo.read(cx).work_directory_abs_path.clone())
            .collect::<Vec<_>>()
    });
    repositories.sort();
    pretty_assertions::assert_eq!(
        repositories,
        [
            Path::new(path!("/project")).into(),
            Path::new(path!("/project/some-worktree")).into(),
            Path::new(path!("/project/subdir/some-submodule")).into(),
        ]
    );

    // Generate a git-related event for the worktree and check that it's refreshed.
    fs.with_git_state(
        path!("/project/some-worktree/.git").as_ref(),
        true,
        |state| {
            state
                .head_contents
                .insert(repo_path("src/b.txt"), "b".to_owned());
            state
                .index_contents
                .insert(repo_path("src/b.txt"), "b".to_owned());
        },
    )
    .unwrap();
    cx.run_until_parked();

    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/project/some-worktree/src/b.txt"), cx)
        })
        .await
        .unwrap();
    let (worktree_repo, barrier) = project.update(cx, |project, cx| {
        let (repo, _) = project
            .git_store()
            .read(cx)
            .repository_and_path_for_buffer_id(buffer.read(cx).remote_id(), cx)
            .unwrap();
        pretty_assertions::assert_eq!(
            repo.read(cx).work_directory_abs_path,
            Path::new(path!("/project/some-worktree")).into(),
        );
        pretty_assertions::assert_eq!(
            repo.read(cx).main_worktree_abs_path(),
            Some(Path::new(path!("/project"))),
        );
        assert!(
            repo.read(cx).linked_worktree_path().is_some(),
            "linked worktree should be detected as a linked worktree"
        );
        let barrier = repo.update(cx, |repo, _| repo.barrier());
        (repo.clone(), barrier)
    });
    barrier.await.unwrap();
    worktree_repo.update(cx, |repo, _| {
        pretty_assertions::assert_eq!(
            repo.status_for_path(&repo_path("src/b.txt"))
                .unwrap()
                .status,
            StatusCode::Modified.worktree(),
        );
    });

    // The same for the submodule.
    fs.with_git_state(
        path!("/project/subdir/some-submodule/.git").as_ref(),
        true,
        |state| {
            state
                .head_contents
                .insert(repo_path("c.txt"), "c".to_owned());
            state
                .index_contents
                .insert(repo_path("c.txt"), "c".to_owned());
        },
    )
    .unwrap();
    cx.run_until_parked();

    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/project/subdir/some-submodule/c.txt"), cx)
        })
        .await
        .unwrap();
    let (submodule_repo, barrier) = project.update(cx, |project, cx| {
        let (repo, _) = project
            .git_store()
            .read(cx)
            .repository_and_path_for_buffer_id(buffer.read(cx).remote_id(), cx)
            .unwrap();
        pretty_assertions::assert_eq!(
            repo.read(cx).work_directory_abs_path,
            Path::new(path!("/project/subdir/some-submodule")).into(),
        );
        pretty_assertions::assert_eq!(
            repo.read(cx).main_worktree_abs_path(),
            Some(Path::new(path!("/project/subdir/some-submodule"))),
        );
        assert!(
            repo.read(cx).linked_worktree_path().is_none(),
            "submodule should not be detected as a linked worktree"
        );
        let barrier = repo.update(cx, |repo, _| repo.barrier());
        (repo.clone(), barrier)
    });
    barrier.await.unwrap();
    submodule_repo.update(cx, |repo, _| {
        pretty_assertions::assert_eq!(
            repo.status_for_path(&repo_path("c.txt")).unwrap().status,
            StatusCode::Modified.worktree(),
        );
    });
}

#[gpui::test]
async fn test_repository_deduplication(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/root"),
        json!({
            "project": {
                ".git": {},
                "child1": {
                    "a.txt": "A",
                },
                "child2": {
                    "b.txt": "B",
                }
            }
        }),
    )
    .await;

    let project = Project::test(
        fs.clone(),
        [
            path!("/root/project/child1").as_ref(),
            path!("/root/project/child2").as_ref(),
        ],
        cx,
    )
    .await;

    let tree = project.read_with(cx, |project, cx| project.worktrees(cx).next().unwrap());
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    let repos = project.read_with(cx, |project, cx| {
        project
            .repositories(cx)
            .values()
            .map(|repo| repo.read(cx).work_directory_abs_path.clone())
            .collect::<Vec<_>>()
    });
    pretty_assertions::assert_eq!(repos, [Path::new(path!("/root/project")).into()]);
}

#[gpui::test]
async fn test_buffer_changed_file_path_updates_git_diff(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let file_1_committed = String::from(r#"file_1_committed"#);
    let file_1_staged = String::from(r#"file_1_staged"#);
    let file_2_committed = String::from(r#"file_2_committed"#);
    let file_2_staged = String::from(r#"file_2_staged"#);
    let buffer_contents = String::from(r#"buffer"#);

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/dir"),
        json!({
            ".git": {},
           "src": {
               "file_1.rs": file_1_committed.clone(),
               "file_2.rs": file_2_committed.clone(),
           }
        }),
    )
    .await;

    fs.set_head_for_repo(
        path!("/dir/.git").as_ref(),
        &[
            ("src/file_1.rs", file_1_committed.clone()),
            ("src/file_2.rs", file_2_committed.clone()),
        ],
        "deadbeef",
    );
    fs.set_index_for_repo(
        path!("/dir/.git").as_ref(),
        &[
            ("src/file_1.rs", file_1_staged.clone()),
            ("src/file_2.rs", file_2_staged.clone()),
        ],
    );

    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;

    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/dir/src/file_1.rs"), cx)
        })
        .await
        .unwrap();

    buffer.update(cx, |buffer, cx| {
        buffer.edit([(0..buffer.len(), buffer_contents.as_str())], None, cx);
    });

    let unstaged_diff = project
        .update(cx, |project, cx| {
            project.open_unstaged_diff(buffer.clone(), cx)
        })
        .await
        .unwrap();

    cx.run_until_parked();

    unstaged_diff.update(cx, |unstaged_diff, cx| {
        let base_text = unstaged_diff.base_text_string(cx).unwrap();
        assert_eq!(base_text, file_1_staged, "Should start with file_1 staged");
    });

    // Save the buffer as `file_2.rs`, which should trigger the
    // `BufferChangedFilePath` event.
    project
        .update(cx, |project, cx| {
            let worktree_id = project.worktrees(cx).next().unwrap().read(cx).id();
            let path = ProjectPath {
                worktree_id,
                path: rel_path("src/file_2.rs").into(),
            };
            project.save_buffer_as(buffer.clone(), path, cx)
        })
        .await
        .unwrap();

    cx.run_until_parked();

    // Verify that the diff bases have been updated to file_2's contents due to
    // the `BufferChangedFilePath` event being handled.
    unstaged_diff.update(cx, |unstaged_diff, cx| {
        let snapshot = buffer.read(cx).snapshot();
        let base_text = unstaged_diff.base_text_string(cx).unwrap();
        assert_eq!(
            base_text, file_2_staged,
            "Diff bases should be automatically updated to file_2 staged content"
        );

        let hunks: Vec<_> = unstaged_diff.snapshot(cx).hunks(&snapshot).collect();
        assert!(!hunks.is_empty(), "Should have diff hunks for file_2");
    });

    let uncommitted_diff = project
        .update(cx, |project, cx| {
            project.open_uncommitted_diff(buffer.clone(), cx)
        })
        .await
        .unwrap();

    cx.run_until_parked();

    uncommitted_diff.update(cx, |uncommitted_diff, cx| {
        let base_text = uncommitted_diff.base_text_string(cx).unwrap();
        assert_eq!(
            base_text, file_2_committed,
            "Uncommitted diff should compare against file_2 committed content"
        );
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
