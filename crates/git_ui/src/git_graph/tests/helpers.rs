use super::*;

use super::*;
use anyhow::{Context, Result, bail};
use collections::{HashMap, HashSet};
use fs::FakeFs;
use git::Oid;
use git::repository::{CommitData, InitialGraphCommitData};
use gpui::{TestAppContext, UpdateGlobal};
use project::git_store::{GitStoreEvent, RepositoryEvent};
use project::{Project, TaskSourceKind, task_store::TaskSettingsLocation};
use rand::prelude::*;
use serde_json::json;
use settings::{SettingsStore, ThemeSettingsContent};
use smallvec::{SmallVec, smallvec};
use std::path::Path;
use std::sync::{Arc, Mutex};

fn init_test(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
        theme_settings::init(theme::LoadThemes::JustBase, cx);
        language_model::init(cx);
        crate::init(cx);
    });
}

fn build_oid_to_row_map(graph: &GraphData) -> HashMap<Oid, usize> {
    graph
        .commits
        .iter()
        .enumerate()
        .map(|(idx, entry)| (entry.data.sha, idx))
        .collect()
}

fn verify_commit_order(graph: &GraphData, commits: &[Arc<InitialGraphCommitData>]) -> Result<()> {
    if graph.commits.len() != commits.len() {
        bail!(
            "Commit count mismatch: graph has {} commits, expected {}",
            graph.commits.len(),
            commits.len()
        );
    }

    for (idx, (graph_commit, expected_commit)) in
        graph.commits.iter().zip(commits.iter()).enumerate()
    {
        if graph_commit.data.sha != expected_commit.sha {
            bail!(
                "Commit order mismatch at index {}: graph has {:?}, expected {:?}",
                idx,
                graph_commit.data.sha,
                expected_commit.sha
            );
        }
    }

    Ok(())
}

fn verify_line_endpoints(graph: &GraphData, oid_to_row: &HashMap<Oid, usize>) -> Result<()> {
    for line in &graph.lines {
        let child_row = *oid_to_row
            .get(&line.child)
            .context("Line references non-existent child commit")?;

        let parent_row = *oid_to_row
            .get(&line.parent)
            .context("Line references non-existent parent commit")?;

        if child_row >= parent_row {
            bail!(
                "child_row ({}) must be < parent_row ({})",
                child_row,
                parent_row
            );
        }

        if line.full_interval.start != child_row {
            bail!(
                "full_interval.start ({}) != child_row ({})",
                line.full_interval.start,
                child_row
            );
        }

        if line.full_interval.end != parent_row {
            bail!(
                "full_interval.end ({}) != parent_row ({})",
                line.full_interval.end,
                parent_row
            );
        }

        if let Some(last_segment) = line.segments.last() {
            let segment_end_row = match last_segment {
                CommitLineSegment::Straight { to_row } => *to_row,
                CommitLineSegment::Curve { on_row, .. } => *on_row,
            };

            if segment_end_row != line.full_interval.end {
                bail!(
                    "last segment ends at row {} but full_interval.end is {}",
                    segment_end_row,
                    line.full_interval.end
                );
            }
        }
    }

    Ok(())
}

fn verify_column_correctness(graph: &GraphData, oid_to_row: &HashMap<Oid, usize>) -> Result<()> {
    for line in &graph.lines {
        let child_row = *oid_to_row
            .get(&line.child)
            .context("Line references non-existent child commit")?;

        let parent_row = *oid_to_row
            .get(&line.parent)
            .context("Line references non-existent parent commit")?;

        let child_lane = graph.commits[child_row].lane;
        if line.child_column != child_lane {
            bail!(
                "child_column ({}) != child's lane ({})",
                line.child_column,
                child_lane
            );
        }

        let mut current_column = line.child_column;
        for segment in &line.segments {
            if let CommitLineSegment::Curve { to_column, .. } = segment {
                current_column = *to_column;
            }
        }

        let parent_lane = graph.commits[parent_row].lane;
        if current_column != parent_lane {
            bail!(
                "ending column ({}) != parent's lane ({})",
                current_column,
                parent_lane
            );
        }
    }

    Ok(())
}

fn verify_segment_continuity(graph: &GraphData) -> Result<()> {
    for line in &graph.lines {
        if line.segments.is_empty() {
            bail!("Line has no segments");
        }

        let mut current_row = line.full_interval.start;

        for (idx, segment) in line.segments.iter().enumerate() {
            let segment_end_row = match segment {
                CommitLineSegment::Straight { to_row } => *to_row,
                CommitLineSegment::Curve { on_row, .. } => *on_row,
            };

            if segment_end_row < current_row {
                bail!(
                    "segment {} ends at row {} which is before current row {}",
                    idx,
                    segment_end_row,
                    current_row
                );
            }

            current_row = segment_end_row;
        }
    }

    Ok(())
}

fn verify_line_overlaps(graph: &GraphData) -> Result<()> {
    for line in &graph.lines {
        let child_row = line.full_interval.start;

        let mut current_column = line.child_column;
        let mut current_row = child_row;

        for segment in &line.segments {
            match segment {
                CommitLineSegment::Straight { to_row } => {
                    for row in (current_row + 1)..*to_row {
                        if row < graph.commits.len() {
                            let commit_at_row = &graph.commits[row];
                            if commit_at_row.lane == current_column {
                                bail!(
                                    "straight segment from row {} to {} in column {} passes through commit {:?} at row {}",
                                    current_row,
                                    to_row,
                                    current_column,
                                    commit_at_row.data.sha,
                                    row
                                );
                            }
                        }
                    }
                    current_row = *to_row;
                }
                CommitLineSegment::Curve {
                    to_column, on_row, ..
                } => {
                    current_column = *to_column;
                    current_row = *on_row;
                }
            }
        }
    }

    Ok(())
}

fn verify_keep_shared_parents_on_leftmost_lane(graph: &GraphData) -> Result<()> {
    let mut active_lane_parents: Vec<Option<Oid>> = Vec::new();
    let mut parent_to_lanes: HashMap<Oid, SmallVec<[usize; 1]>> = HashMap::default();

    for (row, entry) in graph.commits.iter().enumerate() {
        let pending_lanes = parent_to_lanes.remove(&entry.data.sha).unwrap_or_default();

        if pending_lanes.len() > 1
            && let Some(expected_lane) = pending_lanes.iter().copied().min()
            && entry.lane != expected_lane
        {
            bail!(
                "commit {:?} at row {} uses lane {}, but shared parent should use leftmost pending lane {} from {:?}",
                entry.data.sha,
                row,
                entry.lane,
                expected_lane,
                pending_lanes
            );
        }

        for lane in pending_lanes {
            let Some(active_lane_parent) = active_lane_parents.get_mut(lane) else {
                bail!(
                    "commit {:?} at row {} was pending on missing lane {}",
                    entry.data.sha,
                    row,
                    lane
                );
            };

            if *active_lane_parent != Some(entry.data.sha) {
                bail!(
                    "commit {:?} at row {} was pending on lane {}, but that lane points to {:?}",
                    entry.data.sha,
                    row,
                    lane,
                    active_lane_parent
                );
            }

            *active_lane_parent = None;
        }

        for (parent_index, parent) in entry.data.parents.iter().enumerate() {
            let lane = if parent_index == 0 {
                entry.lane
            } else if let Some(empty_lane) = active_lane_parents.iter().position(Option::is_none) {
                empty_lane
            } else {
                active_lane_parents.push(None);
                active_lane_parents.len() - 1
            };

            if lane >= active_lane_parents.len() {
                active_lane_parents.resize(lane + 1, None);
            }

            active_lane_parents[lane] = Some(*parent);
            parent_to_lanes.entry(*parent).or_default().push(lane);
        }
    }

    Ok(())
}

fn verify_coverage(graph: &GraphData) -> Result<()> {
    let mut expected_edges: HashSet<(Oid, Oid)> = HashSet::default();
    for entry in &graph.commits {
        for parent in &entry.data.parents {
            expected_edges.insert((entry.data.sha, *parent));
        }
    }

    let mut found_edges: HashSet<(Oid, Oid)> = HashSet::default();
    for line in &graph.lines {
        let edge = (line.child, line.parent);

        if !found_edges.insert(edge) {
            bail!(
                "Duplicate line found for edge {:?} -> {:?}",
                line.child,
                line.parent
            );
        }

        if !expected_edges.contains(&edge) {
            bail!(
                "Orphan line found: {:?} -> {:?} is not in the commit graph",
                line.child,
                line.parent
            );
        }
    }

    for (child, parent) in &expected_edges {
        if !found_edges.contains(&(*child, *parent)) {
            bail!("Missing line for edge {:?} -> {:?}", child, parent);
        }
    }

    assert_eq!(
        expected_edges.symmetric_difference(&found_edges).count(),
        0,
        "The symmetric difference should be zero"
    );

    Ok(())
}

fn verify_merge_line_optimality(graph: &GraphData, oid_to_row: &HashMap<Oid, usize>) -> Result<()> {
    for line in &graph.lines {
        let first_segment = line.segments.first();
        let is_merge_line = matches!(
            first_segment,
            Some(CommitLineSegment::Curve {
                curve_kind: CurveKind::Merge,
                ..
            })
        );

        if !is_merge_line {
            continue;
        }

        let child_row = *oid_to_row
            .get(&line.child)
            .context("Line references non-existent child commit")?;

        let parent_row = *oid_to_row
            .get(&line.parent)
            .context("Line references non-existent parent commit")?;

        let parent_lane = graph.commits[parent_row].lane;

        let Some(CommitLineSegment::Curve { to_column, .. }) = first_segment else {
            continue;
        };

        let curves_directly_to_parent = *to_column == parent_lane;

        if !curves_directly_to_parent {
            continue;
        }

        let curve_row = child_row + 1;
        let has_commits_in_path = graph.commits[curve_row..parent_row]
            .iter()
            .any(|c| c.lane == parent_lane);

        if has_commits_in_path {
            bail!(
                "Merge line from {:?} to {:?} curves directly to parent lane {} but there are commits in that lane between rows {} and {}",
                line.child,
                line.parent,
                parent_lane,
                curve_row,
                parent_row
            );
        }

        let curve_ends_at_parent = curve_row == parent_row;

        if curve_ends_at_parent {
            if line.segments.len() != 1 {
                bail!(
                    "Merge line from {:?} to {:?} curves directly to parent (curve_row == parent_row), but has {} segments instead of 1 [MergeCurve]",
                    line.child,
                    line.parent,
                    line.segments.len()
                );
            }
        } else {
            if line.segments.len() != 2 {
                bail!(
                    "Merge line from {:?} to {:?} curves directly to parent lane without overlap, but has {} segments instead of 2 [MergeCurve, Straight]",
                    line.child,
                    line.parent,
                    line.segments.len()
                );
            }

            let is_straight_segment = matches!(
                line.segments.get(1),
                Some(CommitLineSegment::Straight { .. })
            );

            if !is_straight_segment {
                bail!(
                    "Merge line from {:?} to {:?} curves directly to parent lane without overlap, but second segment is not a Straight segment",
                    line.child,
                    line.parent
                );
            }
        }
    }

    Ok(())
}

fn verify_all_invariants(graph: &GraphData, commits: &[Arc<InitialGraphCommitData>]) -> Result<()> {
    let oid_to_row = build_oid_to_row_map(graph);

    verify_commit_order(graph, commits).context("commit order")?;
    verify_line_endpoints(graph, &oid_to_row).context("line endpoints")?;
    verify_column_correctness(graph, &oid_to_row).context("column correctness")?;
    verify_segment_continuity(graph).context("segment continuity")?;
    verify_merge_line_optimality(graph, &oid_to_row).context("merge line optimality")?;
    verify_keep_shared_parents_on_leftmost_lane(graph)
        .context("keep shared parents on leftmost lane")?;
    verify_coverage(graph).context("coverage")?;
    verify_line_overlaps(graph).context("line overlaps")?;
    Ok(())
}
