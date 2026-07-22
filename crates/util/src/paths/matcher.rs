use super::*;

#[derive(Clone)]
pub struct PathMatcher {
    sources: Vec<(String, RelPathBuf, /*trailing separator*/ bool)>,
    glob: GlobSet,
    path_style: PathStyle,
}

impl std::fmt::Debug for PathMatcher {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PathMatcher")
            .field("sources", &self.sources)
            .field("path_style", &self.path_style)
            .finish()
    }
}

impl PartialEq for PathMatcher {
    fn eq(&self, other: &Self) -> bool {
        self.sources.eq(&other.sources)
    }
}

impl Eq for PathMatcher {}

impl PathMatcher {
    pub fn new(
        globs: impl IntoIterator<Item = impl AsRef<str>>,
        path_style: PathStyle,
    ) -> Result<Self, globset::Error> {
        let globs = globs
            .into_iter()
            .map(|as_str| {
                GlobBuilder::new(as_str.as_ref())
                    .backslash_escape(path_style.is_posix())
                    .build()
            })
            .collect::<Result<Vec<_>, _>>()?;
        let sources = globs
            .iter()
            .filter_map(|glob| {
                let glob = glob.glob();
                Some((
                    glob.to_string(),
                    RelPath::new(&glob.as_ref(), path_style)
                        .ok()
                        .map(std::borrow::Cow::into_owned)?,
                    glob.ends_with(path_style.separators_ch()),
                ))
            })
            .collect();
        let mut glob_builder = GlobSetBuilder::new();
        for single_glob in globs {
            glob_builder.add(single_glob);
        }
        let glob = glob_builder.build()?;
        Ok(PathMatcher {
            glob,
            sources,
            path_style,
        })
    }

    pub fn sources(&self) -> impl Iterator<Item = &str> + Clone {
        self.sources.iter().map(|(source, ..)| source.as_str())
    }

    pub fn is_match<P: AsRef<RelPath>>(&self, other: P) -> bool {
        let other = other.as_ref();
        if self
            .sources
            .iter()
            .any(|(_, source, _)| other.starts_with(source) || other.ends_with(source))
        {
            return true;
        }
        let other_path = other.display(self.path_style);

        if self.glob.is_match(&*other_path) {
            return true;
        }

        self.glob
            .is_match(other_path.into_owned() + self.path_style.primary_separator())
    }

    pub fn is_match_std_path<P: AsRef<Path>>(&self, other: P) -> bool {
        let other = other.as_ref();
        if self.sources.iter().any(|(_, source, _)| {
            other.starts_with(source.as_std_path()) || other.ends_with(source.as_std_path())
        }) {
            return true;
        }
        self.glob.is_match(other)
    }
}

impl Default for PathMatcher {
    fn default() -> Self {
        Self {
            path_style: PathStyle::local(),
            glob: GlobSet::empty(),
            sources: vec![],
        }
    }
}
