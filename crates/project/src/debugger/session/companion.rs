use super::*;

impl Session {
    fn launch_browser_for_remote_server(
        &mut self,
        mut request: LaunchBrowserInCompanionParams,
        cx: &mut Context<Self>,
    ) {
        let Some(remote_client) = self.remote_client.clone() else {
            log::error!("can't launch browser in companion for non-remote project");
            return;
        };
        let Some(http_client) = self.http_client.clone() else {
            return;
        };
        let Some(node_runtime) = self.node_runtime.clone() else {
            return;
        };

        let mut console_output = self.console_output(cx);
        let task = cx.spawn(async move |this, cx| {
            let forward_ports_process = if remote_client
                .read_with(cx, |client, _| client.shares_network_interface())
            {
                request.other.insert(
                    "proxyUri".into(),
                    format!("127.0.0.1:{}", request.server_port).into(),
                );
                None
            } else {
                let port = TcpTransport::unused_port(IpAddr::V4(Ipv4Addr::LOCALHOST))
                    .await
                    .context("getting port for DAP")?;
                request
                    .other
                    .insert("proxyUri".into(), format!("127.0.0.1:{port}").into());
                let mut port_forwards = vec![(port, "localhost".to_owned(), request.server_port)];

                if let Some(value) = request.params.get("url")
                    && let Some(url) = value.as_str()
                    && let Some(url) = Url::parse(url).ok()
                    && let Some(frontend_port) = url.port()
                {
                    port_forwards.push((frontend_port, "localhost".to_owned(), frontend_port));
                }

                let child = remote_client.update(cx, |client, _| {
                    let command = client.build_forward_ports_command(port_forwards)?;
                    let child = new_command(command.program)
                        .args(command.args)
                        .envs(command.env)
                        .spawn()
                        .context("spawning port forwarding process")?;
                    anyhow::Ok(child)
                })?;
                Some(child)
            };

            let mut companion_process = None;
            let companion_port =
                if let Some(companion_port) = this.read_with(cx, |this, _| this.companion_port)? {
                    companion_port
                } else {
                    let task = cx.spawn(async move |cx| spawn_companion(node_runtime, cx).await);
                    match task.await {
                        Ok((port, child)) => {
                            companion_process = Some(child);
                            port
                        }
                        Err(e) => {
                            console_output
                                .send(format!("Failed to launch browser companion process: {e}"))
                                .await
                                .ok();
                            return Err(e);
                        }
                    }
                };

            let mut background_tasks = Vec::new();
            if let Some(mut forward_ports_process) = forward_ports_process {
                background_tasks.push(cx.spawn(async move |_| {
                    forward_ports_process.status().await.log_err();
                }));
            };
            if let Some(mut companion_process) = companion_process {
                if let Some(stderr) = companion_process.stderr.take() {
                    let mut console_output = console_output.clone();
                    background_tasks.push(cx.spawn(async move |_| {
                        let mut stderr = BufReader::new(stderr);
                        let mut line = String::new();
                        while let Ok(n) = stderr.read_line(&mut line).await
                            && n > 0
                        {
                            console_output
                                .send(format!("companion stderr: {line}"))
                                .await
                                .ok();
                            line.clear();
                        }
                    }));
                }
                background_tasks.push(cx.spawn({
                    let mut console_output = console_output.clone();
                    async move |_| match companion_process.status().await {
                        Ok(status) => {
                            if status.success() {
                                console_output
                                    .send("Companion process exited normally".into())
                                    .await
                                    .ok();
                            } else {
                                console_output
                                    .send(format!(
                                        "Companion process exited abnormally with {status:?}"
                                    ))
                                    .await
                                    .ok();
                            }
                        }
                        Err(e) => {
                            console_output
                                .send(format!("Failed to join companion process: {e}"))
                                .await
                                .ok();
                        }
                    }
                }));
            }

            // TODO pass wslInfo as needed

            let companion_address = format!("127.0.0.1:{companion_port}");
            let mut companion_started = false;
            for _ in 0..10 {
                if TcpStream::connect(&companion_address).await.is_ok() {
                    companion_started = true;
                    break;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
            }
            if !companion_started {
                console_output
                    .send("Browser companion failed to start".into())
                    .await
                    .ok();
                bail!("Browser companion failed to start");
            }

            let response = http_client
                .post_json(
                    &format!("http://{companion_address}/launch-and-attach"),
                    serde_json::to_string(&request)
                        .context("serializing request")?
                        .into(),
                )
                .await;
            match response {
                Ok(response) => {
                    if !response.status().is_success() {
                        console_output
                            .send("Launch request to companion failed".into())
                            .await
                            .ok();
                        return Err(anyhow!("launch request failed"));
                    }
                }
                Err(e) => {
                    console_output
                        .send("Failed to read response from companion".into())
                        .await
                        .ok();
                    return Err(e);
                }
            }

            this.update(cx, |this, _| {
                this.background_tasks.extend(background_tasks);
                this.companion_port = Some(companion_port);
            })?;

            anyhow::Ok(())
        });
        self.background_tasks.push(cx.spawn(async move |_, _| {
            task.await.log_err();
        }));
    }

    fn kill_browser(&self, request: KillCompanionBrowserParams, cx: &mut App) {
        let Some(companion_port) = self.companion_port else {
            log::error!("received killCompanionBrowser but js-debug-companion is not running");
            return;
        };
        let Some(http_client) = self.http_client.clone() else {
            return;
        };

        cx.spawn(async move |_| {
            http_client
                .post_json(
                    &format!("http://127.0.0.1:{companion_port}/kill"),
                    serde_json::to_string(&request)
                        .context("serializing request")?
                        .into(),
                )
                .await?;
            anyhow::Ok(())
        })
        .detach_and_log_err(cx)
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct LaunchBrowserInCompanionParams {
    server_port: u16,
    params: HashMap<String, serde_json::Value>,
    #[serde(flatten)]
    other: HashMap<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct KillCompanionBrowserParams {
    launch_id: u64,
}

async fn spawn_companion(
    node_runtime: NodeRuntime,
    cx: &mut AsyncApp,
) -> Result<(u16, util::command::Child)> {
    let binary_path = node_runtime
        .binary_path()
        .await
        .context("getting node path")?;
    let path = cx
        .spawn(async move |cx| get_or_install_companion(node_runtime, cx).await)
        .await?;
    log::info!("will launch js-debug-companion version {path:?}");

    let port = {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .context("getting port for companion")?;
        listener.local_addr()?.port()
    };

    let dir = paths::data_dir()
        .join("js_debug_companion_state")
        .to_string_lossy()
        .to_string();

    let child = new_command(binary_path)
        .arg(path)
        .args([
            format!("--listen=127.0.0.1:{port}"),
            format!("--state={dir}"),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawning companion child process")?;

    Ok((port, child))
}

async fn get_or_install_companion(node: NodeRuntime, cx: &mut AsyncApp) -> Result<PathBuf> {
    const PACKAGE_NAME: &str = "@mav-industries/js-debug-companion-cli";

    async fn install_latest_version(dir: PathBuf, node: NodeRuntime) -> Result<PathBuf> {
        let temp_dir = tempfile::tempdir().context("creating temporary directory")?;
        node.npm_install_latest_packages(temp_dir.path(), &[PACKAGE_NAME])
            .await
            .context("installing latest companion package")?;
        let version = node
            .npm_package_installed_version(temp_dir.path(), PACKAGE_NAME)
            .await
            .context("getting installed companion version")?
            .context("companion was not installed")?;
        let version_folder = dir.join(version.to_string());
        smol::fs::rename(temp_dir.path(), &version_folder)
            .await
            .context("moving companion package into place")?;
        Ok(version_folder)
    }

    let dir = paths::debug_adapters_dir().join("js-debug-companion");
    let (latest_installed_version, latest_version) = cx
        .background_spawn({
            let dir = dir.clone();
            let node = node.clone();
            async move {
                smol::fs::create_dir_all(&dir)
                    .await
                    .context("creating companion installation directory")?;

                let children = smol::fs::read_dir(&dir)
                    .await
                    .context("reading companion installation directory")?
                    .try_collect::<Vec<_>>()
                    .await
                    .context("reading companion installation directory entries")?;

                let latest_installed_version = children
                    .iter()
                    .filter_map(|child| {
                        Some((
                            child.path(),
                            semver::Version::parse(child.file_name().to_str()?).ok()?,
                        ))
                    })
                    .max_by_key(|(_, version)| version.clone());

                let latest_version = node
                    .npm_package_latest_version(PACKAGE_NAME)
                    .await
                    .log_err();
                anyhow::Ok((latest_installed_version, latest_version))
            }
        })
        .await?;

    let path = if let Some((installed_path, installed_version)) = latest_installed_version {
        if let Some(latest_version) = latest_version
            && latest_version != installed_version
        {
            cx.background_spawn(install_latest_version(dir.clone(), node.clone()))
                .detach();
        }
        Ok(installed_path)
    } else {
        cx.background_spawn(install_latest_version(dir.clone(), node.clone()))
            .await
    };

    Ok(path?
        .join("node_modules")
        .join(PACKAGE_NAME)
        .join("out")
        .join("cli.js"))
}
