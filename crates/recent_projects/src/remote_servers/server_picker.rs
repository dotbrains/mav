use super::filter::{FilterData, FilteredServer};
use super::*;
mod delegate;

#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(super) struct SshServerIndex(pub(super) usize);
impl std::fmt::Display for SshServerIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(super) struct WslServerIndex(pub(super) usize);
impl std::fmt::Display for WslServerIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(super) enum ServerIndex {
    Ssh(SshServerIndex),
    Wsl(WslServerIndex),
}
impl From<SshServerIndex> for ServerIndex {
    fn from(index: SshServerIndex) -> Self {
        Self::Ssh(index)
    }
}
impl From<WslServerIndex> for ServerIndex {
    fn from(index: WslServerIndex) -> Self {
        Self::Wsl(index)
    }
}

#[derive(Clone)]
struct ProjectEntry {
    pub(super) project: RemoteProject,
}

#[derive(Clone)]
pub(super) enum RemoteEntry {
    Project {
        projects: Vec<ProjectEntry>,
        connection: Connection,
        index: ServerIndex,
    },
    SshConfig {
        host: SharedString,
    },
}

impl RemoteEntry {
    fn display_host(&self) -> &str {
        match self {
            Self::Project { connection, .. } => match connection {
                Connection::Ssh(c) => c.nickname.as_deref().unwrap_or(&c.host),
                Connection::Wsl(c) => &c.distro_name,
                Connection::DevContainer(c) => &c.name,
            },
            Self::SshConfig { host, .. } => host,
        }
    }

    /// Extra text to match against that isn't shown in the primary label.
    /// When an SSH connection has a nickname, [`display_host`] surfaces the
    /// nickname and the real host is only shown as a muted aux label, so we
    /// index the host here to keep it searchable.
    fn host_alias(&self) -> Option<&str> {
        match self {
            Self::Project {
                connection: Connection::Ssh(c),
                ..
            } if c.nickname.is_some() => Some(&c.host),
            _ => None,
        }
    }

    fn connection(&self) -> Cow<'_, Connection> {
        match self {
            Self::Project { connection, .. } => Cow::Borrowed(connection),
            Self::SshConfig { host, .. } => Cow::Owned(
                SshConnection {
                    host: host.to_string(),
                    ..SshConnection::default()
                }
                .into(),
            ),
        }
    }
}

#[derive(Clone)]
pub(super) struct DefaultState {
    pub(super) servers: Vec<RemoteEntry>,
    /// `None` when no filter is active; `Some` carries the fuzzy match results
    /// (server/project indices plus highlight positions) sorted by score.
    pub(super) filtered_servers: Option<Vec<FilteredServer>>,
    pub(super) filter_data: Arc<FilterData>,
}

impl DefaultState {
    pub(super) fn new(ssh_config_servers: &BTreeSet<SharedString>, cx: &mut App) -> Self {
        let ssh_settings = RemoteSettings::get_global(cx);
        let read_ssh_config = ssh_settings.read_ssh_config;

        let ssh_servers = ssh_settings
            .ssh_connections()
            .enumerate()
            .map(|(index, connection)| {
                let projects = connection
                    .projects
                    .iter()
                    .map(|project| ProjectEntry {
                        project: project.clone(),
                    })
                    .collect();
                RemoteEntry::Project {
                    projects,
                    index: ServerIndex::Ssh(SshServerIndex(index)),
                    connection: connection.into(),
                }
            });

        let wsl_servers = ssh_settings
            .wsl_connections()
            .enumerate()
            .map(|(index, connection)| {
                let projects = connection
                    .projects
                    .iter()
                    .map(|project| ProjectEntry {
                        project: project.clone(),
                    })
                    .collect();
                RemoteEntry::Project {
                    projects,
                    index: ServerIndex::Wsl(WslServerIndex(index)),
                    connection: connection.into(),
                }
            });

        let mut servers = ssh_servers.chain(wsl_servers).collect::<Vec<RemoteEntry>>();

        if read_ssh_config {
            let mut extra_servers_from_config = ssh_config_servers.clone();
            for server in &servers {
                if let RemoteEntry::Project {
                    connection: Connection::Ssh(ssh_options),
                    ..
                } = server
                {
                    extra_servers_from_config.remove(&SharedString::new(ssh_options.host.clone()));
                }
            }
            servers.extend(
                extra_servers_from_config
                    .into_iter()
                    .map(|host| RemoteEntry::SshConfig { host }),
            );
        }

        let filter_data = Arc::new(FilterData::build(&servers));
        Self {
            servers,
            filtered_servers: None,
            filter_data,
        }
    }

    pub(super) fn filter_sync(&mut self, query: &str) {
        if query.is_empty() {
            self.filtered_servers = None;
            return;
        }
        self.filtered_servers = Some(filter::run_sync(&self.filter_data, query));
    }
}

#[derive(Clone)]
pub(super) enum ViewServerOptionsState {
    Ssh {
        connection: SshConnectionOptions,
        server_index: SshServerIndex,
        entries: [NavigableEntry; 4],
    },
    Wsl {
        connection: WslConnectionOptions,
        server_index: WslServerIndex,
        entries: [NavigableEntry; 2],
    },
}

impl ViewServerOptionsState {
    pub(super) fn entries(&self) -> &[NavigableEntry] {
        match self {
            Self::Ssh { entries, .. } => entries,
            Self::Wsl { entries, .. } => entries,
        }
    }
}

pub(super) enum Mode {
    Default,
    ViewServerOptions(ViewServerOptionsState),
    EditNickname(EditNicknameState),
    ProjectPicker(Entity<ProjectPicker>),
    CreateRemoteServer(CreateRemoteServer),
    CreateRemoteDevContainer(CreateRemoteDevContainer),
    #[cfg(target_os = "windows")]
    AddWslDistro(AddWslDistro),
}

impl Mode {
    /// The default mode is backed by [`RemoteServerProjects::default_picker`],
    /// which is rebuilt from settings independently, so this just selects the
    /// variant and ignores its arguments.
    pub(super) fn default_mode(
        _ssh_config_servers: &BTreeSet<SharedString>,
        _cx: &mut App,
    ) -> Self {
        Self::Default
    }
}

pub(super) enum RemoteMatch {
    AddServer,
    AddDevContainer,
    AddWsl,
    Separator,
    ServerHeader {
        server: usize,
        host_positions: Vec<usize>,
    },
    Project {
        server: usize,
        project: usize,
        positions: Vec<usize>,
    },
    OpenFolder {
        server: usize,
    },
    ViewServerOptions {
        server: usize,
    },
}

impl RemoteMatch {
    fn is_selectable(&self) -> bool {
        !matches!(
            self,
            RemoteMatch::Separator | RemoteMatch::ServerHeader { .. }
        )
    }
}

pub(super) struct RemoteServerPickerDelegate {
    remote_server_projects: WeakEntity<RemoteServerProjects>,
    state: DefaultState,
    matches: Vec<RemoteMatch>,
    selected_index: usize,
    query: String,
    has_open_project: bool,
    is_local: bool,
}

impl RemoteServerPickerDelegate {
    pub(super) fn new(
        remote_server_projects: WeakEntity<RemoteServerProjects>,
        ssh_config_servers: &BTreeSet<SharedString>,
        has_open_project: bool,
        is_local: bool,
        cx: &mut App,
    ) -> Self {
        let mut this = Self {
            remote_server_projects,
            state: DefaultState::new(ssh_config_servers, cx),
            matches: Vec::new(),
            selected_index: 0,
            query: String::new(),
            has_open_project,
            is_local,
        };
        this.rebuild_matches();
        this
    }

    pub(super) fn reload(
        &mut self,
        ssh_config_servers: &BTreeSet<SharedString>,
        has_open_project: bool,
        is_local: bool,
        cx: &mut App,
    ) {
        self.has_open_project = has_open_project;
        self.is_local = is_local;
        self.state = DefaultState::new(ssh_config_servers, cx);
        // Settings/ssh-config changes are rare, so re-applying the active query
        // synchronously here is fine; the per-keystroke path filters off-thread.
        self.state.filter_sync(self.query.trim());
        self.rebuild_matches();
    }

    /// Flattens the current (already-filtered) `DefaultState` into the picker's
    /// match list. The fuzzy filtering itself runs separately (off-thread on the
    /// keystroke path, see [`Self::update_matches`]); this only reads
    /// [`DefaultState::filtered_servers`].
    fn rebuild_matches(&mut self) {
        let has_open_project = self.has_open_project;
        let is_local = self.is_local;

        let mut matches = Vec::new();
        if self.query.trim().is_empty() {
            matches.push(RemoteMatch::AddServer);
            if has_open_project && is_local {
                matches.push(RemoteMatch::AddDevContainer);
            }
            if cfg!(target_os = "windows") {
                matches.push(RemoteMatch::AddWsl);
            }
        }

        let push_server = |matches: &mut Vec<RemoteMatch>,
                           server_index: usize,
                           server: &RemoteEntry,
                           host_positions: Vec<usize>,
                           project_matches: Vec<(usize, Vec<usize>)>| {
            if !matches.is_empty() {
                matches.push(RemoteMatch::Separator);
            }
            matches.push(RemoteMatch::ServerHeader {
                server: server_index,
                host_positions,
            });
            match server {
                RemoteEntry::Project { .. } => {
                    for (project, positions) in project_matches {
                        matches.push(RemoteMatch::Project {
                            server: server_index,
                            project,
                            positions,
                        });
                    }
                    matches.push(RemoteMatch::OpenFolder {
                        server: server_index,
                    });
                    matches.push(RemoteMatch::ViewServerOptions {
                        server: server_index,
                    });
                }
                RemoteEntry::SshConfig { .. } => {
                    matches.push(RemoteMatch::OpenFolder {
                        server: server_index,
                    });
                }
            }
        };

        match &self.state.filtered_servers {
            None => {
                for (server_index, server) in self.state.servers.iter().enumerate() {
                    let project_matches = match server {
                        RemoteEntry::Project { projects, .. } => {
                            (0..projects.len()).map(|p| (p, Vec::new())).collect()
                        }
                        RemoteEntry::SshConfig { .. } => Vec::new(),
                    };
                    push_server(
                        &mut matches,
                        server_index,
                        server,
                        Vec::new(),
                        project_matches,
                    );
                }
            }
            Some(results) => {
                for filtered in results {
                    let server_index = filtered.server_index;
                    let Some(server) = self.state.servers.get(server_index) else {
                        continue;
                    };
                    let project_matches = filtered
                        .project_matches
                        .iter()
                        .map(|pm| (pm.project_index, pm.path_positions.clone()))
                        .collect();
                    push_server(
                        &mut matches,
                        server_index,
                        server,
                        filtered.host_positions.clone(),
                        project_matches,
                    );
                }
            }
        }

        self.matches = matches;
        self.selected_index = self
            .matches
            .iter()
            .position(RemoteMatch::is_selectable)
            .unwrap_or(0);
    }

    fn render_server_header(
        &self,
        server_index: usize,
        host_positions: &[usize],
    ) -> Option<AnyElement> {
        let server = self.state.servers.get(server_index)?;
        let connection = server.connection().into_owned();
        let (main_label, aux_label, is_wsl) = match &connection {
            Connection::Ssh(connection) => {
                if let Some(nickname) = connection.nickname.clone() {
                    let aux_label = SharedString::from(format!("({})", connection.host));
                    (nickname, Some(aux_label), false)
                } else {
                    (connection.host.clone(), None, false)
                }
            }
            Connection::Wsl(connection) => (connection.distro_name.clone(), None, true),
            Connection::DevContainer(connection) => (connection.name.clone(), None, false),
        };
        Some(
            h_flex()
                .w_full()
                .pt_1()
                .px_3()
                .gap_1()
                .overflow_hidden()
                .child(
                    h_flex()
                        .gap_1()
                        .max_w_96()
                        .overflow_hidden()
                        .text_ellipsis()
                        .when(is_wsl, |this| {
                            this.child(
                                Label::new("WSL:")
                                    .size(LabelSize::Small)
                                    .color(Color::Muted),
                            )
                        })
                        .child(
                            HighlightedLabel::new(main_label, host_positions.to_vec())
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        ),
                )
                .children(
                    aux_label
                        .map(|label| Label::new(label).size(LabelSize::Small).color(Color::Muted)),
                )
                .into_any_element(),
        )
    }

    fn render_action_item(
        &self,
        ix: usize,
        icon: IconName,
        label: &'static str,
        selected: bool,
    ) -> AnyElement {
        ListItem::new(("remote-action", ix))
            .toggle_state(selected)
            .inset(true)
            .spacing(ui::ListItemSpacing::Sparse)
            .start_slot(Icon::new(icon).color(Color::Muted))
            .child(Label::new(label))
            .into_any_element()
    }
}
