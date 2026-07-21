use super::*;

#[derive(Debug, Clone)]
pub struct DirectoryItem {
    pub path: PathBuf,
    pub is_dir: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DocumentColor {
    pub lsp_range: lsp::Range,
    pub color: lsp::Color,
    pub resolved: bool,
    pub color_presentations: Vec<ColorPresentation>,
}

impl Eq for DocumentColor {}

impl std::hash::Hash for DocumentColor {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.lsp_range.hash(state);
        self.color.red.to_bits().hash(state);
        self.color.green.to_bits().hash(state);
        self.color.blue.to_bits().hash(state);
        self.color.alpha.to_bits().hash(state);
        self.resolved.hash(state);
        self.color_presentations.hash(state);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColorPresentation {
    pub label: SharedString,
    pub text_edit: Option<lsp::TextEdit>,
    pub additional_text_edits: Vec<lsp::TextEdit>,
}

impl std::hash::Hash for ColorPresentation {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.label.hash(state);
        if let Some(ref edit) = self.text_edit {
            edit.range.hash(state);
            edit.new_text.hash(state);
        }
        self.additional_text_edits.len().hash(state);
        for edit in &self.additional_text_edits {
            edit.range.hash(state);
            edit.new_text.hash(state);
        }
    }
}

#[derive(Clone)]
pub enum DirectoryLister {
    Project(Entity<Project>),
    Local(Entity<Project>, Arc<dyn Fs>),
}

impl std::fmt::Debug for DirectoryLister {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DirectoryLister::Project(project) => {
                write!(f, "DirectoryLister::Project({project:?})")
            }
            DirectoryLister::Local(project, _) => {
                write!(f, "DirectoryLister::Local({project:?})")
            }
        }
    }
}

impl DirectoryLister {
    pub fn is_local(&self, cx: &App) -> bool {
        match self {
            DirectoryLister::Local(..) => true,
            DirectoryLister::Project(project) => project.read(cx).is_local(),
        }
    }

    pub fn resolve_tilde<'a>(&self, path: &'a String, cx: &App) -> Cow<'a, str> {
        if self.is_local(cx) {
            shellexpand::tilde(path)
        } else {
            Cow::from(path)
        }
    }

    pub fn default_query(&self, cx: &mut App) -> String {
        let project = match self {
            DirectoryLister::Project(project) => project,
            DirectoryLister::Local(project, _) => project,
        }
        .read(cx);
        let path_style = project.path_style(cx);
        project
            .visible_worktrees(cx)
            .next()
            .map(|worktree| worktree.read(cx).abs_path().to_string_lossy().into_owned())
            .or_else(|| std::env::home_dir().map(|dir| dir.to_string_lossy().into_owned()))
            .map(|mut s| {
                s.push_str(path_style.primary_separator());
                s
            })
            .unwrap_or_else(|| {
                if path_style.is_windows() {
                    "C:\\"
                } else {
                    "~/"
                }
                .to_string()
            })
    }

    pub fn list_directory(&self, path: String, cx: &mut App) -> Task<Result<Vec<DirectoryItem>>> {
        match self {
            DirectoryLister::Project(project) => {
                project.update(cx, |project, cx| project.list_directory(path, cx))
            }
            DirectoryLister::Local(_, fs) => {
                let fs = fs.clone();
                cx.background_spawn(async move {
                    let mut results = vec![];
                    let expanded = shellexpand::tilde(&path);
                    let query = Path::new(expanded.as_ref());
                    let mut response = fs.read_dir(query).await?;
                    while let Some(path) = response.next().await {
                        let path = path?;
                        if let Some(file_name) = path.file_name() {
                            results.push(DirectoryItem {
                                path: PathBuf::from(file_name.to_os_string()),
                                is_dir: fs.is_dir(&path).await,
                            });
                        }
                    }
                    Ok(results)
                })
            }
        }
    }

    pub fn path_style(&self, cx: &App) -> PathStyle {
        match self {
            Self::Local(project, ..) | Self::Project(project, ..) => {
                project.read(cx).path_style(cx)
            }
        }
    }
}
