use super::*;

#[test]
fn test_initial_graph_commit_data_tag_names() {
    let commit = InitialGraphCommitData {
        sha: Oid::from_bytes(&[0; 20]).unwrap(),
        parents: SmallVec::new(),
        ref_names: vec![
            SharedString::from("HEAD -> main"),
            SharedString::from("origin/main"),
            SharedString::from("tag: v1.0.0"),
            SharedString::from("tag: v1.1.0"),
            SharedString::from("tag: "),
            SharedString::from("refs/heads/feature"),
        ],
    };

    assert_eq!(commit.tag_names(), ["v1.0.0", "v1.1.0"]);
}

#[test]
fn test_parse_file_history_changed_files_output() {
    let queried_paths = vec![
        RepoPath::new("src/a.rs").unwrap(),
        RepoPath::new("src/b.rs").unwrap(),
    ];
    let output = concat!(
        "\x1e\0\nsrc/a.rs\0src/shared.rs\0",
        "\x1e\0\nsrc/b.rs\0src/shared.rs\0",
        "\x1e\0\nsrc/a.rs\0src/b.rs\0src/shared.rs\0",
    );

    let histories = parse_file_history_changed_files_output(output, &queried_paths);

    assert_eq!(histories.len(), 2);
    assert_eq!(
        histories[0].file_sets,
        vec![
            vec![
                RepoPath::new("src/a.rs").unwrap(),
                RepoPath::new("src/shared.rs").unwrap(),
            ],
            vec![
                RepoPath::new("src/a.rs").unwrap(),
                RepoPath::new("src/b.rs").unwrap(),
                RepoPath::new("src/shared.rs").unwrap(),
            ],
        ]
    );
    assert_eq!(
        histories[1].file_sets,
        vec![
            vec![
                RepoPath::new("src/b.rs").unwrap(),
                RepoPath::new("src/shared.rs").unwrap(),
            ],
            vec![
                RepoPath::new("src/a.rs").unwrap(),
                RepoPath::new("src/b.rs").unwrap(),
                RepoPath::new("src/shared.rs").unwrap(),
            ],
        ]
    );
}

#[gpui::test]
async fn test_branches_return_head_when_commit_metadata_cannot_be_read(cx: &mut TestAppContext) {
    disable_git_global_config();

    cx.executor().allow_parking();

    let repo_dir = tempfile::tempdir().unwrap();
    git_init_repo(repo_dir.path());
    let repo = RealGitRepository::new(
        &repo_dir.path().join(".git"),
        None,
        Some("git".into()),
        cx.executor(),
    )
    .unwrap();

    smol::fs::write(repo_dir.path().join("file.txt"), "content")
        .await
        .unwrap();
    repo.stage_paths(vec![repo_path("file.txt")], Arc::new(HashMap::default()))
        .await
        .unwrap();
    repo.commit(
        "Initial commit".into(),
        None,
        CommitOptions::default(),
        AskPassDelegate::new(&mut cx.to_async(), |_, _, _| {}),
        Arc::new(test_commit_envs()),
    )
    .await
    .unwrap();

    smol::fs::write(
        repo_dir.path().join(".git").join("refs/heads/broken"),
        "0a103ede22f159c792dc6405e0c8304d9bd4dc29\n",
    )
    .await
    .unwrap();

    let branches_scan = repo.branches().await.unwrap();
    assert!(branches_scan.error.is_some());
    let head_branch = branches_scan
        .branches
        .iter()
        .find(|branch| branch.is_head)
        .expect("branch list should include HEAD");
    assert!(head_branch.ref_name.starts_with("refs/heads/"));

    assert!(
        branches_scan
            .branches
            .iter()
            .all(|branch| branch.ref_name.as_ref() != "refs/heads/broken")
    );
}

#[gpui::test]
async fn test_compare_checkpoints(cx: &mut TestAppContext) {
    disable_git_global_config();

    cx.executor().allow_parking();

    let repo_dir = tempfile::tempdir().unwrap();
    git_init_repo(repo_dir.path());
    let repo = RealGitRepository::new(
        &repo_dir.path().join(".git"),
        None,
        Some("git".into()),
        cx.executor(),
    )
    .unwrap();

    smol::fs::write(repo_dir.path().join("file1"), "content1")
        .await
        .unwrap();
    let checkpoint1 = repo.checkpoint().await.unwrap();

    smol::fs::write(repo_dir.path().join("file2"), "content2")
        .await
        .unwrap();
    let checkpoint2 = repo.checkpoint().await.unwrap();

    assert!(
        !repo
            .compare_checkpoints(checkpoint1, checkpoint2.clone())
            .await
            .unwrap()
    );

    let checkpoint3 = repo.checkpoint().await.unwrap();
    assert!(
        repo.compare_checkpoints(checkpoint2, checkpoint3)
            .await
            .unwrap()
    );
}

#[gpui::test]
async fn test_checkpoint_exclude_binary_files(cx: &mut TestAppContext) {
    disable_git_global_config();

    cx.executor().allow_parking();

    let repo_dir = tempfile::tempdir().unwrap();
    let text_path = repo_dir.path().join("main.rs");
    let bin_path = repo_dir.path().join("binary.o");

    git_init_repo(repo_dir.path());

    smol::fs::write(&text_path, "fn main() {}").await.unwrap();

    smol::fs::write(&bin_path, "some binary file here")
        .await
        .unwrap();

    let repo = RealGitRepository::new(
        &repo_dir.path().join(".git"),
        None,
        Some("git".into()),
        cx.executor(),
    )
    .unwrap();

    // initial commit
    repo.stage_paths(vec![repo_path("main.rs")], Arc::new(HashMap::default()))
        .await
        .unwrap();
    repo.commit(
        "Initial commit".into(),
        None,
        CommitOptions::default(),
        AskPassDelegate::new(&mut cx.to_async(), |_, _, _| {}),
        Arc::new(test_commit_envs()),
    )
    .await
    .unwrap();

    let checkpoint = repo.checkpoint().await.unwrap();

    smol::fs::write(&text_path, "fn main() { println!(\"Modified\"); }")
        .await
        .unwrap();
    smol::fs::write(&bin_path, "Modified binary file")
        .await
        .unwrap();

    repo.restore_checkpoint(checkpoint).await.unwrap();

    // Text files should be restored to checkpoint state,
    // but binaries should not (they aren't tracked)
    assert_eq!(
        smol::fs::read_to_string(&text_path).await.unwrap(),
        "fn main() {}"
    );

    assert_eq!(
        smol::fs::read_to_string(&bin_path).await.unwrap(),
        "Modified binary file"
    );
}

#[test]
fn test_branches_parsing() {
    // suppress "help: octal escapes are not supported, `\0` is always null"
    #[allow(clippy::octal_escapes)]
    let input = "*\0060964da10574cd9bf06463a53bf6e0769c5c45e\0\0refs/heads/mav-patches\0refs/remotes/origin/mav-patches\0\01733187470\0John Doe\0generated protobuf\n";
    assert_eq!(
        parse_branch_input(input).unwrap(),
        vec![Branch {
            is_head: true,
            ref_name: "refs/heads/mav-patches".into(),
            upstream: Some(Upstream {
                ref_name: "refs/remotes/origin/mav-patches".into(),
                tracking: UpstreamTracking::Tracked(UpstreamTrackingStatus {
                    ahead: 0,
                    behind: 0
                })
            }),
            most_recent_commit: Some(CommitSummary {
                sha: "060964da10574cd9bf06463a53bf6e0769c5c45e".into(),
                subject: "generated protobuf".into(),
                commit_timestamp: 1733187470,
                author_name: SharedString::new_static("John Doe"),
                has_parent: false,
            })
        }]
    )
}

#[test]
fn test_branches_parsing_containing_refs_with_missing_fields() {
    #[allow(clippy::octal_escapes)]
    let input = " \090012116c03db04344ab10d50348553aa94f1ea0\0refs/heads/broken\n \0eb0cae33272689bd11030822939dd2701c52f81e\0895951d681e5561478c0acdd6905e8aacdfd2249\0refs/heads/dev\0\0\01762948725\0Mav\0Add feature\n*\0895951d681e5561478c0acdd6905e8aacdfd2249\0\0refs/heads/main\0\0\01762948695\0Mav\0Initial commit\n";

    let branches = parse_branch_input(input).unwrap();
    assert_eq!(branches.len(), 2);
    assert_eq!(
        branches,
        vec![
            Branch {
                is_head: false,
                ref_name: "refs/heads/dev".into(),
                upstream: None,
                most_recent_commit: Some(CommitSummary {
                    sha: "eb0cae33272689bd11030822939dd2701c52f81e".into(),
                    subject: "Add feature".into(),
                    commit_timestamp: 1762948725,
                    author_name: SharedString::new_static("Mav"),
                    has_parent: true,
                })
            },
            Branch {
                is_head: true,
                ref_name: "refs/heads/main".into(),
                upstream: None,
                most_recent_commit: Some(CommitSummary {
                    sha: "895951d681e5561478c0acdd6905e8aacdfd2249".into(),
                    subject: "Initial commit".into(),
                    commit_timestamp: 1762948695,
                    author_name: SharedString::new_static("Mav"),
                    has_parent: false,
                })
            }
        ]
    )
}

#[test]
fn test_upstream_branch_name() {
    let upstream = Upstream {
        ref_name: "refs/remotes/origin/feature/branch".into(),
        tracking: UpstreamTracking::Tracked(UpstreamTrackingStatus {
            ahead: 0,
            behind: 0,
        }),
    };
    assert_eq!(upstream.branch_name(), Some("feature/branch"));

    let upstream = Upstream {
        ref_name: "refs/remotes/upstream/main".into(),
        tracking: UpstreamTracking::Tracked(UpstreamTrackingStatus {
            ahead: 0,
            behind: 0,
        }),
    };
    assert_eq!(upstream.branch_name(), Some("main"));

    let upstream = Upstream {
        ref_name: "refs/heads/local".into(),
        tracking: UpstreamTracking::Tracked(UpstreamTrackingStatus {
            ahead: 0,
            behind: 0,
        }),
    };
    assert_eq!(upstream.branch_name(), None);

    // Test case where upstream branch name differs from what might be the local branch name
    let upstream = Upstream {
        ref_name: "refs/remotes/origin/feature/git-pull-request".into(),
        tracking: UpstreamTracking::Tracked(UpstreamTrackingStatus {
            ahead: 0,
            behind: 0,
        }),
    };
    assert_eq!(upstream.branch_name(), Some("feature/git-pull-request"));
}
