use std::path::Path;

use gpui::{App, Entity};
use project::Worktree;
use util::paths::{PathWithPosition, normalize_lexically};

pub(super) fn local_paths_to_check(
    potential_paths: &[PathWithPosition],
    cwd: Option<&Path>,
    worktree_candidates: &[Entity<Worktree>],
    cx: &App,
) -> Vec<PathWithPosition> {
    cwd.iter()
        .flat_map(|cwd| {
            potential_paths.iter().filter_map(|path_to_check| {
                path_to_check.path.is_relative().then(|| PathWithPosition {
                    path: cwd.join(&path_to_check.path),
                    row: path_to_check.row,
                    column: path_to_check.column,
                })
            })
        })
        .chain(potential_paths.iter().flat_map(|path_to_check| {
            let mut paths_to_check = Vec::new();
            let maybe_path = &path_to_check.path;
            if maybe_path.starts_with("~") {
                if let Some(home_path) =
                    maybe_path
                        .strip_prefix("~")
                        .ok()
                        .and_then(|stripped_maybe_path| {
                            Some(dirs::home_dir()?.join(stripped_maybe_path))
                        })
                {
                    paths_to_check.push(PathWithPosition {
                        path: home_path,
                        row: path_to_check.row,
                        column: path_to_check.column,
                    });
                }
            } else {
                paths_to_check.push(PathWithPosition {
                    path: maybe_path.clone(),
                    row: path_to_check.row,
                    column: path_to_check.column,
                });
                if maybe_path.is_relative() {
                    for worktree in worktree_candidates {
                        if !worktree.read(cx).is_single_file() {
                            paths_to_check.push(PathWithPosition {
                                path: worktree.read(cx).abs_path().join(maybe_path),
                                row: path_to_check.row,
                                column: path_to_check.column,
                            });
                        }
                    }
                }
            }
            paths_to_check
        }))
        .collect()
}

pub(super) fn project_paths_to_check(
    potential_paths: &[PathWithPosition],
    cwd: Option<&Path>,
) -> Vec<PathWithPosition> {
    cwd.iter()
        .flat_map(|cwd| {
            potential_paths
                .iter()
                .filter_map(|path_to_check| normalize_absolute_candidate(cwd, path_to_check))
        })
        .chain(potential_paths.iter().filter_map(|path_to_check| {
            let maybe_path = &path_to_check.path;
            (maybe_path.starts_with("~") || maybe_path.is_absolute()).then(|| path_to_check.clone())
        }))
        .collect()
}

fn normalize_absolute_candidate(
    cwd: &Path,
    path_to_check: &PathWithPosition,
) -> Option<PathWithPosition> {
    path_to_check.path.is_relative().then(|| {
        normalize_lexically(&cwd.join(&path_to_check.path))
            .ok()
            .map(|path| PathWithPosition {
                path,
                row: path_to_check.row,
                column: path_to_check.column,
            })
    })?
}
