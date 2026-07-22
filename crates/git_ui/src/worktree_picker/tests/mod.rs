use super::super::*;
use super::*;
use fs::FakeFs;
use gpui::{AppContext, TestAppContext, VisualTestContext};
use project::project_settings::ProjectSettings;
use project::{Project, WorktreeSettings};
use serde_json::json;
use settings::Settings as _;
use settings::SettingsStore;
use util::path;
use workspace::MultiWorkspace;

fn init_test(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
        theme_settings::init(theme::LoadThemes::JustBase, cx);
        editor::init(cx);
        ProjectSettings::register(cx);
        WorktreeSettings::register(cx);
    });
}

async fn init_worktree_picker_test(
    cx: &mut TestAppContext,
) -> (
    Arc<FakeFs>,
    Entity<WorktreePicker>,
    Entity<project::git_store::Repository>,
    PathBuf,
    VisualTestContext,
) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "project": {
                ".git": {},
                "file.txt": "buffer_text",
            },
            "worktrees": {},
        }),
    )
    .await;
    fs.set_head_for_repo(
        path!("/root/project/.git").as_ref(),
        &[("file.txt", "buffer_text".to_string())],
        "deadbeef",
    );

    let project = Project::test(fs.clone(), [path!("/root/project").as_ref()], cx).await;
    cx.executor().run_until_parked();

    let repository = project.read_with(cx, |project, cx| {
        project.repositories(cx).values().next().unwrap().clone()
    });
    let worktree_path = PathBuf::from(path!("/root/worktrees/dirty-wt"));

    cx.update(|cx| {
        repository.update(cx, |repository, _| {
            repository.create_worktree(
                git::repository::CreateWorktreeTarget::NewBranch {
                    branch_name: "dirty-wt".to_string(),
                    base_sha: Some("deadbeef".to_string()),
                },
                worktree_path.clone(),
            )
        })
    })
    .await
    .unwrap()
    .unwrap();

    let window_handle =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window_handle
        .read_with(cx, |multi_workspace, _| multi_workspace.workspace().clone())
        .unwrap();

    let mut cx = VisualTestContext::from_window(window_handle.into(), cx);

    let worktree_picker = cx.update(|window, cx| {
        cx.new(|cx| WorktreePicker::new(project, workspace.downgrade(), window, cx))
    });

    cx.run_until_parked();

    (fs, worktree_picker, repository, worktree_path, cx)
}

fn worktree_index(
    worktree_picker: &Entity<WorktreePicker>,
    worktree_path: &Path,
    cx: &mut VisualTestContext,
) -> usize {
    worktree_picker.update(cx, |worktree_picker, cx| {
            worktree_picker.picker.update(cx, |picker, _| {
                picker
                    .delegate
                    .matches
                    .iter()
                    .position(|entry| {
                        matches!(entry, WorktreeEntry::Worktree { worktree, .. } if worktree.path == *worktree_path)
                    })
                    .expect("worktree should appear in picker")
            })
        })
}

fn picker_contains_worktree(
    worktree_picker: &Entity<WorktreePicker>,
    worktree_path: &Path,
    cx: &mut VisualTestContext,
) -> bool {
    worktree_picker.update(cx, |worktree_picker, cx| {
            worktree_picker.picker.update(cx, |picker, _| {
                picker.delegate.all_worktrees.iter().any(|worktree| {
                    worktree.path == *worktree_path
                }) && picker.delegate.matches.iter().any(|entry| {
                    matches!(entry, WorktreeEntry::Worktree { worktree, .. } if worktree.path == *worktree_path)
                })
            })
        })
}

fn deleting_worktree_paths(
    worktree_picker: &Entity<WorktreePicker>,
    cx: &mut VisualTestContext,
) -> HashSet<PathBuf> {
    worktree_picker.update(cx, |worktree_picker, cx| {
        worktree_picker.picker.update(cx, |picker, _| {
            picker.delegate.deleting_worktree_paths.clone()
        })
    })
}

async fn repo_contains_worktree(
    repository: &Entity<project::git_store::Repository>,
    worktree_path: &Path,
    cx: &mut VisualTestContext,
) -> bool {
    let worktrees = repository
        .update(cx, |repository, _| repository.worktrees())
        .await
        .unwrap()
        .unwrap();
    worktrees
        .iter()
        .any(|worktree| worktree.path == *worktree_path)
}

mod create_tests;
mod delete_tests;
mod grouping_tests;
mod remove_tests;
