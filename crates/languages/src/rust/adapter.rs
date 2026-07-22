use super::*;

#[cfg(target_os = "macos")]
impl RustLspAdapter {
    pub(super) const GITHUB_ASSET_KIND: AssetKind = AssetKind::Gz;
    const ARCH_SERVER_NAME: &str = "apple-darwin";
}

#[cfg(target_os = "linux")]
impl RustLspAdapter {
    pub(super) const GITHUB_ASSET_KIND: AssetKind = AssetKind::Gz;
    const ARCH_SERVER_NAME: &str = "unknown-linux";
}

#[cfg(target_os = "freebsd")]
impl RustLspAdapter {
    pub(super) const GITHUB_ASSET_KIND: AssetKind = AssetKind::Gz;
    const ARCH_SERVER_NAME: &str = "unknown-freebsd";
}

#[cfg(target_os = "windows")]
impl RustLspAdapter {
    pub(super) const GITHUB_ASSET_KIND: AssetKind = AssetKind::Zip;
    const ARCH_SERVER_NAME: &str = "pc-windows-msvc";
}

#[cfg(target_os = "linux")]
enum LibcType {
    Gnu,
    Musl,
}

impl RustLspAdapter {
    pub(super) fn convert_rust_analyzer_schema(
        raw_schema: &serde_json::Value,
    ) -> serde_json::Value {
        let Some(schema_array) = raw_schema.as_array() else {
            return raw_schema.clone();
        };

        let mut root_properties = serde_json::Map::new();

        for item in schema_array {
            if let Some(props) = item.get("properties").and_then(|p| p.as_object()) {
                for (key, value) in props {
                    let parts: Vec<&str> = key.split('.').collect();

                    if parts.is_empty() {
                        continue;
                    }

                    let parts_to_process = if parts.first() == Some(&"rust-analyzer") {
                        &parts[1..]
                    } else {
                        &parts[..]
                    };

                    if parts_to_process.is_empty() {
                        continue;
                    }

                    let mut current = &mut root_properties;

                    for (i, part) in parts_to_process.iter().enumerate() {
                        let is_last = i == parts_to_process.len() - 1;

                        if is_last {
                            current.insert(part.to_string(), value.clone());
                        } else {
                            let next_current = current
                                .entry(part.to_string())
                                .or_insert_with(|| {
                                    serde_json::json!({
                                        "type": "object",
                                        "properties": {}
                                    })
                                })
                                .as_object_mut()
                                .expect("should be an object")
                                .entry("properties")
                                .or_insert_with(|| serde_json::json!({}))
                                .as_object_mut()
                                .expect("properties should be an object");

                            current = next_current;
                        }
                    }
                }
            }
        }

        serde_json::json!({
            "type": "object",
            "properties": root_properties
        })
    }

    #[cfg(target_os = "linux")]
    async fn determine_libc_type() -> LibcType {
        use futures::pin_mut;

        async fn from_ldd_version() -> Option<LibcType> {
            use util::command::new_command;

            let ldd_output = new_command("ldd").arg("--version").output().await.ok()?;
            let ldd_version = String::from_utf8_lossy(&ldd_output.stdout);

            if ldd_version.contains("GNU libc") || ldd_version.contains("GLIBC") {
                Some(LibcType::Gnu)
            } else if ldd_version.contains("musl") {
                Some(LibcType::Musl)
            } else {
                None
            }
        }

        if let Some(libc_type) = from_ldd_version().await {
            return libc_type;
        }

        let Ok(dir_entries) = smol::fs::read_dir("/lib").await else {
            // defaulting to gnu because nix doesn't have /lib files due to not following FHS
            return LibcType::Gnu;
        };
        let dir_entries = dir_entries.filter_map(async move |e| e.ok());
        pin_mut!(dir_entries);

        let mut has_musl = false;
        let mut has_gnu = false;

        while let Some(entry) = dir_entries.next().await {
            let file_name = entry.file_name();
            let file_name = file_name.to_string_lossy();
            if file_name.starts_with("ld-musl-") {
                has_musl = true;
            } else if file_name.starts_with("ld-linux-") {
                has_gnu = true;
            }
        }

        match (has_musl, has_gnu) {
            (true, _) => LibcType::Musl,
            (_, true) => LibcType::Gnu,
            _ => LibcType::Gnu,
        }
    }

    #[cfg(target_os = "linux")]
    async fn build_arch_server_name_linux() -> String {
        let libc = match Self::determine_libc_type().await {
            LibcType::Musl => "musl",
            LibcType::Gnu => "gnu",
        };

        format!("{}-{}", Self::ARCH_SERVER_NAME, libc)
    }

    pub(super) async fn rustup_rust_analyzer_for_worktree(
        delegate: &dyn LspAdapterDelegate,
    ) -> Option<PathBuf> {
        if !Self::workspace_has_rust_toolchain_override(delegate).await {
            return None;
        }

        let rustup = delegate.which("rustup".as_ref()).await?;
        let env = delegate.shell_env().await;
        let worktree_root = delegate.worktree_root_path();
        let output = new_command(rustup)
            .args(["which", "rust-analyzer"])
            .envs(env.iter())
            .current_dir(worktree_root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await;
        let output = match output {
            Ok(output) if output.status.success() => output,
            Ok(output) => {
                log::debug!(
                    "failed to locate rust-analyzer through rustup in {worktree_root:?}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                return None;
            }
            Err(err) => {
                log::debug!(
                    "failed to run `rustup which rust-analyzer` in {worktree_root:?}: {err:#}"
                );
                return None;
            }
        };

        let path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
        Some(path).filter(|p| !p.as_os_str().is_empty())
    }

    async fn workspace_has_rust_toolchain_override(delegate: &dyn LspAdapterDelegate) -> bool {
        for file_name in ["rust-toolchain.toml", "rust-toolchain"] {
            if fs::metadata(delegate.resolve_relative_path(PathBuf::from(file_name)))
                .await
                .is_ok()
            {
                return true;
            }
        }

        false
    }

    pub(super) async fn build_asset_name() -> String {
        let extension = match Self::GITHUB_ASSET_KIND {
            AssetKind::TarGz => "tar.gz",
            AssetKind::TarBz2 => "tar.bz2",
            AssetKind::Gz => "gz",
            AssetKind::Zip => "zip",
        };

        #[cfg(target_os = "linux")]
        let arch_server_name = Self::build_arch_server_name_linux().await;
        #[cfg(not(target_os = "linux"))]
        let arch_server_name = Self::ARCH_SERVER_NAME.to_string();

        format!(
            "{}-{}-{}.{}",
            SERVER_NAME,
            std::env::consts::ARCH,
            &arch_server_name,
            extension
        )
    }
}
