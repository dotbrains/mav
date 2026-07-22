use super::*;

#[derive(PartialEq, Eq, Clone, Copy)]
pub(crate) struct FileMask(u8);

impl std::fmt::Debug for FileMask {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "FileMask(")?;
        let mut items = vec![];

        if self.contains(USER) {
            items.push("USER");
        }
        if self.contains(PROJECT) {
            items.push("LOCAL");
        }
        if self.contains(SERVER) {
            items.push("SERVER");
        }

        write!(f, "{})", items.join(" | "))
    }
}

pub(crate) const USER: FileMask = FileMask(1 << 0);
pub(crate) const PROJECT: FileMask = FileMask(1 << 2);
pub(crate) const SERVER: FileMask = FileMask(1 << 3);

impl std::ops::BitAnd for FileMask {
    type Output = Self;

    fn bitand(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }
}

impl std::ops::BitOr for FileMask {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl FileMask {
    pub(crate) fn contains(&self, other: FileMask) -> bool {
        self.0 & other.0 != 0
    }
}

#[allow(unused)]
#[derive(Clone, PartialEq, Debug)]
pub(crate) enum SettingsUiFile {
    User,                                // Uses all settings.
    Project((WorktreeId, Arc<RelPath>)), // Has a special name, and special set of settings
    Server(&'static str),                // Uses a special name, and the user settings
}

impl SettingsUiFile {
    pub(crate) fn setting_type(&self) -> &'static str {
        match self {
            SettingsUiFile::User => "User",
            SettingsUiFile::Project(_) => "Project",
            SettingsUiFile::Server(_) => "Server",
        }
    }

    pub(crate) fn is_server(&self) -> bool {
        matches!(self, SettingsUiFile::Server(_))
    }

    pub(crate) fn worktree_id(&self) -> Option<WorktreeId> {
        match self {
            SettingsUiFile::User => None,
            SettingsUiFile::Project((worktree_id, _)) => Some(*worktree_id),
            SettingsUiFile::Server(_) => None,
        }
    }

    pub(crate) fn from_settings(file: settings::SettingsFile) -> Option<Self> {
        Some(match file {
            settings::SettingsFile::User => SettingsUiFile::User,
            settings::SettingsFile::Project(location) => SettingsUiFile::Project(location),
            settings::SettingsFile::Server => SettingsUiFile::Server("todo: server name"),
            settings::SettingsFile::Default => return None,
            settings::SettingsFile::Global => return None,
        })
    }

    pub(crate) fn to_settings(&self) -> settings::SettingsFile {
        match self {
            SettingsUiFile::User => settings::SettingsFile::User,
            SettingsUiFile::Project(location) => settings::SettingsFile::Project(location.clone()),
            SettingsUiFile::Server(_) => settings::SettingsFile::Server,
        }
    }

    pub(crate) fn mask(&self) -> FileMask {
        match self {
            SettingsUiFile::User => USER,
            SettingsUiFile::Project(_) => PROJECT,
            SettingsUiFile::Server(_) => SERVER,
        }
    }
}
