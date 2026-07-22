use super::*;

// ── ThreadWorktreePaths tests ──────────────────────────────────────

/// Helper to build a `ThreadWorktreePaths` from (main, folder) pairs.

fn make_worktree_paths(pairs: &[(&str, &str)]) -> WorktreePaths {
    let (mains, folders): (Vec<&Path>, Vec<&Path>) = pairs
        .iter()
        .map(|(m, f)| (Path::new(*m), Path::new(*f)))
        .unzip();
    WorktreePaths::from_path_lists(PathList::new(&mains), PathList::new(&folders)).unwrap()
}

#[test]
fn test_thread_worktree_paths_full_add_then_remove_cycle() {
    // Full scenario from the issue:
    //   1. Start with linked worktree selectric → mav
    //   2. Add cloud
    //   3. Remove mav

    let mut paths = make_worktree_paths(&[("/projects/mav", "/worktrees/selectric/mav")]);

    // Step 2: add cloud
    paths.add_path(Path::new("/projects/cloud"), Path::new("/projects/cloud"));

    assert_eq!(paths.ordered_pairs().count(), 2);
    assert_eq!(
        paths.folder_path_list(),
        &PathList::new(&[
            Path::new("/worktrees/selectric/mav"),
            Path::new("/projects/cloud"),
        ])
    );
    assert_eq!(
        paths.main_worktree_path_list(),
        &PathList::new(&[Path::new("/projects/mav"), Path::new("/projects/cloud"),])
    );

    // Step 3: remove mav
    paths.remove_main_path(Path::new("/projects/mav"));

    assert_eq!(paths.ordered_pairs().count(), 1);
    assert_eq!(
        paths.folder_path_list(),
        &PathList::new(&[Path::new("/projects/cloud")])
    );
    assert_eq!(
        paths.main_worktree_path_list(),
        &PathList::new(&[Path::new("/projects/cloud")])
    );
}

#[test]
fn test_thread_worktree_paths_add_is_idempotent() {
    let mut paths = make_worktree_paths(&[("/projects/mav", "/projects/mav")]);

    paths.add_path(Path::new("/projects/mav"), Path::new("/projects/mav"));

    assert_eq!(paths.ordered_pairs().count(), 1);
}

#[test]
fn test_thread_worktree_paths_remove_nonexistent_is_noop() {
    let mut paths = make_worktree_paths(&[("/projects/mav", "/worktrees/selectric/mav")]);

    paths.remove_main_path(Path::new("/projects/nonexistent"));

    assert_eq!(paths.ordered_pairs().count(), 1);
}

#[test]
fn test_thread_worktree_paths_from_path_lists_preserves_association() {
    let folder = PathList::new(&[
        Path::new("/worktrees/selectric/mav"),
        Path::new("/projects/cloud"),
    ]);
    let main = PathList::new(&[Path::new("/projects/mav"), Path::new("/projects/cloud")]);

    let paths = WorktreePaths::from_path_lists(main, folder).unwrap();

    let pairs: Vec<_> = paths
        .ordered_pairs()
        .map(|(m, f)| (m.clone(), f.clone()))
        .collect();
    assert_eq!(pairs.len(), 2);
    assert!(pairs.contains(&(
        PathBuf::from("/projects/mav"),
        PathBuf::from("/worktrees/selectric/mav")
    )));
    assert!(pairs.contains(&(
        PathBuf::from("/projects/cloud"),
        PathBuf::from("/projects/cloud")
    )));
}

#[test]
fn test_thread_worktree_paths_main_deduplicates_linked_worktrees() {
    // Two linked worktrees of the same main repo: the main_worktree_path_list
    // deduplicates because PathList stores unique sorted paths, but
    // ordered_pairs still has both entries.
    let paths = make_worktree_paths(&[
        ("/projects/mav", "/worktrees/selectric/mav"),
        ("/projects/mav", "/worktrees/feature/mav"),
    ]);

    // main_worktree_path_list has the duplicate main path twice
    // (PathList keeps all entries from its input)
    assert_eq!(paths.ordered_pairs().count(), 2);
    assert_eq!(
        paths.folder_path_list(),
        &PathList::new(&[
            Path::new("/worktrees/selectric/mav"),
            Path::new("/worktrees/feature/mav"),
        ])
    );
    assert_eq!(
        paths.main_worktree_path_list(),
        &PathList::new(&[Path::new("/projects/mav"), Path::new("/projects/mav"),])
    );
}

#[test]
fn test_thread_worktree_paths_mismatched_lengths_returns_error() {
    let folder = PathList::new(&[
        Path::new("/worktrees/selectric/mav"),
        Path::new("/projects/cloud"),
    ]);
    let main = PathList::new(&[Path::new("/projects/mav")]);

    let result = WorktreePaths::from_path_lists(main, folder);
    assert!(result.is_err());
}
