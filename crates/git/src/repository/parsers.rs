use super::*;

pub(super) fn parse_file_history_changed_files_output(
    output: &str,
    queried_paths: &[RepoPath],
) -> Vec<FileHistoryChangedFileSets> {
    let mut histories = vec![FileHistoryChangedFileSets::default(); queried_paths.len()];

    for record in output.split('\x1e') {
        let changed_files = record
            .split('\0')
            .filter_map(|field| {
                let path = field.trim_start_matches('\n');
                if path.is_empty() {
                    return None;
                }
                RepoPath::new(path).ok()
            })
            .collect::<std::collections::BTreeSet<_>>();

        if changed_files.is_empty() {
            continue;
        }

        let file_set = changed_files.iter().cloned().collect::<Vec<_>>();
        for (index, queried_path) in queried_paths.iter().enumerate() {
            if changed_files.contains(queried_path) {
                histories[index].file_sets.push(file_set.clone());
            }
        }
    }

    histories
}

pub(super) fn parse_initial_graph_output<'a>(
    lines: impl Iterator<Item = &'a str>,
) -> Vec<Arc<InitialGraphCommitData>> {
    lines
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            // Format: "SHA\x00PARENT1 PARENT2...\x00REF1, REF2, ..."
            let mut parts = line.split('\x00');

            let sha = Oid::from_str(parts.next()?).ok()?;
            let parents_str = parts.next()?;
            let parents = parents_str
                .split_whitespace()
                .filter_map(|p| Oid::from_str(p).ok())
                .collect();

            let ref_names_str = parts.next().unwrap_or("");
            let ref_names = if ref_names_str.is_empty() {
                Vec::new()
            } else {
                ref_names_str
                    .split(", ")
                    .map(|s| SharedString::from(s.to_string()))
                    .collect()
            };

            Some(Arc::new(InitialGraphCommitData {
                sha,
                parents,
                ref_names,
            }))
        })
        .collect()
}

pub(super) fn git_status_args(path_prefixes: &[RepoPath]) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("status"),
        OsString::from("--porcelain=v1"),
        OsString::from("--untracked-files=all"),
        OsString::from("--no-renames"),
        OsString::from("-z"),
        OsString::from("--"),
    ];
    args.extend(path_prefixes.iter().map(|path_prefix| {
        if path_prefix.is_empty() {
            Path::new(".").into()
        } else {
            path_prefix.as_std_path().into()
        }
    }));
    args
}

/// Temporarily git-ignore commonly ignored files and files over 2MB
pub(super) async fn exclude_files(git: &GitBinary) -> Result<GitExcludeOverride> {
    const MAX_SIZE: u64 = 2 * 1024 * 1024; // 2 MB
    let mut excludes = git.with_exclude_overrides().await?;
    excludes
        .add_excludes(include_str!("../checkpoint.gitignore"))
        .await?;

    let working_directory = git.working_directory.clone();
    let untracked_files = git.list_untracked_files().await?;
    let excluded_paths = untracked_files.into_iter().map(|path| {
        let working_directory = working_directory.clone();
        smol::spawn(async move {
            let full_path = working_directory.join(path.clone());
            match smol::fs::metadata(&full_path).await {
                Ok(metadata) if metadata.is_file() && metadata.len() >= MAX_SIZE => {
                    Some(PathBuf::from("/").join(path.clone()))
                }
                _ => None,
            }
        })
    });

    let excluded_paths = futures::future::join_all(excluded_paths).await;
    let excluded_paths = excluded_paths.into_iter().flatten().collect::<Vec<_>>();

    if !excluded_paths.is_empty() {
        let exclude_patterns = excluded_paths
            .into_iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("\n");
        excludes.add_excludes(&exclude_patterns).await?;
    }

    Ok(excludes)
}

pub(super) fn parse_branch_input(input: &str) -> Result<Vec<Branch>> {
    let mut branches = Vec::new();
    for line in input.split('\n') {
        if line.is_empty() {
            continue;
        }
        let mut fields = line.split('\x00');
        let Some(head) = fields.next() else {
            continue;
        };
        let Some(head_sha) = fields.next().map(|f| f.to_string().into()) else {
            continue;
        };
        let Some(parent_sha) = fields.next().map(|f| f.to_string()) else {
            continue;
        };
        let Some(ref_name) = fields.next().map(|f| f.to_string().into()) else {
            continue;
        };
        let Some(upstream_name) = fields.next().map(|f| f.to_string()) else {
            continue;
        };
        let Some(upstream_tracking) = fields.next().and_then(|f| parse_upstream_track(f).ok())
        else {
            continue;
        };
        let Some(commiterdate) = fields.next().and_then(|f| f.parse::<i64>().ok()) else {
            continue;
        };
        let Some(author_name) = fields.next().map(|f| f.to_string().into()) else {
            continue;
        };
        let Some(subject) = fields.next().map(|f| f.to_string().into()) else {
            continue;
        };

        branches.push(Branch {
            is_head: head == "*",
            ref_name,
            most_recent_commit: Some(CommitSummary {
                sha: head_sha,
                subject,
                commit_timestamp: commiterdate,
                author_name: author_name,
                has_parent: !parent_sha.is_empty(),
            }),
            upstream: if upstream_name.is_empty() {
                None
            } else {
                Some(Upstream {
                    ref_name: upstream_name.into(),
                    tracking: upstream_tracking,
                })
            },
        })
    }

    Ok(branches)
}

pub(super) fn format_branch_scan_error(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr)
        .trim()
        .replace('\n', " ");
    if stderr.is_empty() {
        format!("git for-each-ref exited with {}", output.status)
    } else {
        stderr
    }
}

pub(super) fn parse_upstream_track(upstream_track: &str) -> Result<UpstreamTracking> {
    if upstream_track.is_empty() {
        return Ok(UpstreamTracking::Tracked(UpstreamTrackingStatus {
            ahead: 0,
            behind: 0,
        }));
    }

    let upstream_track = upstream_track.strip_prefix("[").context("missing [")?;
    let upstream_track = upstream_track.strip_suffix("]").context("missing [")?;
    let mut ahead: u32 = 0;
    let mut behind: u32 = 0;
    for component in upstream_track.split(", ") {
        if component == "gone" {
            return Ok(UpstreamTracking::Gone);
        }
        if let Some(ahead_num) = component.strip_prefix("ahead ") {
            ahead = ahead_num.parse::<u32>()?;
        }
        if let Some(behind_num) = component.strip_prefix("behind ") {
            behind = behind_num.parse::<u32>()?;
        }
    }
    Ok(UpstreamTracking::Tracked(UpstreamTrackingStatus {
        ahead,
        behind,
    }))
}

pub(super) fn checkpoint_author_envs() -> HashMap<String, String> {
    HashMap::from_iter([
        ("GIT_AUTHOR_NAME".to_string(), "Mav".to_string()),
        ("GIT_AUTHOR_EMAIL".to_string(), "hi@mav.dev".to_string()),
        ("GIT_COMMITTER_NAME".to_string(), "Mav".to_string()),
        ("GIT_COMMITTER_EMAIL".to_string(), "hi@mav.dev".to_string()),
    ])
}
