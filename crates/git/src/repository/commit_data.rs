use super::*;

/// Commit data needed for the git graph visualization.
#[derive(Debug, Clone)]
pub struct CommitData {
    pub sha: Oid,
    /// Most commits have a single parent, so we use a SmallVec to avoid allocations.
    pub parents: SmallVec<[Oid; 1]>,
    pub author_name: SharedString,
    pub author_email: SharedString,
    pub commit_timestamp: i64,
    pub subject: SharedString,
    pub message: SharedString,
}

#[derive(Debug)]
pub struct InitialGraphCommitData {
    pub sha: Oid,
    pub parents: SmallVec<[Oid; 1]>,
    pub ref_names: Vec<SharedString>,
}

impl InitialGraphCommitData {
    pub fn tag_names(&self) -> Vec<&str> {
        self.ref_names
            .iter()
            .filter_map(|ref_name| {
                let tag_name = ref_name.strip_prefix("tag: ")?;

                if tag_name.is_empty() {
                    return None;
                }

                Some(tag_name)
            })
            .collect()
    }
}

pub(super) struct CommitDataRequest {
    pub(super) sha: Oid,
    pub(super) response_tx: oneshot::Sender<Result<CommitData>>,
}

pub struct CommitDataReader {
    pub(super) request_tx: async_channel::Sender<CommitDataRequest>,
    pub(super) _task: Task<()>,
}

impl CommitDataReader {
    pub async fn read(&self, sha: Oid) -> Result<CommitData> {
        let (response_tx, response_rx) = oneshot::channel();
        self.request_tx
            .send(CommitDataRequest { sha, response_tx })
            .await
            .map_err(|_| anyhow!("commit data reader task closed"))?;
        response_rx
            .await
            .map_err(|_| anyhow!("commit data reader task dropped response"))?
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn for_test(
        executor: BackgroundExecutor,
        resolve: impl 'static + Send + Sync + Fn(Oid) -> Result<CommitData>,
    ) -> Self {
        let (request_tx, request_rx) = smol::channel::bounded::<CommitDataRequest>(64);
        let resolve = Arc::new(resolve);
        let delay_executor = executor.clone();
        let task = executor.spawn(async move {
            while let Ok(CommitDataRequest { sha, response_tx }) = request_rx.recv().await {
                delay_executor.simulate_random_delay().await;
                response_tx.send(resolve(sha)).ok();
            }
        });

        Self {
            request_tx,
            _task: task,
        }
    }
}

pub(super) fn parse_cat_file_commit(sha: Oid, content: &str) -> Option<CommitData> {
    let mut parents = SmallVec::new();
    let mut author_name = SharedString::default();
    let mut author_email = SharedString::default();
    let mut commit_timestamp = 0i64;
    let mut in_headers = true;
    let mut subject = None;
    let mut message_lines = Vec::new();

    for line in content.lines() {
        if in_headers {
            if line.is_empty() {
                in_headers = false;
                continue;
            }

            if let Some(parent_sha) = line.strip_prefix("parent ") {
                if let Ok(oid) = Oid::from_str(parent_sha.trim()) {
                    parents.push(oid);
                }
            } else if let Some(author_line) = line.strip_prefix("author ") {
                if let Some((name_email, _timestamp_tz)) = author_line.rsplit_once(' ') {
                    if let Some((name_email, timestamp_str)) = name_email.rsplit_once(' ') {
                        if let Ok(ts) = timestamp_str.parse::<i64>() {
                            commit_timestamp = ts;
                        }
                        if let Some((name, email)) = name_email.rsplit_once(" <") {
                            author_name = SharedString::from(name.to_string());
                            author_email =
                                SharedString::from(email.trim_end_matches('>').to_string());
                        }
                    }
                }
            }
        } else {
            if subject.is_none() {
                subject = Some(SharedString::from(line.to_string()));
            }
            message_lines.push(line);
        }
    }

    Some(CommitData {
        sha,
        parents,
        author_name,
        author_email,
        commit_timestamp,
        subject: subject.unwrap_or_default(),
        message: SharedString::from(message_lines.join("\n")),
    })
}
