use super::*;

#[cfg(any(test, feature = "test-support"))]
impl Repository {
    pub fn loaded_commit_data_for_test(&self) -> HashMap<Oid, CommitData> {
        self.commit_data
            .iter()
            .filter_map(|(sha, state)| match state {
                CommitDataState::Loaded(data) => Some((*sha, data.as_ref().clone())),
                CommitDataState::Loading(_) => None,
            })
            .collect()
    }
}
