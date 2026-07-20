use super::*;
use pretty_assertions::assert_eq;

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
