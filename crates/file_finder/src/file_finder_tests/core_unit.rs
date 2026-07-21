use super::*;

#[test]
fn test_path_elision() {
    #[track_caller]
    fn check(path: &str, budget: usize, matches: impl IntoIterator<Item = usize>, expected: &str) {
        let mut path = path.to_owned();
        let slice = PathComponentSlice::new(&path);
        let matches = Vec::from_iter(matches);
        if let Some(range) = slice.elision_range(budget - 1, &matches) {
            path.replace_range(range, "…");
        }
        assert_eq!(path, expected);
    }

    // Simple cases, mostly to check that different path shapes are handled gracefully.
    check("p/a/b/c/d/", 6, [], "p/…/d/");
    check("p/a/b/c/d/", 1, [2, 4, 6], "p/a/b/c/d/");
    check("p/a/b/c/d/", 10, [2, 6], "p/a/…/c/d/");
    check("p/a/b/c/d/", 8, [6], "p/…/c/d/");

    check("p/a/b/c/d", 5, [], "p/…/d");
    check("p/a/b/c/d", 9, [2, 4, 6], "p/a/b/c/d");
    check("p/a/b/c/d", 9, [2, 6], "p/a/…/c/d");
    check("p/a/b/c/d", 7, [6], "p/…/c/d");

    check("/p/a/b/c/d/", 7, [], "/p/…/d/");
    check("/p/a/b/c/d/", 11, [3, 5, 7], "/p/a/b/c/d/");
    check("/p/a/b/c/d/", 11, [3, 7], "/p/a/…/c/d/");
    check("/p/a/b/c/d/", 9, [7], "/p/…/c/d/");

    // If the budget can't be met, no elision is done.
    check(
        "project/dir/child/grandchild",
        5,
        [],
        "project/dir/child/grandchild",
    );

    // The longest unmatched segment is picked for elision.
    check(
        "project/one/two/X/three/sub",
        21,
        [16],
        "project/…/X/three/sub",
    );

    // Elision stops when the budget is met, even though there are more components in the chosen segment.
    // It proceeds from the end of the unmatched segment that is closer to the midpoint of the path.
    check(
        "project/one/two/three/X/sub",
        21,
        [22],
        "project/…/three/X/sub",
    )
}

#[test]
fn test_custom_project_search_ordering_in_file_finder() {
    let mut file_finder_sorted_output = vec![
        ProjectPanelOrdMatch(PathMatch {
            score: 0.5,
            positions: Vec::new(),
            worktree_id: 0,
            path: rel_path("b0.5").into(),
            path_prefix: rel_path("").into(),
            distance_to_relative_ancestor: 0,
            is_dir: false,
        }),
        ProjectPanelOrdMatch(PathMatch {
            score: 1.0,
            positions: Vec::new(),
            worktree_id: 0,
            path: rel_path("c1.0").into(),
            path_prefix: rel_path("").into(),
            distance_to_relative_ancestor: 0,
            is_dir: false,
        }),
        ProjectPanelOrdMatch(PathMatch {
            score: 1.0,
            positions: Vec::new(),
            worktree_id: 0,
            path: rel_path("a1.0").into(),
            path_prefix: rel_path("").into(),
            distance_to_relative_ancestor: 0,
            is_dir: false,
        }),
        ProjectPanelOrdMatch(PathMatch {
            score: 0.5,
            positions: Vec::new(),
            worktree_id: 0,
            path: rel_path("a0.5").into(),
            path_prefix: rel_path("").into(),
            distance_to_relative_ancestor: 0,
            is_dir: false,
        }),
        ProjectPanelOrdMatch(PathMatch {
            score: 1.0,
            positions: Vec::new(),
            worktree_id: 0,
            path: rel_path("b1.0").into(),
            path_prefix: rel_path("").into(),
            distance_to_relative_ancestor: 0,
            is_dir: false,
        }),
    ];
    file_finder_sorted_output.sort_by(|a, b| b.cmp(a));

    assert_eq!(
        file_finder_sorted_output,
        vec![
            ProjectPanelOrdMatch(PathMatch {
                score: 1.0,
                positions: Vec::new(),
                worktree_id: 0,
                path: rel_path("a1.0").into(),
                path_prefix: rel_path("").into(),
                distance_to_relative_ancestor: 0,
                is_dir: false,
            }),
            ProjectPanelOrdMatch(PathMatch {
                score: 1.0,
                positions: Vec::new(),
                worktree_id: 0,
                path: rel_path("b1.0").into(),
                path_prefix: rel_path("").into(),
                distance_to_relative_ancestor: 0,
                is_dir: false,
            }),
            ProjectPanelOrdMatch(PathMatch {
                score: 1.0,
                positions: Vec::new(),
                worktree_id: 0,
                path: rel_path("c1.0").into(),
                path_prefix: rel_path("").into(),
                distance_to_relative_ancestor: 0,
                is_dir: false,
            }),
            ProjectPanelOrdMatch(PathMatch {
                score: 0.5,
                positions: Vec::new(),
                worktree_id: 0,
                path: rel_path("a0.5").into(),
                path_prefix: rel_path("").into(),
                distance_to_relative_ancestor: 0,
                is_dir: false,
            }),
            ProjectPanelOrdMatch(PathMatch {
                score: 0.5,
                positions: Vec::new(),
                worktree_id: 0,
                path: rel_path("b0.5").into(),
                path_prefix: rel_path("").into(),
                distance_to_relative_ancestor: 0,
                is_dir: false,
            }),
        ]
    );
}
