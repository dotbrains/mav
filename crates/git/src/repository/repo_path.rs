use super::*;

#[derive(Clone, Ord, Hash, PartialOrd, Eq, PartialEq)]
pub struct RepoPath(pub(super) Arc<RelPath>);

impl std::fmt::Debug for RepoPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl RepoPath {
    pub fn new<S: AsRef<str> + ?Sized>(s: &S) -> Result<Self> {
        let rel_path = RelPath::unix(s.as_ref())?;
        Ok(Self::from_rel_path(rel_path))
    }

    pub fn from_std_path(path: &Path, path_style: PathStyle) -> Result<Self> {
        let rel_path = RelPath::new(path, path_style)?;
        Ok(Self::from_rel_path(&rel_path))
    }

    pub fn from_proto(proto: &str) -> Result<Self> {
        let rel_path = RelPath::from_proto(proto)?;
        Ok(Self(rel_path))
    }

    pub fn from_rel_path(path: &RelPath) -> RepoPath {
        Self(Arc::from(path))
    }

    pub fn as_std_path(&self) -> &Path {
        if self.is_empty() {
            Path::new(".")
        } else {
            self.0.as_std_path()
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
pub fn repo_path<S: AsRef<str> + ?Sized>(s: &S) -> RepoPath {
    RepoPath(RelPath::unix(s.as_ref()).unwrap().into())
}

impl AsRef<Arc<RelPath>> for RepoPath {
    fn as_ref(&self) -> &Arc<RelPath> {
        &self.0
    }
}

impl std::ops::Deref for RepoPath {
    type Target = RelPath;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug)]
pub struct RepoPathDescendants<'a>(pub &'a RepoPath);

impl MapSeekTarget<RepoPath> for RepoPathDescendants<'_> {
    fn cmp_cursor(&self, key: &RepoPath) -> Ordering {
        if key.starts_with(self.0) {
            Ordering::Greater
        } else {
            self.0.cmp(key)
        }
    }
}
