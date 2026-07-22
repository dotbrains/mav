use super::*;

impl LocalWorktree {
    /// Find the lowest path in the worktree's datastructures that is an ancestor
    pub(super) fn lowest_ancestor(&self, path: &RelPath) -> Arc<RelPath> {
        let mut lowest_ancestor = None;
        for path in path.ancestors() {
            if self.entry_for_path(path).is_some() {
                lowest_ancestor = Some(path.into());
                break;
            }
        }

        lowest_ancestor.unwrap_or_else(|| RelPath::empty_arc())
    }

    pub fn create_entry(
        &self,
        path: Arc<RelPath>,
        is_dir: bool,
        content: Option<Vec<u8>>,
        cx: &Context<Worktree>,
    ) -> Task<Result<CreatedEntry>> {
        let abs_path = self.absolutize(&path);
        let path_excluded = self.settings.is_path_excluded(&path);
        let fs = self.fs.clone();
        let task_abs_path = abs_path.clone();
        let write = cx.background_spawn(async move {
            if is_dir {
                fs.create_dir(&task_abs_path)
                    .await
                    .with_context(|| format!("creating directory {task_abs_path:?}"))
            } else {
                fs.write(&task_abs_path, content.as_deref().unwrap_or(&[]))
                    .await
                    .with_context(|| format!("creating file {task_abs_path:?}"))
            }
        });

        let lowest_ancestor = self.lowest_ancestor(&path);
        cx.spawn(async move |this, cx| {
            write.await?;
            if path_excluded {
                return Ok(CreatedEntry::Excluded { abs_path });
            }

            let (result, refreshes) = this.update(cx, |this, cx| {
                let mut refreshes = Vec::new();
                let refresh_paths = path.strip_prefix(&lowest_ancestor).unwrap();
                for refresh_path in refresh_paths.ancestors() {
                    if refresh_path == RelPath::empty() {
                        continue;
                    }
                    let refresh_full_path = lowest_ancestor.join(refresh_path);

                    refreshes.push(this.as_local_mut().unwrap().refresh_entry(
                        refresh_full_path,
                        None,
                        cx,
                    ));
                }
                (
                    this.as_local_mut().unwrap().refresh_entry(path, None, cx),
                    refreshes,
                )
            })?;
            for refresh in refreshes {
                refresh.await.log_err();
            }

            Ok(result
                .await?
                .map(CreatedEntry::Included)
                .unwrap_or_else(|| CreatedEntry::Excluded { abs_path }))
        })
    }

    pub fn write_file(
        &self,
        path: Arc<RelPath>,
        text: Rope,
        line_ending: LineEnding,
        encoding: &'static Encoding,
        has_bom: bool,
        cx: &Context<Worktree>,
    ) -> Task<Result<Arc<File>>> {
        let fs = self.fs.clone();
        let is_private = self.is_path_private(&path);
        let abs_path = self.absolutize(&path);

        let write = cx.background_spawn({
            let fs = fs.clone();
            let abs_path = abs_path.clone();
            async move {
                // For UTF-8, use the optimized `fs.save` which writes Rope chunks directly to disk
                // without allocating a contiguous string.
                if encoding == encoding_rs::UTF_8 && !has_bom {
                    return fs.save(&abs_path, &text, line_ending).await;
                }

                // For legacy encodings (e.g. Shift-JIS), we fall back to converting the entire Rope
                // to a String/Bytes in memory before writing.
                //
                // Note: This is inefficient for very large files compared to the streaming approach above,
                // but supporting streaming writes for arbitrary encodings would require a significant
                // refactor of the `fs` crate to expose a Writer interface.
                let text_string = text.to_string();
                let normalized_text = match line_ending {
                    LineEnding::Unix => text_string,
                    LineEnding::Windows => text_string.replace('\n', "\r\n"),
                };

                // Create the byte vector manually for UTF-16 encodings because encoding_rs encodes to UTF-8 by default (per WHATWG standards),
                //  which is not what we want for saving files.
                let bytes = if encoding == encoding_rs::UTF_16BE {
                    let mut data = Vec::with_capacity(normalized_text.len() * 2 + 2);
                    if has_bom {
                        data.extend_from_slice(&[0xFE, 0xFF]); // BOM
                    }
                    let utf16be_bytes =
                        normalized_text.encode_utf16().flat_map(|u| u.to_be_bytes());
                    data.extend(utf16be_bytes);
                    data.into()
                } else if encoding == encoding_rs::UTF_16LE {
                    let mut data = Vec::with_capacity(normalized_text.len() * 2 + 2);
                    if has_bom {
                        data.extend_from_slice(&[0xFF, 0xFE]); // BOM
                    }
                    let utf16le_bytes =
                        normalized_text.encode_utf16().flat_map(|u| u.to_le_bytes());
                    data.extend(utf16le_bytes);
                    data.into()
                } else {
                    // For other encodings (Shift-JIS, UTF-8 with BOM, etc.), delegate to encoding_rs.
                    let bom_bytes = if has_bom {
                        if encoding == encoding_rs::UTF_8 {
                            vec![0xEF, 0xBB, 0xBF]
                        } else {
                            vec![]
                        }
                    } else {
                        vec![]
                    };
                    let (cow, _, _) = encoding.encode(&normalized_text);
                    if !bom_bytes.is_empty() {
                        let mut bytes = bom_bytes;
                        bytes.extend_from_slice(&cow);
                        bytes.into()
                    } else {
                        cow
                    }
                };

                fs.write(&abs_path, &bytes).await
            }
        });

        cx.spawn(async move |this, cx| {
            write.await?;
            let entry = this
                .update(cx, |this, cx| {
                    this.as_local_mut()
                        .unwrap()
                        .refresh_entry(path.clone(), None, cx)
                })?
                .await?;
            let worktree = this.upgrade().context("worktree dropped")?;
            if let Some(entry) = entry {
                Ok(File::for_entry(entry, worktree))
            } else {
                let metadata = fs
                    .metadata(&abs_path)
                    .await
                    .with_context(|| {
                        format!("Fetching metadata after saving the excluded buffer {abs_path:?}")
                    })?
                    .with_context(|| {
                        format!("Excluded buffer {path:?} got removed during saving")
                    })?;
                Ok(Arc::new(File {
                    worktree,
                    path,
                    disk_state: DiskState::Present {
                        mtime: metadata.mtime,
                        size: metadata.len,
                    },
                    entry_id: None,
                    is_local: true,
                    is_private,
                }))
            }
        })
    }

    pub fn delete_entry(
        &self,
        entry_id: ProjectEntryId,
        trash: bool,
        cx: &Context<Worktree>,
    ) -> Option<Task<Result<Option<TrashedEntry>>>> {
        let entry = self.entry_for_id(entry_id)?.clone();
        let abs_path = self.absolutize(&entry.path);
        let fs = self.fs.clone();

        let delete = cx.background_spawn(async move {
            let trashed_entry = match (entry.is_file(), trash) {
                (true, true) => Some(fs.trash(&abs_path, Default::default()).await?),
                (false, true) => Some(
                    fs.trash(
                        &abs_path,
                        RemoveOptions {
                            recursive: true,
                            ignore_if_not_exists: false,
                        },
                    )
                    .await?,
                ),
                (true, false) => {
                    fs.remove_file(&abs_path, Default::default()).await?;
                    None
                }
                (false, false) => {
                    fs.remove_dir(
                        &abs_path,
                        RemoveOptions {
                            recursive: true,
                            ignore_if_not_exists: false,
                        },
                    )
                    .await?;
                    None
                }
            };

            anyhow::Ok((trashed_entry, entry.path))
        });

        Some(cx.spawn(async move |this, cx| {
            let (trashed_entry, path) = delete.await?;
            this.update(cx, |this, _| {
                this.as_local_mut()
                    .unwrap()
                    .refresh_entries_for_paths(vec![path])
            })?
            .recv()
            .await;

            Ok(trashed_entry)
        }))
    }

    pub async fn restore_entry(
        trash_entry: TrashedEntry,
        this: Entity<Worktree>,
        cx: &mut AsyncApp,
    ) -> Result<RelPathBuf> {
        let Some((fs, worktree_abs_path, path_style)) = this.read_with(cx, |this, _cx| {
            let local_worktree = match this {
                Worktree::Local(local_worktree) => local_worktree,
                Worktree::Remote(_) => return None,
            };

            let fs = local_worktree.fs.clone();
            let path_style = local_worktree.path_style();
            Some((fs, Arc::clone(local_worktree.abs_path()), path_style))
        }) else {
            return Err(anyhow!("Localworktree should not change into a remote one"));
        };

        let path_buf = fs.restore(trash_entry).await?;
        let path = path_buf
            .strip_prefix(worktree_abs_path)
            .context("Could not strip prefix")?;
        let path = RelPath::new(&path, path_style)?;
        let path = path.into_owned();

        Ok(path)
    }

    pub fn copy_external_entries(
        &self,
        target_directory: Arc<RelPath>,
        paths: Vec<Arc<Path>>,
        cx: &Context<Worktree>,
    ) -> Task<Result<Vec<ProjectEntryId>>> {
        let target_directory = self.absolutize(&target_directory);
        let worktree_path = self.abs_path().clone();
        let fs = self.fs.clone();
        let paths = paths
            .into_iter()
            .filter_map(|source| {
                let file_name = source.file_name()?;
                let mut target = target_directory.clone();
                target.push(file_name);

                // Do not allow copying the same file to itself.
                if source.as_ref() != target.as_path() {
                    Some((source, target))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        let paths_to_refresh = paths
            .iter()
            .filter_map(|(_, target)| {
                RelPath::new(
                    target.strip_prefix(&worktree_path).ok()?,
                    PathStyle::local(),
                )
                .ok()
                .map(|path| path.into_arc())
            })
            .collect::<Vec<_>>();

        cx.spawn(async move |this, cx| {
            cx.background_spawn(async move {
                for (source, target) in paths {
                    copy_recursive(
                        fs.as_ref(),
                        &source,
                        &target,
                        fs::CopyOptions {
                            overwrite: true,
                            ..Default::default()
                        },
                    )
                    .await
                    .with_context(|| {
                        format!("Failed to copy file from {source:?} to {target:?}")
                    })?;
                }
                anyhow::Ok(())
            })
            .await
            .log_err();
            let mut refresh = cx.read_entity(
                &this.upgrade().with_context(|| "Dropped worktree")?,
                |this, _| {
                    anyhow::Ok::<postage::barrier::Receiver>(
                        this.as_local()
                            .with_context(|| "Worktree is not local")?
                            .refresh_entries_for_paths(paths_to_refresh.clone()),
                    )
                },
            )?;

            cx.background_spawn(async move {
                refresh.next().await;
                anyhow::Ok(())
            })
            .await
            .log_err();

            let this = this.upgrade().with_context(|| "Dropped worktree")?;
            Ok(cx.read_entity(&this, |this, _| {
                paths_to_refresh
                    .iter()
                    .filter_map(|path| Some(this.entry_for_path(path)?.id))
                    .collect()
            }))
        })
    }
}
