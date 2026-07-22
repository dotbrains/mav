use super::*;

pub async fn copy_recursive<'a>(
    fs: &'a dyn Fs,
    source: &'a Path,
    target: &'a Path,
    options: CopyOptions,
) -> Result<()> {
    for (item, is_dir) in read_dir_items(fs, source).await? {
        let Ok(item_relative_path) = item.strip_prefix(source) else {
            continue;
        };
        let target_item = if item_relative_path == Path::new("") {
            target.to_path_buf()
        } else {
            target.join(item_relative_path)
        };
        if is_dir {
            if !options.overwrite && fs.metadata(&target_item).await.is_ok_and(|m| m.is_some()) {
                if options.ignore_if_exists {
                    continue;
                } else {
                    anyhow::bail!("{target_item:?} already exists");
                }
            }
            let _ = fs
                .remove_dir(
                    &target_item,
                    RemoveOptions {
                        recursive: true,
                        ignore_if_not_exists: true,
                    },
                )
                .await;
            fs.create_dir(&target_item).await?;
        } else {
            fs.copy_file(&item, &target_item, options).await?;
        }
    }
    Ok(())
}

/// Recursively reads all of the paths in the given directory.
///
/// Returns a vector of tuples of (path, is_dir).
pub async fn read_dir_items<'a>(fs: &'a dyn Fs, source: &'a Path) -> Result<Vec<(PathBuf, bool)>> {
    let mut items = Vec::new();
    read_recursive(fs, source, &mut items).await?;
    Ok(items)
}

fn read_recursive<'a>(
    fs: &'a dyn Fs,
    source: &'a Path,
    output: &'a mut Vec<(PathBuf, bool)>,
) -> BoxFuture<'a, Result<()>> {
    use futures::future::FutureExt;

    async move {
        let metadata = fs
            .metadata(source)
            .await?
            .with_context(|| format!("path does not exist: {source:?}"))?;

        if metadata.is_dir {
            output.push((source.to_path_buf(), true));
            let mut children = fs.read_dir(source).await?;
            while let Some(child_path) = children.next().await {
                if let Ok(child_path) = child_path {
                    read_recursive(fs, &child_path, output).await?;
                }
            }
        } else {
            output.push((source.to_path_buf(), false));
        }
        Ok(())
    }
    .boxed()
}
