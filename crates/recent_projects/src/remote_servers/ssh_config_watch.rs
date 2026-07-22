use super::*;

fn spawn_ssh_config_watch(fs: Arc<dyn Fs>, cx: &Context<RemoteServerProjects>) -> Task<()> {
    enum ConfigSource {
        User(String),
        Global(String),
    }

    let mut streams = Vec::new();
    let mut tasks = Vec::new();

    // Setup User Watcher
    let user_path = user_ssh_config_file();
    info!("SSH: Watching User Config at: {:?}", user_path);

    // We clone 'fs' here because we might need it again for the global watcher.
    let (user_s, user_t) = watch_config_file(cx.background_executor(), fs.clone(), user_path);
    streams.push(user_s.map(ConfigSource::User).boxed());
    tasks.push(user_t);

    // Setup Global Watcher
    if let Some(gp) = global_ssh_config_file() {
        info!("SSH: Watching Global Config at: {:?}", gp);
        let (global_s, global_t) =
            watch_config_file(cx.background_executor(), fs, gp.to_path_buf());
        streams.push(global_s.map(ConfigSource::Global).boxed());
        tasks.push(global_t);
    } else {
        debug!("SSH: No Global Config defined.");
    }

    // Combine into a single stream so that only one is parsed at once.
    let mut merged_stream = futures::stream::select_all(streams);

    cx.spawn(async move |remote_server_projects, cx| {
        let _tasks = tasks; // Keeps the background watchers alive
        let mut global_hosts = BTreeSet::default();
        let mut user_hosts = BTreeSet::default();

        while let Some(event) = merged_stream.next().await {
            match event {
                ConfigSource::Global(content) => {
                    global_hosts = parse_ssh_config_hosts(&content);
                }
                ConfigSource::User(content) => {
                    user_hosts = parse_ssh_config_hosts(&content);
                }
            }

            // Sync to Model
            if remote_server_projects
                .update(cx, |project, cx| {
                    project.ssh_config_servers = global_hosts
                        .iter()
                        .chain(user_hosts.iter())
                        .map(SharedString::from)
                        .collect();
                    let ssh_config_servers = project.ssh_config_servers.clone();
                    let (has_open_project, is_local) =
                        RemoteServerProjects::workspace_flags(&project.workspace, cx);
                    project.default_picker.update(cx, |picker, cx| {
                        picker
                            .delegate
                            .reload(&ssh_config_servers, has_open_project, is_local, cx);
                        cx.notify();
                    });
                    cx.notify();
                })
                .is_err()
            {
                return;
            }
        }
    })
}

fn get_text(element: &Entity<Editor>, cx: &mut App) -> String {
    element.read(cx).text(cx).trim().to_string()
}
