use super::*;

pub(super) fn merge_event_roots(
    changed_paths: &[Arc<RelPath>],
    event_roots: &[EventRoot],
) -> Vec<EventRoot> {
    let mut merged_event_roots = Vec::with_capacity(changed_paths.len() + event_roots.len());
    let mut changed_paths = changed_paths.iter().peekable();
    let mut event_roots = event_roots.iter().peekable();
    while let (Some(path), Some(event_root)) = (changed_paths.peek(), event_roots.peek()) {
        match path.cmp(&&event_root.path) {
            Ordering::Less => {
                merged_event_roots.push(EventRoot {
                    path: (*changed_paths.next().expect("peeked changed path")).clone(),
                    was_rescanned: false,
                });
            }
            Ordering::Equal => {
                merged_event_roots.push((*event_roots.next().expect("peeked event root")).clone());
                changed_paths.next();
            }
            Ordering::Greater => {
                merged_event_roots.push((*event_roots.next().expect("peeked event root")).clone());
            }
        }
    }
    merged_event_roots.extend(changed_paths.map(|path| EventRoot {
        path: path.clone(),
        was_rescanned: false,
    }));
    merged_event_roots.extend(event_roots.cloned());
    merged_event_roots
}

pub(super) fn build_diff(
    phase: BackgroundScannerPhase,
    old_snapshot: &Snapshot,
    new_snapshot: &Snapshot,
    event_roots: &[EventRoot],
) -> UpdatedEntriesSet {
    use BackgroundScannerPhase::*;
    use PathChange::{Added, AddedOrUpdated, Loaded, Removed, Updated};

    // Identify which paths have changed. Use the known set of changed
    // parent paths to optimize the search.
    let mut changes = Vec::new();

    let mut old_paths = old_snapshot.entries_by_path.cursor::<PathKey>(());
    let mut new_paths = new_snapshot.entries_by_path.cursor::<PathKey>(());
    let mut last_newly_loaded_dir_path = None;
    old_paths.next();
    new_paths.next();
    for event_root in event_roots {
        let path = PathKey(event_root.path.clone());
        if old_paths.item().is_some_and(|e| e.path < path.0) {
            old_paths.seek_forward(&path, Bias::Left);
        }
        if new_paths.item().is_some_and(|e| e.path < path.0) {
            new_paths.seek_forward(&path, Bias::Left);
        }
        loop {
            match (old_paths.item(), new_paths.item()) {
                (Some(old_entry), Some(new_entry)) => {
                    if old_entry.path > path.0
                        && new_entry.path > path.0
                        && !old_entry.path.starts_with(&path.0)
                        && !new_entry.path.starts_with(&path.0)
                    {
                        break;
                    }

                    match Ord::cmp(&old_entry.path, &new_entry.path) {
                        Ordering::Less => {
                            changes.push((old_entry.path.clone(), old_entry.id, Removed));
                            old_paths.next();
                        }
                        Ordering::Equal => {
                            if phase == EventsReceivedDuringInitialScan {
                                if old_entry.id != new_entry.id {
                                    changes.push((old_entry.path.clone(), old_entry.id, Removed));
                                }
                                // If the worktree was not fully initialized when this event was generated,
                                // we can't know whether this entry was added during the scan or whether
                                // it was merely updated.
                                changes.push((
                                    new_entry.path.clone(),
                                    new_entry.id,
                                    AddedOrUpdated,
                                ));
                            } else if old_entry.id != new_entry.id {
                                changes.push((old_entry.path.clone(), old_entry.id, Removed));
                                changes.push((new_entry.path.clone(), new_entry.id, Added));
                            } else if old_entry != new_entry {
                                if old_entry.kind.is_unloaded() {
                                    last_newly_loaded_dir_path = Some(&new_entry.path);
                                    changes.push((new_entry.path.clone(), new_entry.id, Loaded));
                                } else {
                                    changes.push((new_entry.path.clone(), new_entry.id, Updated));
                                }
                            } else if event_root.was_rescanned {
                                changes.push((new_entry.path.clone(), new_entry.id, Updated));
                            }
                            old_paths.next();
                            new_paths.next();
                        }
                        Ordering::Greater => {
                            let is_newly_loaded = phase == InitialScan
                                || last_newly_loaded_dir_path
                                    .as_ref()
                                    .is_some_and(|dir| new_entry.path.starts_with(dir));
                            changes.push((
                                new_entry.path.clone(),
                                new_entry.id,
                                if is_newly_loaded { Loaded } else { Added },
                            ));
                            new_paths.next();
                        }
                    }
                }
                (Some(old_entry), None) => {
                    changes.push((old_entry.path.clone(), old_entry.id, Removed));
                    old_paths.next();
                }
                (None, Some(new_entry)) => {
                    let is_newly_loaded = phase == InitialScan
                        || last_newly_loaded_dir_path
                            .as_ref()
                            .is_some_and(|dir| new_entry.path.starts_with(dir));
                    changes.push((
                        new_entry.path.clone(),
                        new_entry.id,
                        if is_newly_loaded { Loaded } else { Added },
                    ));
                    new_paths.next();
                }
                (None, None) => break,
            }
        }
    }

    changes.into()
}

pub(super) fn swap_to_front(child_paths: &mut Vec<PathBuf>, file: &str) {
    let position = child_paths
        .iter()
        .position(|path| path.file_name().unwrap() == file);
    if let Some(position) = position {
        let temp = child_paths.remove(position);
        child_paths.insert(0, temp);
    }
}

pub(super) fn char_bag_for_path(root_char_bag: CharBag, path: &RelPath) -> CharBag {
    let mut result = root_char_bag;
    result.extend(path.as_unix_str().chars().map(|c| c.to_ascii_lowercase()));
    result
}
