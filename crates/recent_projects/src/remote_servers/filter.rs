//! Fuzzy filtering for the remote-projects modal.
//!
//! At construction time we build [`FilterData`]: a flat list of
//! [`StringMatchCandidate`]s with one candidate per project for servers that
//! have projects, and one candidate per server (over the host name) for
//! project-less servers and SSH-config-only entries. Each candidate matches
//! against the displayed host plus any search-only alias (the real SSH host
//! when a nickname hides it), so a server stays findable by either name. Each
//! candidate is tagged with [`CandidateMeta`] recording which server (and
//! project, if any) it represents. This snapshot is reused for every
//! keystroke; we only rebuild it when the set of servers changes.
//!
//! On each query, `fuzzy_nucleo::match_strings[_async]` returns matches sorted
//! by score. [`build_filter_results`] regroups those matches by server (a
//! server with N projects can contribute up to N matches) and folds each
//! bucket into a single [`FilteredServer`] carrying highlight positions and
//! the best score across its candidates.
//!
//! Projects inside a [`FilteredServer`] are intentionally ordered by fuzzy
//! score, not by their position in the source list — when a query matches
//! only the host name, the server's projects come back ranked by how well
//! each one also matched.
//!
//! The caller stores the resulting `Vec<FilteredServer>` in
//! [`DefaultState::filtered_servers`] and renders by looking up each
//! `server_index` / `project_index` against the unchanged source list.

use std::sync::atomic::{self, AtomicBool};

use fuzzy_nucleo::{StringMatch, StringMatchCandidate};
use gpui::BackgroundExecutor;

use super::RemoteEntry;

#[derive(Debug)]
pub(super) struct FilterData {
    pub(super) candidates: Vec<StringMatchCandidate>,
    pub(super) meta: Vec<CandidateMeta>,
    pub(super) server_count: usize,
}

#[derive(Debug)]
pub(super) struct CandidateMeta {
    pub(super) server_index: usize,
    pub(super) project_index: Option<usize>,
    /// Byte length of the host text that is actually displayed (and thus
    /// highlightable) as the server's primary label. Host match positions at
    /// or beyond this are dropped — they fall inside the search-only alias.
    pub(super) display_host_byte_len: usize,
    /// Byte length of the full searchable host text (display host plus any
    /// alias), used to find where the project-path portion of the combined
    /// candidate string begins.
    pub(super) match_host_byte_len: usize,
}

#[derive(Clone, Debug)]
pub(super) struct FilteredServer {
    pub(super) server_index: usize,
    pub(super) host_positions: Vec<usize>,
    pub(super) project_matches: Vec<FilteredProject>,
    pub(super) score: f64,
}

#[derive(Clone, Debug)]
pub(super) struct FilteredProject {
    pub(super) project_index: usize,
    pub(super) path_positions: Vec<usize>,
}

impl FilterData {
    pub(super) fn build(servers: &[RemoteEntry]) -> Self {
        let mut candidates = Vec::new();
        let mut meta = Vec::new();
        for (server_index, server) in servers.iter().enumerate() {
            let display_host = server.display_host();
            let display_host_byte_len = display_host.len();
            let search_host = match server.host_alias() {
                Some(alias) => format!("{display_host} {alias}"),
                None => display_host.to_string(),
            };
            let match_host_byte_len = search_host.len();
            match server {
                RemoteEntry::Project { projects, .. } if !projects.is_empty() => {
                    for (project_index, entry) in projects.iter().enumerate() {
                        let combined = format!("{search_host} {}", entry.project.paths.join(", "));
                        meta.push(CandidateMeta {
                            server_index,
                            project_index: Some(project_index),
                            display_host_byte_len,
                            match_host_byte_len,
                        });
                        candidates.push(StringMatchCandidate::new(candidates.len(), combined));
                    }
                }
                RemoteEntry::Project { .. } | RemoteEntry::SshConfig { .. } => {
                    meta.push(CandidateMeta {
                        server_index,
                        project_index: None,
                        display_host_byte_len,
                        match_host_byte_len,
                    });
                    candidates.push(StringMatchCandidate::new(candidates.len(), search_host));
                }
            }
        }
        Self {
            candidates,
            meta,
            server_count: servers.len(),
        }
    }
}

pub(super) fn build_filter_results(
    matches: Vec<StringMatch>,
    filter_data: &FilterData,
) -> Vec<FilteredServer> {
    group_matches_by_server(matches, filter_data)
        .into_iter()
        .enumerate()
        .filter_map(|(server_index, group)| {
            (!group.is_empty()).then(|| build_server_result(server_index, group))
        })
        .collect()
}

fn group_matches_by_server(
    matches: Vec<StringMatch>,
    filter_data: &FilterData,
) -> Vec<Vec<(StringMatch, &CandidateMeta)>> {
    let mut buckets: Vec<Vec<_>> = (0..filter_data.server_count).map(|_| Vec::new()).collect();
    for m in matches {
        let Some(meta) = filter_data.meta.get(m.candidate_id) else {
            continue;
        };
        let Some(bucket) = buckets.get_mut(meta.server_index) else {
            continue;
        };
        bucket.push((m, meta));
    }
    buckets
}

fn build_server_result(
    server_index: usize,
    group: Vec<(StringMatch, &CandidateMeta)>,
) -> FilteredServer {
    debug_assert!(!group.is_empty(), "empty groups are filtered out upstream");

    let mut host_positions = Vec::new();
    let mut project_matches = Vec::new();
    let mut score = f64::NEG_INFINITY;

    for (m, meta) in group {
        score = score.max(m.score);
        // `FilterData::build` emits either one host-only candidate or one
        // candidate per project for a server, never a mix, so the `None` arm
        // assigning `host_positions` can't clobber positions accumulated by
        // the `Some` arm.
        match meta.project_index {
            None => {
                host_positions = m
                    .positions
                    .into_iter()
                    .filter(|&p| p < meta.display_host_byte_len)
                    .collect();
            }
            Some(project_index) => {
                // +1 accounts for the single-byte space separator in
                // format!("{search_host} {paths}") used by FilterData::build.
                // Positions inside the search-only host alias (between the
                // displayed host and the separator) are dropped from both
                // sides — they index into content that isn't shown anywhere.
                let host_prefix_len = meta.match_host_byte_len + 1;
                host_positions.extend(
                    m.positions
                        .iter()
                        .copied()
                        .filter(|&p| p < meta.display_host_byte_len),
                );
                project_matches.push(FilteredProject {
                    project_index,
                    path_positions: m
                        .positions
                        .into_iter()
                        .filter_map(|p| p.checked_sub(host_prefix_len))
                        .collect(),
                });
            }
        }
    }

    host_positions.sort_unstable();
    host_positions.dedup();

    FilteredServer {
        server_index,
        host_positions,
        project_matches,
        score,
    }
}

pub(super) fn run_sync(data: &FilterData, query: &str) -> Vec<FilteredServer> {
    let case = fuzzy_nucleo::Case::smart_if_uppercase_in(query);
    let matches = fuzzy_nucleo::match_strings(
        &data.candidates,
        query,
        case,
        fuzzy_nucleo::LengthPenalty::Off,
        data.candidates.len(),
    );
    let mut results = build_filter_results(matches, data);
    results.sort_by(|a, b| b.score.total_cmp(&a.score));
    results
}

pub(super) async fn run_async(
    data: &FilterData,
    query: &str,
    cancel: &AtomicBool,
    executor: BackgroundExecutor,
) -> Option<Vec<FilteredServer>> {
    let case = fuzzy_nucleo::Case::smart_if_uppercase_in(query);
    let matches = fuzzy_nucleo::match_strings_async(
        &data.candidates,
        query,
        case,
        fuzzy_nucleo::LengthPenalty::Off,
        data.candidates.len(),
        cancel,
        executor,
    )
    .await;
    if cancel.load(atomic::Ordering::Acquire) {
        return None;
    }
    let mut results = build_filter_results(matches, data);
    results.sort_by(|a, b| b.score.total_cmp(&a.score));
    Some(results)
}

#[cfg(test)]
mod tests {
    include!("filter/tests.rs");
}
