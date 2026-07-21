use super::*;
use crate::Project;
use fs::{FakeFs, Fs};
use git::repository::{RepoPath, repo_path};
use gpui::TestAppContext;
use gpui::proptest::prelude::*;
use rand::{SeedableRng, rngs::StdRng};
use serde_json::json;
use settings::SettingsStore;
use std::path::{Path, PathBuf};

fn init_test(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
    });
}

#[gpui::test]
async fn test_open_uncommitted_diff_skips_symlinks(cx: &mut TestAppContext) {
    use util::rel_path::rel_path;

    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        Path::new("/project"),
        json!({
            ".git": {},
            "target.txt": "rule one\nrule two\n",
        }),
    )
    .await;
    fs.insert_symlink("/project/agents.md", PathBuf::from("target.txt"))
        .await;

    fs.set_head_and_index_for_repo(
        Path::new("/project/.git"),
        &[
            // git stores the symlink's target path as the blob for `agents.md`
            ("agents.md", "target.txt".into()),
            ("target.txt", "rule one\n".into()),
        ],
    );

    let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;

    let worktree_id = project.read_with(cx, |project, cx| {
        project.worktrees(cx).next().unwrap().read(cx).id()
    });

    // symlink file should not produce a base diff
    let symlink_buffer = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("agents.md")), cx)
        })
        .await
        .unwrap();
    let symlink_diff = project
        .update(cx, |project, cx| {
            project.open_uncommitted_diff(symlink_buffer, cx)
        })
        .await
        .unwrap();
    symlink_diff.read_with(cx, |diff, _| {
        assert!(
            !diff.base_text_exists(),
            "symlinked buffer should not have a git diff base"
        );
    });

    // regular file should still produce a base diff
    let regular_buffer = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("target.txt")), cx)
        })
        .await
        .unwrap();
    let regular_diff = project
        .update(cx, |project, cx| {
            project.open_uncommitted_diff(regular_buffer, cx)
        })
        .await
        .unwrap();
    regular_diff.read_with(cx, |diff, _| {
        assert!(
            diff.base_text_exists(),
            "regular file should have a git diff base"
        );
    });
}

#[gpui::test]
async fn test_append_pattern_to_ignore_file_creates_and_deduplicates(cx: &mut TestAppContext) {
    let fs: Arc<dyn Fs> = FakeFs::new(cx.executor());
    let path = PathBuf::from("/root/.gitignore");

    // Appending to a non-existent file creates it with a trailing newline.
    super::append_pattern_to_ignore_file(fs.clone(), path.clone(), "build/".to_string())
        .await
        .unwrap();
    assert_eq!(fs.load(&path).await.unwrap(), "build/\n");

    // Appending the same pattern again is a no-op (deduplication).
    super::append_pattern_to_ignore_file(fs.clone(), path.clone(), "build/".to_string())
        .await
        .unwrap();
    assert_eq!(fs.load(&path).await.unwrap(), "build/\n");

    // Appending a distinct pattern adds it with a trailing newline.
    super::append_pattern_to_ignore_file(fs.clone(), path.clone(), "target/".to_string())
        .await
        .unwrap();
    assert_eq!(fs.load(&path).await.unwrap(), "build/\ntarget/\n");
}

#[gpui::test]
async fn test_append_pattern_adds_newline_before_pattern_when_missing(cx: &mut TestAppContext) {
    let fs: Arc<dyn Fs> = FakeFs::new(cx.executor());
    let path = PathBuf::from("/root/.gitignore");

    // Pre-populate the file without a trailing newline.
    fs.save(&path, &text::Rope::from("*.log"), text::LineEnding::Unix)
        .await
        .unwrap();

    // The new pattern must be written on its own line.
    super::append_pattern_to_ignore_file(fs.clone(), path.clone(), "build/".to_string())
        .await
        .unwrap();
    assert_eq!(fs.load(&path).await.unwrap(), "*.log\nbuild/\n");
}

#[test]
fn test_new_worktree_path_uses_posix_style_for_remote_paths() {
    let work_dir = Path::new("/home/user/dev/lsp-tests");
    let directory =
        worktrees_directory_for_repo(work_dir, "../worktrees", PathStyle::Posix).unwrap();
    let directory = PathStyle::Posix
        .join_path(&directory, "nimble-sky")
        .unwrap();
    let path = PathStyle::Posix.join_path(&directory, "lsp-tests").unwrap();

    assert_eq!(
        path,
        PathBuf::from("/home/user/dev/worktrees/lsp-tests/nimble-sky/lsp-tests")
    );
}

fn repo_paths(paths: &[&str]) -> Vec<RepoPath> {
    paths.iter().map(repo_path).collect()
}

#[test]
fn coalesce_repo_paths_keeps_root_only() {
    let coalesced = GitStore::coalesce_repo_paths(repo_paths(&["", "src", "src/lib.rs"]));

    assert_eq!(coalesced, repo_paths(&[""]));
}

#[test]
fn coalesce_repo_paths_keeps_existing_ancestors() {
    let coalesced = GitStore::coalesce_repo_paths(repo_paths(&[
        "src",
        "src/lib.rs",
        "src/nested/file.rs",
        "tests/test.rs",
    ]));

    assert_eq!(coalesced, repo_paths(&["src", "tests/test.rs"]));
}

#[test]
fn coalesce_repo_paths_does_not_invent_missing_parents() {
    let coalesced = GitStore::coalesce_repo_paths(repo_paths(&[
        "submodule/a.txt",
        "submodule/nested/b.txt",
        "top_level.rs",
    ]));

    assert_eq!(
        coalesced,
        repo_paths(&["submodule/a.txt", "submodule/nested/b.txt", "top_level.rs"])
    );
}
