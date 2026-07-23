use super::*;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct WslPath {
    distro: Option<String>,
    path: String,
}

/// A path mapped for use inside WSL.
///
/// WSL UNC and WSL-absolute paths can be mapped structurally up front. Native
/// drive-letter paths depend on the distro's automount configuration
/// (`/etc/wsl.conf` can move the `/mnt` root), so they are translated with
/// `wslpath` inside the distro — but a distro can only be chosen after every
/// path has been parsed (WSL UNC paths pin one), hence this two-stage shape:
/// parse structurally first, then resolve native paths via [`resolve_paths`]
/// once the distro is known.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum PathMapping {
    Wsl(WslPath),
    NativeDrive {
        /// The `\\?\`-stripped, forward-slashed form that `wslpath -u`
        /// accepts (`wslpath` is a Linux binary and doesn't understand
        /// backslash separators).
        windows_path: String,
        /// The conventional `/mnt/<drive>/...` mapping, used when `wslpath`
        /// translation fails.
        fallback: WslPath,
    },
}

impl PathMapping {
    fn distro(&self) -> Option<&str> {
        match self {
            PathMapping::Wsl(path) => path.distro.as_deref(),
            PathMapping::NativeDrive { .. } => None,
        }
    }
}
