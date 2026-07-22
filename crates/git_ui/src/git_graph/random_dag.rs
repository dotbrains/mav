use git::{Oid, repository::InitialGraphCommitData};
use smallvec::{SmallVec, smallvec};
use std::sync::Arc;

/// Generates a random commit DAG suitable for testing git graph rendering.
///
/// The commits are ordered newest-first (like git log output), so:
/// - Index 0 = most recent commit (HEAD)
/// - Last index = oldest commit (root, has no parents)
/// - Parents of commit at index I must have index > I
///
/// When `adversarial` is true, generates complex topologies with many branches
/// and octopus merges. Otherwise generates more realistic linear histories
/// with occasional branches.
pub fn generate_random_commit_dag(
    rng: &mut rand::rngs::StdRng,
    num_commits: usize,
    adversarial: bool,
) -> Vec<Arc<InitialGraphCommitData>> {
    use rand::Rng as _;

    if num_commits == 0 {
        return Vec::new();
    }

    let mut commits: Vec<Arc<InitialGraphCommitData>> = Vec::with_capacity(num_commits);
    let oids: Vec<Oid> = (0..num_commits).map(|_| Oid::random(rng)).collect();

    for i in 0..num_commits {
        let sha = oids[i];

        let parents = if i == num_commits - 1 {
            smallvec![]
        } else {
            generate_parents_from_oids(rng, &oids, i, num_commits, adversarial)
        };

        let ref_names = if i == 0 {
            vec!["HEAD".into(), "main".into()]
        } else if adversarial && rng.random_bool(0.1) {
            vec![format!("branch-{i}").into()]
        } else {
            Vec::new()
        };

        commits.push(Arc::new(InitialGraphCommitData {
            sha,
            parents,
            ref_names,
        }));
    }

    commits
}

fn generate_parents_from_oids(
    rng: &mut rand::rngs::StdRng,
    oids: &[Oid],
    current_idx: usize,
    num_commits: usize,
    adversarial: bool,
) -> SmallVec<[Oid; 1]> {
    use rand::{Rng as _, seq::SliceRandom as _};

    let remaining = num_commits - current_idx - 1;
    if remaining == 0 {
        return smallvec![];
    }

    if adversarial {
        let merge_chance = 0.4;
        let octopus_chance = 0.15;

        if remaining >= 3 && rng.random_bool(octopus_chance) {
            let num_parents = rng.random_range(3..=remaining.min(5));
            let mut parent_indices: Vec<usize> = (current_idx + 1..num_commits).collect();
            parent_indices.shuffle(rng);
            parent_indices
                .into_iter()
                .take(num_parents)
                .map(|idx| oids[idx])
                .collect()
        } else if remaining >= 2 && rng.random_bool(merge_chance) {
            let mut parent_indices: Vec<usize> = (current_idx + 1..num_commits).collect();
            parent_indices.shuffle(rng);
            parent_indices
                .into_iter()
                .take(2)
                .map(|idx| oids[idx])
                .collect()
        } else {
            let parent_idx = rng.random_range(current_idx + 1..num_commits);
            smallvec![oids[parent_idx]]
        }
    } else {
        let merge_chance = 0.15;
        let skip_chance = 0.1;

        if remaining >= 2 && rng.random_bool(merge_chance) {
            let first_parent = current_idx + 1;
            let second_parent = rng.random_range(current_idx + 2..num_commits);
            smallvec![oids[first_parent], oids[second_parent]]
        } else if rng.random_bool(skip_chance) && remaining >= 2 {
            let skip = rng.random_range(1..remaining.min(3));
            smallvec![oids[current_idx + 1 + skip]]
        } else {
            smallvec![oids[current_idx + 1]]
        }
    }
}
