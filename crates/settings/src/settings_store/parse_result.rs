use super::*;

/// The result of parsing settings, including any migration attempts
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsParseResult {
    /// The result of parsing the settings file (possibly after migration)
    pub parse_status: ParseStatus,
    /// The result of attempting to migrate the settings file
    pub migration_status: MigrationStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationStatus {
    /// No migration was needed - settings are up to date
    NotNeeded,
    /// Settings were automatically migrated in memory, but the file needs to be updated
    Succeeded,
    /// Migration was attempted but failed. Original settings were parsed instead.
    Failed { error: String },
}

impl Default for SettingsParseResult {
    fn default() -> Self {
        Self {
            parse_status: ParseStatus::Success,
            migration_status: MigrationStatus::NotNeeded,
        }
    }
}

impl SettingsParseResult {
    pub fn unwrap(self) -> bool {
        self.result().unwrap()
    }

    pub fn expect(self, message: &str) -> bool {
        self.result().expect(message)
    }

    /// Formats the ParseResult as a Result type. This is a lossy conversion
    pub fn result(self) -> Result<bool> {
        let migration_result = match self.migration_status {
            MigrationStatus::NotNeeded => Ok(false),
            MigrationStatus::Succeeded => Ok(true),
            MigrationStatus::Failed { error } => {
                Err(anyhow::format_err!(error)).context("Failed to migrate settings")
            }
        };

        let parse_result = match self.parse_status {
            ParseStatus::Success | ParseStatus::Unchanged => Ok(()),
            ParseStatus::Failed { error } => {
                Err(anyhow::format_err!(error)).context("Failed to parse settings")
            }
        };

        match (migration_result, parse_result) {
            (migration_result @ Ok(_), Ok(())) => migration_result,
            (Err(migration_err), Ok(())) => Err(migration_err),
            (_, Err(parse_err)) => Err(parse_err),
        }
    }

    /// Returns true if there were any errors migrating and parsing the settings content or if migration was required but there were no errors
    pub fn requires_user_action(&self) -> bool {
        matches!(self.parse_status, ParseStatus::Failed { .. })
            || matches!(
                self.migration_status,
                MigrationStatus::Succeeded | MigrationStatus::Failed { .. }
            )
    }

    pub fn ok(self) -> Option<bool> {
        self.result().ok()
    }

    pub fn parse_error(&self) -> Option<String> {
        match &self.parse_status {
            ParseStatus::Failed { error } => Some(error.clone()),
            ParseStatus::Success | ParseStatus::Unchanged => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum InvalidSettingsError {
    LocalSettings {
        path: Arc<RelPath>,
        message: String,
    },
    UserSettings {
        message: String,
    },
    ServerSettings {
        message: String,
    },
    DefaultSettings {
        message: String,
    },
    Editorconfig {
        path: LocalSettingsPath,
        message: String,
    },
    Tasks {
        path: PathBuf,
        message: String,
    },
    Debug {
        path: PathBuf,
        message: String,
    },
}

impl std::fmt::Display for InvalidSettingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InvalidSettingsError::LocalSettings { message, .. }
            | InvalidSettingsError::UserSettings { message }
            | InvalidSettingsError::ServerSettings { message }
            | InvalidSettingsError::DefaultSettings { message }
            | InvalidSettingsError::Tasks { message, .. }
            | InvalidSettingsError::Editorconfig { message, .. }
            | InvalidSettingsError::Debug { message, .. } => write!(f, "{message}"),
        }
    }
}
impl std::error::Error for InvalidSettingsError {}
