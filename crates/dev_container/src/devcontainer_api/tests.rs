use super::*;

use std::path::PathBuf;

use crate::devcontainer_api::{DevContainerConfig, find_configs_in_snapshot};
use fs::FakeFs;
use gpui::TestAppContext;
use project::Project;
use serde_json::json;
use settings::SettingsStore;
use util::path;

fn init_test(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
    });
}

#[gpui::test]
async fn test_find_configs_root_devcontainer_json(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({
            ".devcontainer.json": "{}"
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/project").as_ref()], cx).await;
    cx.run_until_parked();

    let configs = project.read_with(cx, |project, cx| {
        let worktree = project
            .visible_worktrees(cx)
            .next()
            .expect("should have a worktree");
        find_configs_in_snapshot(worktree.read(cx))
    });

    assert_eq!(configs.len(), 1);
    assert_eq!(configs[0].name, "root");
    assert_eq!(configs[0].config_path, PathBuf::from(".devcontainer.json"));
}

#[gpui::test]
async fn test_find_configs_default_devcontainer_dir(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({
            ".devcontainer": {
                "devcontainer.json": "{}"
            }
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/project").as_ref()], cx).await;
    cx.run_until_parked();

    let configs = project.read_with(cx, |project, cx| {
        let worktree = project
            .visible_worktrees(cx)
            .next()
            .expect("should have a worktree");
        find_configs_in_snapshot(worktree.read(cx))
    });

    assert_eq!(configs.len(), 1);
    assert_eq!(configs[0], DevContainerConfig::default_config());
}

#[gpui::test]
async fn test_find_configs_dir_and_root_both_included(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({
            ".devcontainer.json": "{}",
            ".devcontainer": {
                "devcontainer.json": "{}"
            }
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/project").as_ref()], cx).await;
    cx.run_until_parked();

    let configs = project.read_with(cx, |project, cx| {
        let worktree = project
            .visible_worktrees(cx)
            .next()
            .expect("should have a worktree");
        find_configs_in_snapshot(worktree.read(cx))
    });

    assert_eq!(configs.len(), 2);
    assert_eq!(configs[0], DevContainerConfig::default_config());
    assert_eq!(configs[1], DevContainerConfig::root_config());
}

#[gpui::test]
async fn test_find_configs_subfolder_configs(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({
            ".devcontainer": {
                "rust": {
                    "devcontainer.json": "{}"
                },
                "python": {
                    "devcontainer.json": "{}"
                }
            }
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/project").as_ref()], cx).await;
    cx.run_until_parked();

    let configs = project.read_with(cx, |project, cx| {
        let worktree = project
            .visible_worktrees(cx)
            .next()
            .expect("should have a worktree");
        find_configs_in_snapshot(worktree.read(cx))
    });

    assert_eq!(configs.len(), 2);
    let names: Vec<&str> = configs.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"python"));
    assert!(names.contains(&"rust"));
}

#[gpui::test]
async fn test_find_configs_default_and_subfolder(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({
            ".devcontainer": {
                "devcontainer.json": "{}",
                "gpu": {
                    "devcontainer.json": "{}"
                }
            }
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/project").as_ref()], cx).await;
    cx.run_until_parked();

    let configs = project.read_with(cx, |project, cx| {
        let worktree = project
            .visible_worktrees(cx)
            .next()
            .expect("should have a worktree");
        find_configs_in_snapshot(worktree.read(cx))
    });

    assert_eq!(configs.len(), 2);
    assert_eq!(configs[0].name, "default");
    assert_eq!(configs[1].name, "gpu");
}

#[gpui::test]
async fn test_find_configs_no_devcontainer(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({
            "src": {
                "main.rs": "fn main() {}"
            }
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/project").as_ref()], cx).await;
    cx.run_until_parked();

    let configs = project.read_with(cx, |project, cx| {
        let worktree = project
            .visible_worktrees(cx)
            .next()
            .expect("should have a worktree");
        find_configs_in_snapshot(worktree.read(cx))
    });

    assert!(configs.is_empty());
}

#[gpui::test]
async fn test_find_configs_root_json_and_subfolder_configs(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({
            ".devcontainer.json": "{}",
            ".devcontainer": {
                "rust": {
                    "devcontainer.json": "{}"
                }
            }
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/project").as_ref()], cx).await;
    cx.run_until_parked();

    let configs = project.read_with(cx, |project, cx| {
        let worktree = project
            .visible_worktrees(cx)
            .next()
            .expect("should have a worktree");
        find_configs_in_snapshot(worktree.read(cx))
    });

    assert_eq!(configs.len(), 2);
    assert_eq!(configs[0].name, "root");
    assert_eq!(configs[0].config_path, PathBuf::from(".devcontainer.json"));
    assert_eq!(configs[1].name, "rust");
    assert_eq!(
        configs[1].config_path,
        PathBuf::from(".devcontainer/rust/devcontainer.json")
    );
}

#[gpui::test]
async fn test_find_configs_empty_devcontainer_dir_falls_back_to_root(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({
            ".devcontainer.json": "{}",
            ".devcontainer": {}
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/project").as_ref()], cx).await;
    cx.run_until_parked();

    let configs = project.read_with(cx, |project, cx| {
        let worktree = project
            .visible_worktrees(cx)
            .next()
            .expect("should have a worktree");
        find_configs_in_snapshot(worktree.read(cx))
    });

    assert_eq!(configs.len(), 1);
    assert_eq!(configs[0], DevContainerConfig::root_config());
}
