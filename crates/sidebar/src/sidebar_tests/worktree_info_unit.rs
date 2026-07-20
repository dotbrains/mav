use super::*;

#[test]
fn test_worktree_info_branch_names_for_main_worktrees() {
    let folder_paths = PathList::new(&[PathBuf::from("/projects/myapp")]);
    let worktree_paths = WorktreePaths::from_folder_paths(&folder_paths);

    let branch_by_path: HashMap<PathBuf, SharedString> =
        [(PathBuf::from("/projects/myapp"), "feature-x".into())]
            .into_iter()
            .collect();

    let infos = worktree_info_from_thread_paths(&worktree_paths, &branch_by_path);
    assert_eq!(infos.len(), 1);
    assert_eq!(infos[0].kind, ui::WorktreeKind::Main);
    assert_eq!(infos[0].branch_name, Some(SharedString::from("feature-x")));
    assert_eq!(infos[0].worktree_name, Some(SharedString::from("myapp")));
}

#[test]
fn test_worktree_info_branch_names_for_linked_worktrees() {
    let main_paths = PathList::new(&[PathBuf::from("/projects/myapp")]);
    let folder_paths = PathList::new(&[PathBuf::from("/projects/myapp-feature")]);
    let worktree_paths =
        WorktreePaths::from_path_lists(main_paths, folder_paths).expect("same length");

    let branch_by_path: HashMap<PathBuf, SharedString> = [(
        PathBuf::from("/projects/myapp-feature"),
        "feature-branch".into(),
    )]
    .into_iter()
    .collect();

    let infos = worktree_info_from_thread_paths(&worktree_paths, &branch_by_path);
    assert_eq!(infos.len(), 1);
    assert_eq!(infos[0].kind, ui::WorktreeKind::Linked);
    assert_eq!(
        infos[0].branch_name,
        Some(SharedString::from("feature-branch"))
    );
}

#[test]
fn test_worktree_info_missing_branch_returns_none() {
    let folder_paths = PathList::new(&[PathBuf::from("/projects/myapp")]);
    let worktree_paths = WorktreePaths::from_folder_paths(&folder_paths);

    let branch_by_path: HashMap<PathBuf, SharedString> = HashMap::new();

    let infos = worktree_info_from_thread_paths(&worktree_paths, &branch_by_path);
    assert_eq!(infos.len(), 1);
    assert_eq!(infos[0].kind, ui::WorktreeKind::Main);
    assert_eq!(infos[0].branch_name, None);
    assert_eq!(infos[0].worktree_name, Some(SharedString::from("myapp")));
}
