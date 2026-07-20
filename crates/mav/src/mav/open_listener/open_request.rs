use super::{RawOpenRequest, connect_to_cli};
use agent_ui::ExternalSourcePrompt;
use anyhow::{Context as _, Result};
use cli::{CliRequest, CliResponseSink};
use client::{MavLink, parse_mav_link};
use futures::channel::mpsc;
use gpui::App;
use recent_projects::RemoteSettings;
use remote::{RemoteConnectionOptions, WslConnectionOptions};
use settings::Settings;
use ui::SharedString;
use util::ResultExt;

#[derive(Default, Debug)]
pub struct OpenRequest {
    pub kind: Option<OpenRequestKind>,
    pub open_paths: Vec<String>,
    pub diff_paths: Vec<[String; 2]>,
    pub diff_all: bool,
    pub dev_container: bool,
    pub open_channel_notes: Vec<(u64, Option<String>)>,
    pub join_channel: Option<u64>,
    pub remote_connection: Option<RemoteConnectionOptions>,
    pub open_behavior: Option<cli::OpenBehavior>,
}

pub enum OpenRequestKind {
    CliConnection(
        (
            mpsc::UnboundedReceiver<CliRequest>,
            Box<dyn CliResponseSink>,
        ),
    ),
    FocusApp,
    Extension {
        extension_id: String,
    },
    AgentPanel {
        external_source_prompt: Option<ExternalSourcePrompt>,
    },
    InstallSkill {
        /// Full `SKILL.md` contents embedded in a `mav://skill` share link.
        content: String,
    },
    DockMenuAction {
        index: usize,
    },
    BuiltinJsonSchema {
        schema_path: String,
    },
    Setting {
        /// `None` opens settings without navigating to a specific path.
        setting_path: Option<String>,
    },
    GitClone {
        repo_url: SharedString,
    },
    GitCommit {
        sha: String,
    },
}

impl std::fmt::Debug for OpenRequestKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CliConnection(_) => write!(f, "CliConnection(..)"),
            Self::FocusApp => write!(f, "FocusApp"),
            Self::Extension { extension_id } => f
                .debug_struct("Extension")
                .field("extension_id", extension_id)
                .finish(),
            Self::AgentPanel {
                external_source_prompt,
            } => f
                .debug_struct("AgentPanel")
                .field("external_source_prompt", external_source_prompt)
                .finish(),
            Self::InstallSkill { content } => f
                .debug_struct("InstallSkill")
                .field("content_len", &content.len())
                .finish(),
            Self::DockMenuAction { index } => f
                .debug_struct("DockMenuAction")
                .field("index", index)
                .finish(),
            Self::BuiltinJsonSchema { schema_path } => f
                .debug_struct("BuiltinJsonSchema")
                .field("schema_path", schema_path)
                .finish(),
            Self::Setting { setting_path } => f
                .debug_struct("Setting")
                .field("setting_path", setting_path)
                .finish(),
            Self::GitClone { repo_url } => f
                .debug_struct("GitClone")
                .field("repo_url", repo_url)
                .finish(),
            Self::GitCommit { sha } => f.debug_struct("GitCommit").field("sha", sha).finish(),
        }
    }
}

impl OpenRequest {
    pub fn is_focus_app_only(&self) -> bool {
        matches!(self.kind, Some(OpenRequestKind::FocusApp))
            && self.open_paths.is_empty()
            && self.diff_paths.is_empty()
            && self.remote_connection.is_none()
            && self.join_channel.is_none()
            && self.open_channel_notes.is_empty()
    }

    pub fn parse(request: RawOpenRequest, cx: &App) -> Result<Self> {
        let mut this = Self::default();

        this.diff_paths = request.diff_paths;
        this.diff_all = request.diff_all;
        this.dev_container = request.dev_container;
        this.open_behavior = request.open_behavior;
        if let Some(wsl) = request.wsl {
            let (user, distro_name) = if let Some((user, distro)) = wsl.split_once('@') {
                if user.is_empty() {
                    anyhow::bail!("user is empty in wsl argument");
                }
                (Some(user.to_string()), distro.to_string())
            } else {
                (None, wsl)
            };
            this.remote_connection = Some(RemoteConnectionOptions::Wsl(WslConnectionOptions {
                distro_name,
                user,
            }));
        }

        for url in request.urls {
            if let Some(server_name) = url.strip_prefix("mav-cli://") {
                this.kind = Some(OpenRequestKind::CliConnection(connect_to_cli(server_name)?));
            } else if let Some(action_index) = url.strip_prefix("mav-dock-action://") {
                this.kind = Some(OpenRequestKind::DockMenuAction {
                    index: action_index.parse()?,
                });
            } else if let Some(file) = url.strip_prefix("file://") {
                this.parse_file_path(file)
            } else if let Some(file) = url.strip_prefix("mav://file") {
                this.parse_file_path(file)
            } else if let Some(file) = url.strip_prefix("mav://ssh") {
                let ssh_url = "ssh:/".to_string() + file;
                this.parse_ssh_file_path(&ssh_url, cx)?
            } else if let Some(extension_id) = url.strip_prefix("mav://extension/") {
                this.kind = Some(OpenRequestKind::Extension {
                    extension_id: extension_id.to_string(),
                });
            } else if url.starts_with(agent_skills::SKILL_SHARE_LINK_PREFIX) {
                this.parse_skill_install_url(&url)?
            } else if let Some(agent_path) = url.strip_prefix("mav://agent") {
                this.parse_agent_url(agent_path)
            } else if url == "mav://" || url == "mav://open" || url == "mav://open/" {
                this.kind = Some(OpenRequestKind::FocusApp);
            } else if let Some(schema_path) = url.strip_prefix("mav://schemas/") {
                this.kind = Some(OpenRequestKind::BuiltinJsonSchema {
                    schema_path: schema_path.to_string(),
                });
            } else if url == "mav://settings" || url == "mav://settings/" {
                this.kind = Some(OpenRequestKind::Setting { setting_path: None });
            } else if let Some(setting_path) = url.strip_prefix("mav://settings/") {
                this.kind = Some(OpenRequestKind::Setting {
                    setting_path: Some(setting_path.to_string()),
                });
            } else if let Some(clone_path) = url.strip_prefix("mav://git/clone") {
                this.parse_git_clone_url(clone_path)?
            } else if let Some(commit_path) = url.strip_prefix("mav://git/commit/") {
                this.parse_git_commit_url(commit_path)?
            } else if url.starts_with("ssh://") {
                this.parse_ssh_file_path(&url, cx)?
            } else if let Some(mav_link) = parse_mav_link(&url, cx) {
                match mav_link {
                    MavLink::Channel { channel_id } => {
                        this.join_channel = Some(channel_id);
                    }
                    MavLink::ChannelNotes {
                        channel_id,
                        heading,
                    } => {
                        this.open_channel_notes.push((channel_id, heading));
                    }
                }
            } else {
                log::error!("unhandled url: {}", url);
            }
        }

        Ok(this)
    }

    fn parse_file_path(&mut self, file: &str) {
        if let Some(decoded) = urlencoding::decode(file).log_err() {
            self.open_paths.push(decoded.into_owned())
        }
    }

    fn parse_agent_url(&mut self, agent_path: &str) {
        // Format: "" or "?prompt=<text>".
        let agent_path = agent_path.strip_prefix('/').unwrap_or(agent_path);
        let external_source_prompt = agent_path.strip_prefix('?').and_then(|query| {
            url::form_urlencoded::parse(query.as_bytes())
                .find_map(|(key, value)| (key == "prompt").then_some(value))
                .and_then(|prompt| ExternalSourcePrompt::new(prompt.as_ref()))
        });
        self.kind = Some(OpenRequestKind::AgentPanel {
            external_source_prompt,
        });
    }

    fn parse_skill_install_url(&mut self, url: &str) -> Result<()> {
        // Format: mav://skill?data=<base64url of SKILL.md contents>
        let content = agent_skills::decode_skill_share_link(url)?;
        self.kind = Some(OpenRequestKind::InstallSkill { content });
        Ok(())
    }

    fn parse_git_clone_url(&mut self, clone_path: &str) -> Result<()> {
        // Format: /?repo=<url> or ?repo=<url>
        let clone_path = clone_path.strip_prefix('/').unwrap_or(clone_path);

        let query = clone_path
            .strip_prefix('?')
            .context("invalid git clone url: missing query string")?;

        let repo_url = url::form_urlencoded::parse(query.as_bytes())
            .find_map(|(key, value)| (key == "repo").then_some(value))
            .filter(|s| !s.is_empty())
            .context("invalid git clone url: missing repo query parameter")?
            .to_string()
            .into();

        self.kind = Some(OpenRequestKind::GitClone { repo_url });

        Ok(())
    }

    fn parse_git_commit_url(&mut self, commit_path: &str) -> Result<()> {
        // Format: <sha>?repo=<path>
        let (sha, query) = commit_path
            .split_once('?')
            .context("invalid git commit url: missing query string")?;
        anyhow::ensure!(!sha.is_empty(), "invalid git commit url: missing sha");

        let repo = url::form_urlencoded::parse(query.as_bytes())
            .find_map(|(key, value)| (key == "repo").then_some(value))
            .filter(|s| !s.is_empty())
            .context("invalid git commit url: missing repo query parameter")?
            .to_string();

        self.open_paths.push(repo);

        self.kind = Some(OpenRequestKind::GitCommit {
            sha: sha.to_string(),
        });

        Ok(())
    }

    fn parse_ssh_file_path(&mut self, file: &str, cx: &App) -> Result<()> {
        let url = parse_ssh_url(file)?;
        let host = match url
            .host()
            .with_context(|| format!("missing host in ssh url: {url}"))?
        {
            url::Host::Domain(host) => host.to_string(),
            url::Host::Ipv4(host) => host.to_string(),
            url::Host::Ipv6(host) => host.to_string(),
        };
        let username = if url.username().is_empty() {
            None
        } else {
            Some(urlencoding::decode(url.username())?.into_owned())
        };
        let port = url.port();
        anyhow::ensure!(
            self.open_paths.is_empty(),
            "cannot open both local and ssh paths"
        );
        let mut connection_options =
            RemoteSettings::get_global(cx).connection_options_for(host, port, username);
        if let Some(password) = url.password() {
            connection_options.password = Some(urlencoding::decode(password)?.into_owned());
        }

        let connection_options = RemoteConnectionOptions::Ssh(connection_options);
        if let Some(ssh_connection) = &self.remote_connection {
            anyhow::ensure!(
                *ssh_connection == connection_options,
                "cannot open multiple different remote connections"
            );
        }
        self.remote_connection = Some(connection_options);
        self.parse_file_path(url.path());
        Ok(())
    }
}

fn parse_ssh_url(url: &str) -> Result<url::Url> {
    if let Ok(url) = url::Url::parse(url) {
        return Ok(url);
    }
    // SCP/git style urls use ':' to separate from Authority and Path.
    // They are unsupported by Url::parse, but can be normalized into a Url.
    //   SCPUrl("ssh://user@host:~/relpath") => Url("ssh://user@host/~/relpath")
    //   SCPUrl("ssh://user@host:/abs/path") => Url("ssh://user@host/abs/path")
    //
    // TODO: Add IPv6 support: "ssh://[2600::]:~/foo"
    let ssh_target = url
        .strip_prefix("ssh://")
        .with_context(|| format!("invalid ssh url: {url}"))?;

    let (authority, path) = if let Some((authority, path)) = ssh_target.rsplit_once(":~/") {
        (authority, format!("/~/{path}"))
    } else if let Some((authority, path)) = ssh_target.rsplit_once(":/") {
        (authority, format!("/{path}"))
    } else {
        anyhow::bail!("invalid ssh url: {url}");
    };

    let (userinfo, host) = authority
        .rsplit_once('@')
        .map_or((None, authority), |(userinfo, host)| (Some(userinfo), host));
    anyhow::ensure!(
        !host.is_empty() && !host.starts_with('[') && !host.contains(':'),
        "invalid ssh url: {url}"
    );

    let normalized_authority = if let Some(userinfo) = userinfo {
        let (username, colon_password) =
            if let Some((username, password)) = userinfo.split_once(':') {
                (
                    urlencoding::encode(&urlencoding::decode(username)?).into_owned(),
                    format!(
                        ":{}",
                        urlencoding::encode(&urlencoding::decode(password)?).into_owned()
                    ),
                )
            } else {
                (
                    urlencoding::encode(&urlencoding::decode(userinfo)?).into_owned(),
                    String::new(),
                )
            };
        format!("{username}{colon_password}@{host}")
    } else {
        authority.to_string()
    };

    Ok(url::Url::parse(&format!(
        "ssh://{normalized_authority}{path}"
    ))?)
}
