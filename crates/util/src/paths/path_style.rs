use crate::rel_path::RelPath;
use std::borrow::Cow;
use std::fmt::Display;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PathStyle {
    Posix,
    Windows,
}

impl PathStyle {
    #[cfg(target_os = "windows")]
    pub const fn local() -> Self {
        PathStyle::Windows
    }

    #[cfg(not(target_os = "windows"))]
    pub const fn local() -> Self {
        PathStyle::Posix
    }

    #[inline]
    pub fn primary_separator(&self) -> &'static str {
        match self {
            PathStyle::Posix => "/",
            PathStyle::Windows => "\\",
        }
    }

    pub fn separators(&self) -> &'static [&'static str] {
        match self {
            PathStyle::Posix => &["/"],
            PathStyle::Windows => &["\\", "/"],
        }
    }

    pub fn separators_ch(&self) -> &'static [char] {
        match self {
            PathStyle::Posix => &['/'],
            PathStyle::Windows => &['\\', '/'],
        }
    }

    pub fn is_absolute(&self, path_like: &str) -> bool {
        path_like.starts_with('/')
            || *self == PathStyle::Windows
                && (path_like.starts_with('\\')
                    || path_like
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_alphabetic())
                        && path_like[1..]
                            .strip_prefix(':')
                            .is_some_and(|path| path.starts_with('/') || path.starts_with('\\')))
    }

    pub fn is_windows(&self) -> bool {
        *self == PathStyle::Windows
    }

    pub fn is_posix(&self) -> bool {
        *self == PathStyle::Posix
    }

    pub fn join(self, left: impl AsRef<Path>, right: impl AsRef<Path>) -> Option<String> {
        let right = right.as_ref().to_str()?;
        if is_absolute(right, self) {
            return None;
        }
        let left = left.as_ref().to_str()?;
        if left.is_empty() {
            Some(right.into())
        } else {
            Some(format!(
                "{left}{}{right}",
                if left.ends_with(self.primary_separator()) {
                    ""
                } else {
                    self.primary_separator()
                }
            ))
        }
    }

    pub fn join_path(
        self,
        left: impl AsRef<Path>,
        right: impl AsRef<Path>,
    ) -> anyhow::Result<PathBuf> {
        let left = left
            .as_ref()
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Path contains invalid UTF-8"))?;
        let right = right.as_ref();
        let right_string = right
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Path contains invalid UTF-8"))?;
        let joined = self
            .join(left, right_string)
            .ok_or_else(|| anyhow::anyhow!("Path must be relative: {right:?}"))?;
        Ok(PathBuf::from(self.normalize(&joined)))
    }

    pub fn normalize(self, path_like: &str) -> String {
        match self {
            PathStyle::Windows => crate::normalize_path(Path::new(path_like))
                .to_string_lossy()
                .into_owned(),
            PathStyle::Posix => {
                let is_absolute = path_like.starts_with('/');
                let remainder = if is_absolute {
                    path_like.trim_start_matches('/')
                } else {
                    path_like
                };

                let mut components = Vec::new();
                for component in remainder.split(self.separators_ch()) {
                    match component {
                        "" | "." => {}
                        ".." => {
                            if components
                                .last()
                                .is_some_and(|component| *component != "..")
                            {
                                components.pop();
                            } else if !is_absolute {
                                components.push(component);
                            }
                        }
                        component => components.push(component),
                    }
                }

                let normalized = components.join(self.primary_separator());
                if is_absolute && normalized.is_empty() {
                    "/".to_string()
                } else if is_absolute {
                    format!("/{normalized}")
                } else {
                    normalized
                }
            }
        }
    }

    pub fn split(self, path_like: &str) -> (Option<&str>, &str) {
        let Some(pos) = path_like.rfind(self.primary_separator()) else {
            return (None, path_like);
        };
        let filename_start = pos + self.primary_separator().len();
        (
            Some(&path_like[..filename_start]),
            &path_like[filename_start..],
        )
    }

    pub fn strip_prefix<'a>(&self, child: &'a Path, parent: &'a Path) -> Option<Cow<'a, RelPath>> {
        let parent = parent.to_str()?;
        if parent.is_empty() {
            return RelPath::new(child, *self).ok();
        }
        let parent = self
            .separators()
            .iter()
            .find_map(|sep| parent.strip_suffix(sep))
            .unwrap_or(parent);
        let child = child.to_str()?;

        // Match behavior of std::path::Path, which is case-insensitive for drive letters (e.g., "C:" == "c:")
        let stripped = if self.is_windows()
            && child.as_bytes().get(1) == Some(&b':')
            && parent.as_bytes().get(1) == Some(&b':')
            && child.as_bytes()[0].eq_ignore_ascii_case(&parent.as_bytes()[0])
        {
            child[2..].strip_prefix(&parent[2..])?
        } else {
            child.strip_prefix(parent)?
        };
        if let Some(relative) = self
            .separators()
            .iter()
            .find_map(|sep| stripped.strip_prefix(sep))
        {
            RelPath::new(relative.as_ref(), *self).ok()
        } else if stripped.is_empty() {
            Some(Cow::Borrowed(RelPath::empty()))
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub struct RemotePathBuf {
    style: PathStyle,
    string: String,
}

impl RemotePathBuf {
    pub fn new(string: String, style: PathStyle) -> Self {
        Self { style, string }
    }

    pub fn from_str(path: &str, style: PathStyle) -> Self {
        Self::new(path.to_string(), style)
    }

    pub fn path_style(&self) -> PathStyle {
        self.style
    }

    pub fn to_proto(self) -> String {
        self.string
    }
}

impl Display for RemotePathBuf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.string)
    }
}

pub fn is_absolute(path_like: &str, path_style: PathStyle) -> bool {
    path_like.starts_with('/')
        || path_style == PathStyle::Windows
            && (path_like.starts_with('\\')
                || path_like
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic())
                    && path_like[1..]
                        .strip_prefix(':')
                        .is_some_and(|path| path.starts_with('/') || path.starts_with('\\')))
}
