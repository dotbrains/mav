use super::*;

pub(super) async fn get_cached_server_binary(
    container_dir: PathBuf,
) -> Option<LanguageServerBinary> {
    let binary_result = maybe!(async {
        let mut last = None;
        let mut entries = fs::read_dir(&container_dir)
            .await
            .with_context(|| format!("listing {container_dir:?}"))?;
        while let Some(entry) = entries.next().await {
            let path = entry?.path();
            if path.extension().is_some_and(|ext| ext == "metadata") {
                continue;
            }
            last = Some(path);
        }

        let path = match last {
            Some(last) => last,
            None => return Ok(None),
        };
        let path = match RustLspAdapter::GITHUB_ASSET_KIND {
            AssetKind::TarGz | AssetKind::TarBz2 | AssetKind::Gz => path, // Tar and gzip extract in place.
            AssetKind::Zip => path.join("rust-analyzer.exe"),             // zip contains a .exe
        };

        anyhow::Ok(Some(LanguageServerBinary {
            path,
            env: None,
            arguments: Vec::new(),
        }))
    })
    .await;

    match binary_result {
        Ok(Some(binary)) => Some(binary),
        Ok(None) => {
            log::info!("No cached rust-analyzer binary found");
            None
        }
        Err(e) => {
            log::error!("Failed to look up cached rust-analyzer binary: {e:#}");
            None
        }
    }
}
