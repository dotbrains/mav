use super::*;

#[test]
fn test_git_graph_merge_commits() {
    let mut rng = StdRng::seed_from_u64(42);

    let oid1 = Oid::random(&mut rng);
    let oid2 = Oid::random(&mut rng);
    let oid3 = Oid::random(&mut rng);
    let oid4 = Oid::random(&mut rng);

    let commits = vec![
        Arc::new(InitialGraphCommitData {
            sha: oid1,
            parents: smallvec![oid2, oid3],
            ref_names: vec!["HEAD".into()],
        }),
        Arc::new(InitialGraphCommitData {
            sha: oid2,
            parents: smallvec![oid4],
            ref_names: vec![],
        }),
        Arc::new(InitialGraphCommitData {
            sha: oid3,
            parents: smallvec![oid4],
            ref_names: vec![],
        }),
        Arc::new(InitialGraphCommitData {
            sha: oid4,
            parents: smallvec![],
            ref_names: vec![],
        }),
    ];

    let mut graph_data = GraphData::new(8);
    graph_data.add_commits(&commits);

    if let Err(error) = verify_all_invariants(&graph_data, &commits) {
        panic!("Graph invariant violation for merge commits:\n{}", error);
    }
}

#[test]
fn test_git_graph_linear_commits() {
    let mut rng = StdRng::seed_from_u64(42);

    let oid1 = Oid::random(&mut rng);
    let oid2 = Oid::random(&mut rng);
    let oid3 = Oid::random(&mut rng);

    let commits = vec![
        Arc::new(InitialGraphCommitData {
            sha: oid1,
            parents: smallvec![oid2],
            ref_names: vec!["HEAD".into()],
        }),
        Arc::new(InitialGraphCommitData {
            sha: oid2,
            parents: smallvec![oid3],
            ref_names: vec![],
        }),
        Arc::new(InitialGraphCommitData {
            sha: oid3,
            parents: smallvec![],
            ref_names: vec![],
        }),
    ];

    let mut graph_data = GraphData::new(8);
    graph_data.add_commits(&commits);

    if let Err(error) = verify_all_invariants(&graph_data, &commits) {
        panic!("Graph invariant violation for linear commits:\n{}", error);
    }
}

#[test]
fn test_git_graph_random_commits() {
    for seed in 0..100 {
        let mut rng = StdRng::seed_from_u64(seed);

        let adversarial = rng.random_bool(0.2);
        let num_commits = if adversarial {
            rng.random_range(10..100)
        } else {
            rng.random_range(5..50)
        };

        let commits = generate_random_commit_dag(&mut rng, num_commits, adversarial);

        assert_eq!(
            num_commits,
            commits.len(),
            "seed={}: Generate random commit dag didn't generate the correct amount of commits",
            seed
        );

        let mut graph_data = GraphData::new(8);
        graph_data.add_commits(&commits);

        if let Err(error) = verify_all_invariants(&graph_data, &commits) {
            panic!(
                "Graph invariant violation (seed={}, adversarial={}, num_commits={}):\n{:#}",
                seed, adversarial, num_commits, error
            );
        }
    }
}

// The full integration test has less iterations because it's significantly slower
// than the random commit test
#[gpui::test(iterations = 10)]
async fn test_git_graph_random_integration(mut rng: StdRng, cx: &mut TestAppContext) {
    init_test(cx);

    let adversarial = rng.random_bool(0.2);
    let num_commits = if adversarial {
        rng.random_range(10..100)
    } else {
        rng.random_range(5..50)
    };

    let commits = generate_random_commit_dag(&mut rng, num_commits, adversarial);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        Path::new("/project"),
        json!({
            ".git": {},
            "file.txt": "content",
        }),
    )
    .await;

    fs.set_graph_commits(Path::new("/project/.git"), commits.clone());

    let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;
    cx.run_until_parked();

    let repository = project.read_with(cx, |project, cx| {
        project
            .active_repository(cx)
            .expect("should have a repository")
    });

    repository.update(cx, |repo, cx| {
        repo.graph_data(LogSource::default(), LogOrder::default(), 0..usize::MAX, cx);
    });
    cx.run_until_parked();

    let graph_commits: Vec<Arc<InitialGraphCommitData>> = repository.update(cx, |repo, cx| {
        repo.graph_data(LogSource::default(), LogOrder::default(), 0..usize::MAX, cx)
            .commits
            .to_vec()
    });

    let mut graph_data = GraphData::new(8);
    graph_data.add_commits(&graph_commits);

    if let Err(error) = verify_all_invariants(&graph_data, &commits) {
        panic!(
            "Graph invariant violation (adversarial={}, num_commits={}):\n{:#}",
            adversarial, num_commits, error
        );
    }
}
