use super::*;

#[derive(Default, Clone, JsonSchema, Debug, PartialEq, RegisterSetting)]
pub struct AllAgentServersSettings(pub HashMap<String, CustomAgentServerSettings>);

impl std::ops::Deref for AllAgentServersSettings {
    type Target = HashMap<String, CustomAgentServerSettings>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for AllAgentServersSettings {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl AllAgentServersSettings {
    pub fn has_registry_agents(&self) -> bool {
        self.values()
            .any(|s| matches!(s, CustomAgentServerSettings::Registry { .. }))
    }
}

#[derive(Clone, JsonSchema, Debug, PartialEq)]
pub enum CustomAgentServerSettings {
    Custom {
        command: AgentServerCommand,
        /// The default mode to use for this agent.
        ///
        /// Note: Not only all agents support modes.
        ///
        /// Default: None
        default_mode: Option<String>,
        /// Default values for session config options.
        ///
        /// This is a map from config option ID to the default value for that option.
        ///
        /// Default: {}
        default_config_options: HashMap<String, AgentConfigOptionValue>,
        /// Favorited values for session config options.
        ///
        /// This is a map from config option ID to a list of favorited value IDs.
        ///
        /// Default: {}
        favorite_config_option_values: HashMap<String, Vec<String>>,
    },
    Registry {
        /// Additional environment variables to pass to the agent.
        ///
        /// Default: {}
        env: HashMap<String, String>,
        /// The default mode to use for this agent.
        ///
        /// Note: Not only all agents support modes.
        ///
        /// Default: None
        default_mode: Option<String>,
        /// Default values for session config options.
        ///
        /// This is a map from config option ID to the default value for that option.
        ///
        /// Default: {}
        default_config_options: HashMap<String, AgentConfigOptionValue>,
        /// Favorited values for session config options.
        ///
        /// This is a map from config option ID to a list of favorited value IDs.
        ///
        /// Default: {}
        favorite_config_option_values: HashMap<String, Vec<String>>,
    },
}

impl CustomAgentServerSettings {
    pub fn command(&self) -> Option<&AgentServerCommand> {
        match self {
            CustomAgentServerSettings::Custom { command, .. } => Some(command),
            CustomAgentServerSettings::Registry { .. } => None,
        }
    }

    pub fn default_mode(&self) -> Option<&str> {
        match self {
            CustomAgentServerSettings::Custom { default_mode, .. }
            | CustomAgentServerSettings::Registry { default_mode, .. } => default_mode.as_deref(),
        }
    }

    pub fn default_config_option(&self, config_id: &str) -> Option<&AgentConfigOptionValue> {
        match self {
            CustomAgentServerSettings::Custom {
                default_config_options,
                ..
            }
            | CustomAgentServerSettings::Registry {
                default_config_options,
                ..
            } => default_config_options.get(config_id),
        }
    }

    pub fn favorite_config_option_values(&self, config_id: &str) -> Option<&[String]> {
        match self {
            CustomAgentServerSettings::Custom {
                favorite_config_option_values,
                ..
            }
            | CustomAgentServerSettings::Registry {
                favorite_config_option_values,
                ..
            } => favorite_config_option_values
                .get(config_id)
                .map(|v| v.as_slice()),
        }
    }
}

impl From<::settings::CustomAgentServerSettings> for CustomAgentServerSettings {
    fn from(value: ::settings::CustomAgentServerSettings) -> Self {
        match value {
            ::settings::CustomAgentServerSettings::Custom {
                path,
                args,
                env,
                default_mode,
                default_config_options,
                favorite_config_option_values,
            } => CustomAgentServerSettings::Custom {
                command: AgentServerCommand {
                    path: PathBuf::from(shellexpand::tilde(&path.to_string_lossy()).as_ref()),
                    args,
                    env: Some(env),
                },
                default_mode,
                default_config_options,
                favorite_config_option_values,
            },
            ::settings::CustomAgentServerSettings::Registry {
                env,
                default_mode,
                default_config_options,
                favorite_config_option_values,
            } => CustomAgentServerSettings::Registry {
                env,
                default_mode,
                default_config_options,
                favorite_config_option_values,
            },
        }
    }
}

impl ::settings::Settings for AllAgentServersSettings {
    fn from_settings(content: &::settings::SettingsContent) -> Self {
        let agent_settings = content.agent_servers.clone().unwrap();
        Self(
            agent_settings
                .0
                .into_iter()
                .map(|(k, v)| {
                    (
                        EXTENSION_TO_REGISTRY_IDS
                            .get(&k.as_str())
                            .map(|v| v.to_string())
                            .unwrap_or(k),
                        v.into(),
                    )
                })
                .collect(),
        )
    }
}
