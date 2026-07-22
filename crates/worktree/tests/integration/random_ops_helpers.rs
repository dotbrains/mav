use super::*;
use pretty_assertions::assert_eq;

pub(crate) fn check_worktree_change_events(tree: &mut Worktree, cx: &mut Context<Worktree>) {
    let mut entries = tree.entries(true, 0).cloned().collect::<Vec<_>>();
    cx.subscribe(&cx.entity(), move |tree, _, event, _| {
        if let Event::UpdatedEntries(changes) = event {
            for (path, _, change_type) in changes.iter() {
                let entry = tree.entry_for_path(path).cloned();
                let ix = match entries.binary_search_by_key(&path, |e| &e.path) {
                    Ok(ix) | Err(ix) => ix,
                };
                match change_type {
                    PathChange::Added => entries.insert(ix, entry.unwrap()),
                    PathChange::Removed => drop(entries.remove(ix)),
                    PathChange::Updated => {
                        let entry = entry.unwrap();
                        let existing_entry = entries.get_mut(ix).unwrap();
                        assert_eq!(existing_entry.path, entry.path);
                        *existing_entry = entry;
                    }
                    PathChange::AddedOrUpdated | PathChange::Loaded => {
                        let entry = entry.unwrap();
                        if entries.get(ix).map(|e| &e.path) == Some(&entry.path) {
                            *entries.get_mut(ix).unwrap() = entry;
                        } else {
                            entries.insert(ix, entry);
                        }
                    }
                }
            }

            let new_entries = tree.entries(true, 0).cloned().collect::<Vec<_>>();
            assert_eq!(entries, new_entries, "incorrect changes: {:?}", changes);
        }
    })
    .detach();
}

pub(crate) fn randomly_mutate_worktree(
    worktree: &mut Worktree,
    rng: &mut impl Rng,
    cx: &mut Context<Worktree>,
) -> Task<Result<()>> {
    log::info!("mutating worktree");
    let worktree = worktree.as_local_mut().unwrap();
    let snapshot = worktree.snapshot();
    let entry = snapshot.entries(false, 0).choose(rng).unwrap();

    match rng.random_range(0_u32..100) {
        0..=33 if entry.path.as_ref() != RelPath::empty() => {
            log::info!("deleting entry {:?} ({})", entry.path, entry.id.to_usize());
            let task = worktree
                .delete_entry(entry.id, false, cx)
                .unwrap_or_else(|| Task::ready(Ok(None)));

            cx.background_spawn(async move {
                task.await?;
                Ok(())
            })
        }
        _ => {
            if entry.is_dir() {
                let child_path = entry.path.join(rel_path(&random_filename(rng)));
                let is_dir = rng.random_bool(0.3);
                log::info!(
                    "creating {} at {:?}",
                    if is_dir { "dir" } else { "file" },
                    child_path,
                );
                let task = worktree.create_entry(child_path, is_dir, None, cx);
                cx.background_spawn(async move {
                    task.await?;
                    Ok(())
                })
            } else {
                log::info!(
                    "overwriting file {:?} ({})",
                    &entry.path,
                    entry.id.to_usize()
                );
                let task = worktree.write_file(
                    entry.path.clone(),
                    "".into(),
                    Default::default(),
                    encoding_rs::UTF_8,
                    false,
                    cx,
                );
                cx.background_spawn(async move {
                    task.await?;
                    Ok(())
                })
            }
        }
    }
}

pub(crate) async fn randomly_mutate_fs(
    fs: &Arc<dyn Fs>,
    root_path: &Path,
    insertion_probability: f64,
    rng: &mut impl Rng,
) {
    log::info!("mutating fs");
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    for path in fs.as_fake().paths(false) {
        if path.starts_with(root_path) {
            if fs.is_file(&path).await {
                files.push(path);
            } else {
                dirs.push(path);
            }
        }
    }

    if (files.is_empty() && dirs.len() == 1) || rng.random_bool(insertion_probability) {
        let path = dirs.choose(rng).unwrap();
        let new_path = path.join(random_filename(rng));

        if rng.random() {
            log::info!(
                "creating dir {:?}",
                new_path.strip_prefix(root_path).unwrap()
            );
            fs.create_dir(&new_path).await.unwrap();
        } else {
            log::info!(
                "creating file {:?}",
                new_path.strip_prefix(root_path).unwrap()
            );
            fs.create_file(&new_path, Default::default()).await.unwrap();
        }
    } else if rng.random_bool(0.05) {
        let ignore_dir_path = dirs.choose(rng).unwrap();
        let ignore_path = ignore_dir_path.join(GITIGNORE);

        let subdirs = dirs
            .iter()
            .filter(|d| d.starts_with(ignore_dir_path))
            .cloned()
            .collect::<Vec<_>>();
        let subfiles = files
            .iter()
            .filter(|d| d.starts_with(ignore_dir_path))
            .cloned()
            .collect::<Vec<_>>();
        let files_to_ignore = {
            let len = rng.random_range(0..=subfiles.len());
            subfiles.choose_multiple(rng, len)
        };
        let dirs_to_ignore = {
            let len = rng.random_range(0..subdirs.len());
            subdirs.choose_multiple(rng, len)
        };

        let mut ignore_contents = String::new();
        for path_to_ignore in files_to_ignore.chain(dirs_to_ignore) {
            writeln!(
                ignore_contents,
                "{}",
                path_to_ignore
                    .strip_prefix(ignore_dir_path)
                    .unwrap()
                    .to_str()
                    .unwrap()
            )
            .unwrap();
        }
        log::info!(
            "creating gitignore {:?} with contents:\n{}",
            ignore_path.strip_prefix(root_path).unwrap(),
            ignore_contents
        );
        fs.save(
            &ignore_path,
            &ignore_contents.as_str().into(),
            Default::default(),
        )
        .await
        .unwrap();
    } else {
        let old_path = {
            let file_path = files.choose(rng);
            let dir_path = dirs[1..].choose(rng);
            file_path.into_iter().chain(dir_path).choose(rng).unwrap()
        };

        let is_rename = rng.random();
        if is_rename {
            let new_path_parent = dirs
                .iter()
                .filter(|d| !d.starts_with(old_path))
                .choose(rng)
                .unwrap();

            let overwrite_existing_dir =
                !old_path.starts_with(new_path_parent) && rng.random_bool(0.3);
            let new_path = if overwrite_existing_dir {
                fs.remove_dir(
                    new_path_parent,
                    RemoveOptions {
                        recursive: true,
                        ignore_if_not_exists: true,
                    },
                )
                .await
                .unwrap();
                new_path_parent.to_path_buf()
            } else {
                new_path_parent.join(random_filename(rng))
            };

            log::info!(
                "renaming {:?} to {}{:?}",
                old_path.strip_prefix(root_path).unwrap(),
                if overwrite_existing_dir {
                    "overwrite "
                } else {
                    ""
                },
                new_path.strip_prefix(root_path).unwrap()
            );
            fs.rename(
                old_path,
                &new_path,
                fs::RenameOptions {
                    overwrite: true,
                    ignore_if_exists: true,
                    create_parents: false,
                },
            )
            .await
            .unwrap();
        } else if fs.is_file(old_path).await {
            log::info!(
                "deleting file {:?}",
                old_path.strip_prefix(root_path).unwrap()
            );
            fs.remove_file(old_path, Default::default()).await.unwrap();
        } else {
            log::info!(
                "deleting dir {:?}",
                old_path.strip_prefix(root_path).unwrap()
            );
            fs.remove_dir(
                old_path,
                RemoveOptions {
                    recursive: true,
                    ignore_if_not_exists: true,
                },
            )
            .await
            .unwrap();
        }
    }
}

pub(crate) fn random_filename(rng: &mut impl Rng) -> String {
    (0..6)
        .map(|_| rng.sample(rand::distr::Alphanumeric))
        .map(char::from)
        .collect()
}
