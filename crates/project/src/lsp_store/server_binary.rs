use super::*;

impl LocalLspStore {
    pub(super) fn get_language_server_binary(
        &self,
        worktree_abs_path: Arc<Path>,
        adapter: Arc<CachedLspAdapter>,
        settings: Arc<LspSettings>,
        toolchain: Option<Toolchain>,
        delegate: Arc<dyn LspAdapterDelegate>,
        allow_binary_download: bool,
        wait_until_worktree_trust: Option<watch::Receiver<bool>>,
        cx: &mut App,
    ) -> Task<Result<LanguageServerBinary>> {
        if let Some(settings) = &settings.binary
            && let Some(path) = settings.path.as_ref().map(PathBuf::from)
        {
            let settings = settings.clone();
            let languages = self.languages.clone();
            return cx.background_spawn(async move {
            if let Some(mut wait_until_worktree_trust) = wait_until_worktree_trust {
                let already_trusted =  *wait_until_worktree_trust.borrow();
                if !already_trusted {
                    log::info!(
                        "Waiting for worktree {worktree_abs_path:?} to be trusted, before starting language server {}",
                        adapter.name(),
                    );
                    while let Some(worktree_trusted) = wait_until_worktree_trust.recv().await {
                        if worktree_trusted {
                            break;
                        }
                    }
                    log::info!(
                        "Worktree {worktree_abs_path:?} is trusted, starting language server {}",
                        adapter.name(),
                    );
                }
                languages
                    .update_lsp_binary_status(adapter.name(), BinaryStatus::Starting);
            }
            let mut env = delegate.shell_env().await;
            env.extend(settings.env.unwrap_or_default());

            Ok(LanguageServerBinary {
                path: delegate.resolve_relative_path(path),
                env: Some(env),
                arguments: settings
                    .arguments
                    .unwrap_or_default()
                    .iter()
                    .map(Into::into)
                    .collect(),
            })
        });
        }

        #[cfg(any(test, feature = "test-support"))]
        if !adapter.adapter.is_extension() && self.languages.has_fake_lsp_server(&adapter.name) {
            let language_server_name = adapter.name.clone();
            let languages = self.languages.clone();
            return cx.spawn(async move |_| {
            if let Some(mut wait_until_worktree_trust) = wait_until_worktree_trust {
                let already_trusted = *wait_until_worktree_trust.borrow();
                if !already_trusted {
                    log::info!(
                        "Waiting for worktree {worktree_abs_path:?} to be trusted, before starting language server {language_server_name}",
                    );
                    while let Some(worktree_trusted) = wait_until_worktree_trust.recv().await {
                        if worktree_trusted {
                            break;
                        }
                    }
                    log::info!(
                        "Worktree {worktree_abs_path:?} is trusted, starting language server {language_server_name}",
                    );
                }
                languages.update_lsp_binary_status(
                    language_server_name.clone(),
                    BinaryStatus::Starting,
                );
            }

            Ok(LanguageServerBinary {
                path: PathBuf::from(format!("/fake/lsp/{language_server_name}")),
                arguments: Vec::new(),
                env: None,
            })
        });
        }

        if cfg!(any(test, feature = "test-support")) && !adapter.adapter.is_extension() {
            return Task::ready(Err(anyhow!(
                "language server binary lookup for {:?} is disabled in tests; register a fake language server or configure an explicit binary",
                adapter.name
            )));
        }

        let lsp_binary_options = LanguageServerBinaryOptions {
            allow_path_lookup: !settings
                .binary
                .as_ref()
                .and_then(|b| b.ignore_system_version)
                .unwrap_or_default(),
            allow_binary_download,
            pre_release: settings
                .fetch
                .as_ref()
                .and_then(|f| f.pre_release)
                .unwrap_or(false),
        };

        cx.spawn(async move |cx| {
            if let Some(mut wait_until_worktree_trust) = wait_until_worktree_trust {
                let already_trusted = *wait_until_worktree_trust.borrow();
                if !already_trusted {
                    log::info!(
                        "Waiting for worktree {worktree_abs_path:?} to be trusted, \
                    before starting language server {}",
                        adapter.name(),
                    );
                    while let Some(worktree_trusted) = wait_until_worktree_trust.recv().await {
                        if worktree_trusted {
                            break;
                        }
                    }
                    log::info!(
                        "Worktree {worktree_abs_path:?} is trusted, starting language server {}",
                        adapter.name(),
                    );
                }
            }

            let (existing_binary, maybe_download_binary) = adapter
                .clone()
                .get_language_server_command(delegate.clone(), toolchain, lsp_binary_options, cx)
                .await
                .await;

            delegate.update_status(adapter.name.clone(), BinaryStatus::None);

            let mut binary = match (existing_binary, maybe_download_binary) {
                (binary, None) => binary?,
                (Err(_), Some(downloader)) => downloader.await?,
                (Ok(existing_binary), Some(downloader)) => {
                    let mut download_timeout = cx
                        .background_executor()
                        .timer(SERVER_DOWNLOAD_TIMEOUT)
                        .fuse();
                    let mut downloader = downloader.fuse();
                    futures::select! {
                        _ = download_timeout => {
                            // Return existing binary and kick the existing work to the background.
                            cx.spawn(async move |_| downloader.await).detach();
                            Ok(existing_binary)
                        },
                        downloaded_or_existing_binary = downloader => {
                            // If download fails, this results in the existing binary.
                            downloaded_or_existing_binary
                        }
                    }?
                }
            };
            let mut shell_env = delegate.shell_env().await;

            shell_env.extend(binary.env.unwrap_or_default());

            if let Some(settings) = settings.binary.as_ref() {
                if let Some(arguments) = &settings.arguments {
                    binary.arguments = arguments.iter().map(Into::into).collect();
                }
                if let Some(env) = &settings.env {
                    shell_env.extend(env.iter().map(|(k, v)| (k.clone(), v.clone())));
                }
            }

            binary.env = Some(shell_env);
            Ok(binary)
        })
    }
}
