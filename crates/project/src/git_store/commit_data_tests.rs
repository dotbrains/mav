use super::*;
use crate::Project;
use fs::FakeFs;
use git::repository::RepoPath;
use gpui::TestAppContext;
use gpui::proptest::prelude::*;
use rand::{SeedableRng, rngs::StdRng};
use serde_json::json;
use settings::{Settings, SettingsStore};
use std::path::Path;

fn init_test(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
    });
}

fn verify_invariants(repository: &Repository) -> anyhow::Result<()> {
    match &repository.commit_data_handler {
        CommitDataHandlerState::Open(handler) => {
            verify_loading_entries_are_pending(repository, handler)?;
            verify_await_result_loading_entries_have_completion_senders(repository, handler)?;
            verify_pending_requests_are_loading(repository, handler)?;
            verify_completion_senders_are_await_result_loading(repository, handler)?;
            verify_completion_senders_are_pending(handler)?;
            verify_non_await_result_loading_entries_have_no_completion_sender(repository, handler)?;
            verify_loaded_entries_are_not_pending(repository, handler)?;
            verify_loaded_entries_have_no_completion_sender(repository, handler)?;
        }
        CommitDataHandlerState::Closed => {
            verify_closed_handler_invariants(repository)?;
        }
    }

    Ok(())
}

fn verify_loading_entries_are_pending(
    repository: &Repository,
    handler: &CommitDataHandler,
) -> anyhow::Result<()> {
    for (sha, state) in &repository.commit_data {
        if matches!(state, CommitDataState::Loading(_)) {
            anyhow::ensure!(
                handler.pending_requests.contains(sha),
                "loading commit data for {sha} must be tracked in pending_requests"
            );
        }
    }

    Ok(())
}

fn verify_await_result_loading_entries_have_completion_senders(
    repository: &Repository,
    handler: &CommitDataHandler,
) -> anyhow::Result<()> {
    for (sha, state) in &repository.commit_data {
        if matches!(state, CommitDataState::Loading(Some(_))) {
            anyhow::ensure!(
                handler.completion_senders.contains_key(sha),
                "await-result loading commit data for {sha} must have a completion sender"
            );
        }
    }

    Ok(())
}

fn verify_pending_requests_are_loading(
    repository: &Repository,
    handler: &CommitDataHandler,
) -> anyhow::Result<()> {
    for sha in &handler.pending_requests {
        anyhow::ensure!(
            matches!(
                repository.commit_data.get(sha),
                Some(CommitDataState::Loading(_))
            ),
            "pending request for {sha} must correspond to loading commit data"
        );
    }

    Ok(())
}

fn verify_completion_senders_are_await_result_loading(
    repository: &Repository,
    handler: &CommitDataHandler,
) -> anyhow::Result<()> {
    for sha in handler.completion_senders.keys() {
        anyhow::ensure!(
            matches!(
                repository.commit_data.get(sha),
                Some(CommitDataState::Loading(Some(_)))
            ),
            "completion sender for {sha} must correspond to await-result loading commit data"
        );
    }

    Ok(())
}

fn verify_completion_senders_are_pending(handler: &CommitDataHandler) -> anyhow::Result<()> {
    for sha in handler.completion_senders.keys() {
        anyhow::ensure!(
            handler.pending_requests.contains(sha),
            "completion sender for {sha} must also be tracked as pending"
        );
    }

    Ok(())
}

fn verify_non_await_result_loading_entries_have_no_completion_sender(
    repository: &Repository,
    handler: &CommitDataHandler,
) -> anyhow::Result<()> {
    for (sha, state) in &repository.commit_data {
        if matches!(state, CommitDataState::Loading(None)) {
            anyhow::ensure!(
                !handler.completion_senders.contains_key(sha),
                "non-await-result loading commit data for {sha} must not have a completion sender"
            );
        }
    }

    Ok(())
}

fn verify_loaded_entries_are_not_pending(
    repository: &Repository,
    handler: &CommitDataHandler,
) -> anyhow::Result<()> {
    for (sha, state) in &repository.commit_data {
        if matches!(state, CommitDataState::Loaded(_)) {
            anyhow::ensure!(
                !handler.pending_requests.contains(sha),
                "loaded commit data for {sha} must not still be pending"
            );
        }
    }

    Ok(())
}

fn verify_loaded_entries_have_no_completion_sender(
    repository: &Repository,
    handler: &CommitDataHandler,
) -> anyhow::Result<()> {
    for (sha, state) in &repository.commit_data {
        if matches!(state, CommitDataState::Loaded(_)) {
            anyhow::ensure!(
                !handler.completion_senders.contains_key(sha),
                "loaded commit data for {sha} must not keep a completion sender"
            );
        }
    }

    Ok(())
}

fn verify_closed_handler_invariants(repository: &Repository) -> anyhow::Result<()> {
    for (sha, state) in &repository.commit_data {
        anyhow::ensure!(
            !matches!(state, CommitDataState::Loading(_)),
            "closed handler must not keep loading commit data for {sha}"
        );
    }

    Ok(())
}

#[gpui::property_test(config = ProptestConfig {
    cases: 20,
    ..Default::default()
})]
async fn test_commit_data_random_invariants(
    #[strategy = any::<u64>()] seed: u64,
    #[strategy = gpui::proptest::collection::vec(0usize..2000, 1..200)] commit_indexes: Vec<usize>,
    #[strategy = gpui::proptest::collection::vec(any::<bool>(), 1..200)] await_results: Vec<bool>,
    #[strategy = gpui::proptest::collection::vec(0usize..2000, 0..200)] failing_commit_indexes: Vec<
        usize,
    >,
    #[strategy = gpui::proptest::collection::vec(0usize..2000, 0..200)] missing_commit_indexes: Vec<
        usize,
    >,
    cx: &mut TestAppContext,
) {
    init_test(cx);
    let mut rng = StdRng::seed_from_u64(seed);

    let commit_shas = (0..2000).map(|_| Oid::random(&mut rng)).collect::<Vec<_>>();
    let failing_shas = failing_commit_indexes
        .into_iter()
        .map(|index| commit_shas[index % commit_shas.len()])
        .collect::<HashSet<_>>();
    let missing_shas = missing_commit_indexes
        .into_iter()
        .map(|index| commit_shas[index % commit_shas.len()])
        .collect::<HashSet<_>>();
    let commit_data = commit_shas
        .iter()
        .filter(|sha| !missing_shas.contains(sha))
        .map(|sha| {
            (
                CommitData {
                    sha: *sha,
                    parents: SmallVec::new(),
                    author_name: SharedString::from(format!("Author {sha}")),
                    author_email: SharedString::from(format!("{sha}@example.com")),
                    commit_timestamp: rng.random_range(0..10_000),
                    subject: SharedString::from(format!("Subject {sha}")),
                    message: SharedString::from(format!("Subject {sha}\n\nBody for {sha}")),
                },
                failing_shas.contains(sha),
            )
        })
        .collect::<Vec<_>>();
    let expected_loaded_shas = commit_indexes
        .iter()
        .map(|index| commit_shas[index % commit_shas.len()])
        .filter(|sha| !failing_shas.contains(sha) && !missing_shas.contains(sha))
        .collect::<HashSet<_>>();

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        Path::new("/project"),
        json!({
            ".git": {},
            "file.txt": "content",
        }),
    )
    .await;
    fs.set_commit_data(Path::new("/project/.git"), commit_data);

    let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;

    let repository = project.read_with(cx, |project, cx| {
        project
            .active_repository(cx)
            .expect("should have a repository")
    });

    cx.update(|cx| {
        cx.observe(&repository, |repo, cx| {
            verify_invariants(repo.read(cx))
                .context("Invariant weren't held after a cx.notify")
                .unwrap();
        })
    })
    .detach();

    let mut next_step = 0;
    while next_step < commit_indexes.len() {
        let remaining_steps = commit_indexes.len() - next_step;
        let chunk_size = rng.random_range(1..=remaining_steps.min(16));
        let chunk_end = next_step + chunk_size;

        for step in next_step..chunk_end {
            let sha = commit_shas[commit_indexes[step] % commit_shas.len()];
            let await_result = await_results[step % await_results.len()];

            repository.update(cx, |repository, cx| {
                repository.fetch_commit_data(sha, await_result, cx);
                verify_invariants(repository)
                    .with_context(|| {
                        format!(
                            "commit data invariant violation after step {} for sha {}",
                            step + 1,
                            sha,
                        )
                    })
                    .unwrap();
            });
        }

        cx.run_until_parked();
        repository.read_with(cx, |repository, _cx| {
            verify_invariants(repository)
                .with_context(|| {
                    format!(
                        "commit data invariant violation after draining through step {}",
                        chunk_end,
                    )
                })
                .unwrap();
        });

        next_step = chunk_end;
    }

    cx.run_until_parked();
    repository.read_with(cx, |repository, _cx| {
        verify_invariants(repository)
            .with_context(|| "commit data invariant violation after final drain".to_string())
            .unwrap();

        let loaded_shas = repository
            .commit_data
            .iter()
            .filter_map(|(sha, state)| match state {
                CommitDataState::Loaded(_) => Some(*sha),
                CommitDataState::Loading(_) => None,
            })
            .collect::<HashSet<_>>();
        let missing_loaded_shas = expected_loaded_shas
            .difference(&loaded_shas)
            .copied()
            .collect::<Vec<_>>();
        let unexpected_loaded_shas = loaded_shas
            .difference(&expected_loaded_shas)
            .copied()
            .collect::<Vec<_>>();
        assert!(
            missing_loaded_shas.is_empty() && unexpected_loaded_shas.is_empty(),
            "loaded commit data SHAs after final drain did not match expectation. missing: {:?}, unexpected: {:?}",
            missing_loaded_shas,
            unexpected_loaded_shas,
        );
    });
}
