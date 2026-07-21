use super::*;

pub(super) fn serialize_blame_buffer_response(
    blame: Option<git::blame::Blame>,
) -> proto::BlameBufferResponse {
    let Some(blame) = blame else {
        return proto::BlameBufferResponse {
            blame_response: None,
        };
    };

    let entries = blame
        .entries
        .into_iter()
        .map(|entry| proto::BlameEntry {
            sha: entry.sha.as_bytes().into(),
            start_line: entry.range.start,
            end_line: entry.range.end,
            original_line_number: entry.original_line_number,
            author: entry.author,
            author_mail: entry.author_mail,
            author_time: entry.author_time,
            author_tz: entry.author_tz,
            committer: entry.committer_name,
            committer_mail: entry.committer_email,
            committer_time: entry.committer_time,
            committer_tz: entry.committer_tz,
            summary: entry.summary,
            previous: entry.previous,
            filename: entry.filename,
        })
        .collect::<Vec<_>>();

    let messages = blame
        .messages
        .into_iter()
        .map(|(oid, message)| proto::CommitMessage {
            oid: oid.as_bytes().into(),
            message,
        })
        .collect::<Vec<_>>();

    proto::BlameBufferResponse {
        blame_response: Some(proto::blame_buffer_response::BlameResponse { entries, messages }),
    }
}

pub(super) fn deserialize_blame_buffer_response(
    response: proto::BlameBufferResponse,
) -> Option<git::blame::Blame> {
    let response = response.blame_response?;
    let entries = response
        .entries
        .into_iter()
        .filter_map(|entry| {
            Some(git::blame::BlameEntry {
                sha: git::Oid::from_bytes(&entry.sha).ok()?,
                range: entry.start_line..entry.end_line,
                original_line_number: entry.original_line_number,
                committer_name: entry.committer,
                committer_time: entry.committer_time,
                committer_tz: entry.committer_tz,
                committer_email: entry.committer_mail,
                author: entry.author,
                author_mail: entry.author_mail,
                author_time: entry.author_time,
                author_tz: entry.author_tz,
                summary: entry.summary,
                previous: entry.previous,
                filename: entry.filename,
            })
        })
        .collect::<Vec<_>>();

    let messages = response
        .messages
        .into_iter()
        .filter_map(|message| Some((git::Oid::from_bytes(&message.oid).ok()?, message.message)))
        .collect::<HashMap<_, _>>();

    Some(Blame { entries, messages })
}

pub(super) fn log_source_to_proto(log_source: &LogSource) -> proto::GitLogSource {
    proto::GitLogSource {
        source: Some(match log_source {
            LogSource::All => proto::git_log_source::Source::All(proto::GitLogSourceAll {}),
            LogSource::Branch(branch) => proto::git_log_source::Source::Branch(branch.to_string()),
            LogSource::Sha(sha) => proto::git_log_source::Source::Sha(sha.to_string()),
            LogSource::Path(path) => proto::git_log_source::Source::Path(path.to_proto()),
        }),
    }
}

pub(super) fn log_source_from_proto(log_source: proto::GitLogSource) -> Result<LogSource> {
    match log_source
        .source
        .context("git log source is missing source")?
    {
        proto::git_log_source::Source::All(_) => Ok(LogSource::All),
        proto::git_log_source::Source::Branch(branch) => Ok(LogSource::Branch(branch.into())),
        proto::git_log_source::Source::Sha(sha) => Ok(LogSource::Sha(Oid::from_str(&sha)?)),
        proto::git_log_source::Source::Path(path) => {
            Ok(LogSource::Path(RepoPath::from_proto(&path)?))
        }
    }
}

pub(super) fn log_order_to_proto(log_order: LogOrder) -> i32 {
    match log_order {
        LogOrder::DateOrder => proto::get_initial_graph_data::LogOrder::DateOrder as i32,
        LogOrder::TopoOrder => proto::get_initial_graph_data::LogOrder::TopoOrder as i32,
        LogOrder::AuthorDateOrder => {
            proto::get_initial_graph_data::LogOrder::AuthorDateOrder as i32
        }
        LogOrder::ReverseChronological => {
            proto::get_initial_graph_data::LogOrder::ReverseChronological as i32
        }
    }
}

pub(super) fn log_order_from_proto(log_order: proto::get_initial_graph_data::LogOrder) -> LogOrder {
    match log_order {
        proto::get_initial_graph_data::LogOrder::DateOrder => LogOrder::DateOrder,
        proto::get_initial_graph_data::LogOrder::TopoOrder => LogOrder::TopoOrder,
        proto::get_initial_graph_data::LogOrder::AuthorDateOrder => LogOrder::AuthorDateOrder,
        proto::get_initial_graph_data::LogOrder::ReverseChronological => {
            LogOrder::ReverseChronological
        }
    }
}

pub(super) fn initial_graph_commit_to_proto(
    commit: &InitialGraphCommitData,
) -> proto::InitialGraphCommit {
    proto::InitialGraphCommit {
        sha: commit.sha.to_string(),
        parents: commit
            .parents
            .iter()
            .map(|parent| parent.to_string())
            .collect(),
        ref_names: commit
            .ref_names
            .iter()
            .map(|ref_name| ref_name.to_string())
            .collect(),
    }
}

pub(super) fn initial_graph_commit_from_proto(
    commit: proto::InitialGraphCommit,
) -> Result<Arc<InitialGraphCommitData>> {
    let sha = Oid::from_str(&commit.sha)?;
    let mut parents = SmallVec::with_capacity(commit.parents.len());
    for parent in &commit.parents {
        parents.push(Oid::from_str(parent)?);
    }
    Ok(Arc::new(InitialGraphCommitData {
        sha,
        parents,
        ref_names: commit
            .ref_names
            .into_iter()
            .map(SharedString::from)
            .collect(),
    }))
}

pub(super) fn commit_data_to_proto(commit: &CommitData) -> proto::CommitData {
    proto::CommitData {
        sha: commit.sha.to_string(),
        parents: commit.parents.iter().map(|p| p.to_string()).collect(),
        author_name: commit.author_name.to_string(),
        author_email: commit.author_email.to_string(),
        commit_timestamp: commit.commit_timestamp,
        subject: commit.subject.to_string(),
        message: commit.message.to_string(),
    }
}

pub(super) fn commit_data_from_proto(commit: proto::CommitData) -> Result<CommitData> {
    let sha = Oid::from_str(&commit.sha)?;
    let mut parents = SmallVec::with_capacity(commit.parents.len());
    for parent in &commit.parents {
        parents.push(Oid::from_str(parent)?);
    }
    Ok(CommitData {
        sha,
        parents,
        author_name: SharedString::from(commit.author_name),
        author_email: SharedString::from(commit.author_email),
        commit_timestamp: commit.commit_timestamp,
        subject: SharedString::from(commit.subject),
        message: SharedString::from(commit.message),
    })
}

pub(super) fn branch_to_proto(branch: &git::repository::Branch) -> proto::Branch {
    proto::Branch {
        is_head: branch.is_head,
        ref_name: branch.ref_name.to_string(),
        unix_timestamp: branch
            .most_recent_commit
            .as_ref()
            .map(|commit| commit.commit_timestamp as u64),
        upstream: branch.upstream.as_ref().map(|upstream| proto::GitUpstream {
            ref_name: upstream.ref_name.to_string(),
            tracking: upstream
                .tracking
                .status()
                .map(|upstream| proto::UpstreamTracking {
                    ahead: upstream.ahead as u64,
                    behind: upstream.behind as u64,
                }),
        }),
        most_recent_commit: branch
            .most_recent_commit
            .as_ref()
            .map(|commit| proto::CommitSummary {
                sha: commit.sha.to_string(),
                subject: commit.subject.to_string(),
                commit_timestamp: commit.commit_timestamp,
                author_name: commit.author_name.to_string(),
            }),
    }
}

pub(super) fn worktree_to_proto(worktree: &git::repository::Worktree) -> proto::Worktree {
    proto::Worktree {
        path: worktree.path.to_string_lossy().to_string(),
        ref_name: worktree
            .ref_name
            .as_ref()
            .map(|s| s.to_string())
            .unwrap_or_default(),
        sha: worktree.sha.to_string(),
        is_main: worktree.is_main,
        is_bare: worktree.is_bare,
    }
}

pub(super) fn proto_to_worktree(proto: &proto::Worktree) -> git::repository::Worktree {
    git::repository::Worktree {
        path: PathBuf::from(proto.path.clone()),
        ref_name: if proto.ref_name.is_empty() {
            None
        } else {
            Some(SharedString::from(&proto.ref_name))
        },
        sha: proto.sha.clone().into(),
        is_main: proto.is_main,
        is_bare: proto.is_bare,
    }
}

pub(super) fn proto_to_branch(proto: &proto::Branch) -> git::repository::Branch {
    git::repository::Branch {
        is_head: proto.is_head,
        ref_name: proto.ref_name.clone().into(),
        upstream: proto
            .upstream
            .as_ref()
            .map(|upstream| git::repository::Upstream {
                ref_name: upstream.ref_name.to_string().into(),
                tracking: upstream
                    .tracking
                    .as_ref()
                    .map(|tracking| {
                        git::repository::UpstreamTracking::Tracked(UpstreamTrackingStatus {
                            ahead: tracking.ahead as u32,
                            behind: tracking.behind as u32,
                        })
                    })
                    .unwrap_or(git::repository::UpstreamTracking::Gone),
            }),
        most_recent_commit: proto.most_recent_commit.as_ref().map(|commit| {
            git::repository::CommitSummary {
                sha: commit.sha.to_string().into(),
                subject: commit.subject.to_string().into(),
                commit_timestamp: commit.commit_timestamp,
                author_name: commit.author_name.to_string().into(),
                has_parent: true,
            }
        }),
    }
}

pub(super) fn commit_details_to_proto(commit: &CommitDetails) -> proto::GitCommitDetails {
    proto::GitCommitDetails {
        sha: commit.sha.to_string(),
        message: commit.message.to_string(),
        commit_timestamp: commit.commit_timestamp,
        author_email: commit.author_email.to_string(),
        author_name: commit.author_name.to_string(),
    }
}

pub(super) fn proto_to_commit_details(proto: &proto::GitCommitDetails) -> CommitDetails {
    CommitDetails {
        sha: proto.sha.clone().into(),
        message: proto.message.clone().into(),
        commit_timestamp: proto.commit_timestamp,
        author_email: proto.author_email.clone().into(),
        author_name: proto.author_name.clone().into(),
    }
}

pub(super) fn status_from_proto(
    simple_status: i32,
    status: Option<proto::GitFileStatus>,
) -> anyhow::Result<FileStatus> {
    use proto::git_file_status::Variant;

    let Some(variant) = status.and_then(|status| status.variant) else {
        let code = proto::GitStatus::from_i32(simple_status)
            .with_context(|| format!("Invalid git status code: {simple_status}"))?;
        let result = match code {
            proto::GitStatus::Added => TrackedStatus {
                worktree_status: StatusCode::Added,
                index_status: StatusCode::Unmodified,
            }
            .into(),
            proto::GitStatus::Modified => TrackedStatus {
                worktree_status: StatusCode::Modified,
                index_status: StatusCode::Unmodified,
            }
            .into(),
            proto::GitStatus::Conflict => UnmergedStatus {
                first_head: UnmergedStatusCode::Updated,
                second_head: UnmergedStatusCode::Updated,
            }
            .into(),
            proto::GitStatus::Deleted => TrackedStatus {
                worktree_status: StatusCode::Deleted,
                index_status: StatusCode::Unmodified,
            }
            .into(),
            _ => anyhow::bail!("Invalid code for simple status: {simple_status}"),
        };
        return Ok(result);
    };

    let result = match variant {
        Variant::Untracked(_) => FileStatus::Untracked,
        Variant::Ignored(_) => FileStatus::Ignored,
        Variant::Unmerged(unmerged) => {
            let [first_head, second_head] =
                [unmerged.first_head, unmerged.second_head].map(|head| {
                    let code = proto::GitStatus::from_i32(head)
                        .with_context(|| format!("Invalid git status code: {head}"))?;
                    let result = match code {
                        proto::GitStatus::Added => UnmergedStatusCode::Added,
                        proto::GitStatus::Updated => UnmergedStatusCode::Updated,
                        proto::GitStatus::Deleted => UnmergedStatusCode::Deleted,
                        _ => anyhow::bail!("Invalid code for unmerged status: {code:?}"),
                    };
                    Ok(result)
                });
            let [first_head, second_head] = [first_head?, second_head?];
            UnmergedStatus {
                first_head,
                second_head,
            }
            .into()
        }
        Variant::Tracked(tracked) => {
            let [index_status, worktree_status] = [tracked.index_status, tracked.worktree_status]
                .map(|status| {
                    let code = proto::GitStatus::from_i32(status)
                        .with_context(|| format!("Invalid git status code: {status}"))?;
                    let result = match code {
                        proto::GitStatus::Modified => StatusCode::Modified,
                        proto::GitStatus::TypeChanged => StatusCode::TypeChanged,
                        proto::GitStatus::Added => StatusCode::Added,
                        proto::GitStatus::Deleted => StatusCode::Deleted,
                        proto::GitStatus::Renamed => StatusCode::Renamed,
                        proto::GitStatus::Copied => StatusCode::Copied,
                        proto::GitStatus::Unmodified => StatusCode::Unmodified,
                        _ => anyhow::bail!("Invalid code for tracked status: {code:?}"),
                    };
                    Ok(result)
                });
            let [index_status, worktree_status] = [index_status?, worktree_status?];
            TrackedStatus {
                index_status,
                worktree_status,
            }
            .into()
        }
    };
    Ok(result)
}

pub(super) fn status_to_proto(status: FileStatus) -> proto::GitFileStatus {
    use proto::git_file_status::{Tracked, Unmerged, Variant};

    let variant = match status {
        FileStatus::Untracked => Variant::Untracked(Default::default()),
        FileStatus::Ignored => Variant::Ignored(Default::default()),
        FileStatus::Unmerged(UnmergedStatus {
            first_head,
            second_head,
        }) => Variant::Unmerged(Unmerged {
            first_head: unmerged_status_to_proto(first_head),
            second_head: unmerged_status_to_proto(second_head),
        }),
        FileStatus::Tracked(TrackedStatus {
            index_status,
            worktree_status,
        }) => Variant::Tracked(Tracked {
            index_status: tracked_status_to_proto(index_status),
            worktree_status: tracked_status_to_proto(worktree_status),
        }),
    };
    proto::GitFileStatus {
        variant: Some(variant),
    }
}

pub(super) fn unmerged_status_to_proto(code: UnmergedStatusCode) -> i32 {
    match code {
        UnmergedStatusCode::Added => proto::GitStatus::Added as _,
        UnmergedStatusCode::Deleted => proto::GitStatus::Deleted as _,
        UnmergedStatusCode::Updated => proto::GitStatus::Updated as _,
    }
}

pub(super) fn tracked_status_to_proto(code: StatusCode) -> i32 {
    match code {
        StatusCode::Added => proto::GitStatus::Added as _,
        StatusCode::Deleted => proto::GitStatus::Deleted as _,
        StatusCode::Modified => proto::GitStatus::Modified as _,
        StatusCode::Renamed => proto::GitStatus::Renamed as _,
        StatusCode::TypeChanged => proto::GitStatus::TypeChanged as _,
        StatusCode::Copied => proto::GitStatus::Copied as _,
        StatusCode::Unmodified => proto::GitStatus::Unmodified as _,
    }
}
