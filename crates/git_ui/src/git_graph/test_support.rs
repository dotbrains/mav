use super::*;

#[cfg(any(test, feature = "test-support"))]
impl GitGraph {
    pub fn search_for_test(&mut self, query: SharedString, cx: &mut Context<Self>) {
        self.search(query, cx);
    }

    pub fn search_matches_for_test(&self) -> Vec<Oid> {
        self.search_state.matches.iter().copied().collect()
    }

    pub fn initial_commit_data_for_test(&self) -> Vec<Arc<InitialGraphCommitData>> {
        self.graph_data
            .commits
            .iter()
            .map(|commit| commit.data.clone())
            .collect()
    }

    pub fn commit_count_and_loading_state_for_test(
        &mut self,
        cx: &mut Context<Self>,
    ) -> (usize, bool) {
        self.commit_count_and_loading_state(cx)
    }

    pub fn log_source_for_test(&self) -> &LogSource {
        &self.log_source
    }
}
