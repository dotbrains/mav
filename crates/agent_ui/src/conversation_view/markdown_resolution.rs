use super::*;

pub(super) fn render_agent_markdown(
    markdown: Entity<Markdown>,
    style: MarkdownStyle,
    workspace: &WeakEntity<Workspace>,
    code_span_resolver: &AgentCodeSpanResolver,
    cx: &App,
) -> MarkdownElement {
    let workspace = workspace.clone();
    let worktree_roots = code_span_resolver.worktree_roots(cx);
    let resolver = code_span_resolver.clone();
    MarkdownElement::new(markdown, style)
        .code_block_renderer(markdown::CodeBlockRenderer::Default {
            copy_button_visibility: markdown::CopyButtonVisibility::VisibleOnHover,
            wrap_button_visibility: markdown::WrapButtonVisibility::VisibleOnHover,
            border: false,
        })
        .image_resolver(move |dest_url| resolve_agent_image(dest_url, &worktree_roots))
        .on_url_click(move |text, window, cx| {
            thread_view::open_link(text, &workspace, window, cx);
        })
        .on_code_span_link(move |text, cx| resolver.try_resolve(text, cx))
}

/// Shared, cloneable handle for resolving inline markdown code spans like
/// `` `src/main.rs:42` `` to clickable workspace file links.
#[derive(Clone)]
pub(crate) struct AgentCodeSpanResolver {
    inner: Arc<AgentCodeSpanResolverInner>,
}

/// Maximum number of memoized code-span resolutions kept in the cache.
const CODE_SPAN_CACHE_CAPACITY: NonZeroUsize = match NonZeroUsize::new(2048) {
    Some(n) => n,
    None => unreachable!(),
};

struct AgentCodeSpanResolverInner {
    project: WeakEntity<Project>,
    cache: Mutex<LruCache<Arc<str>, Option<SharedString>>>,
}

impl AgentCodeSpanResolver {
    pub(crate) fn new(project: &WeakEntity<Project>, _cx: &App) -> Self {
        Self {
            inner: Arc::new(AgentCodeSpanResolverInner {
                project: project.clone(),
                cache: Mutex::new(LruCache::new(CODE_SPAN_CACHE_CAPACITY)),
            }),
        }
    }

    pub(crate) fn clear_cache(&self) {
        self.inner.cache.lock().clear();
    }

    /// Absolute paths of every current worktree.
    /// Used by the markdown image resolver, which needs the same set of roots.
    fn worktree_roots(&self, cx: &App) -> Vec<PathBuf> {
        self.inner
            .project
            .upgrade()
            .map(|project| {
                project
                    .read(cx)
                    .visible_worktrees(cx)
                    .map(|worktree| worktree.read(cx).abs_path().to_path_buf())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn try_resolve(&self, text: &str, cx: &App) -> Option<SharedString> {
        let trimmed = sanitize_path_text(text.trim());
        if !Self::is_path_like(trimmed) {
            return None;
        }

        if let Some(cached) = self.inner.cache.lock().get(trimmed).cloned() {
            return cached;
        }

        let resolved = self.resolve_uncached(trimmed, cx);
        self.inner
            .cache
            .lock()
            .push(Arc::from(trimmed), resolved.clone());
        resolved
    }

    fn resolve_uncached(&self, trimmed: &str, cx: &App) -> Option<SharedString> {
        let path_with_position = PathWithPosition::parse_str(trimmed);
        let candidate_path = &path_with_position.path;
        if candidate_path.as_os_str().is_empty() {
            return None;
        }

        let project = self.inner.project.upgrade()?;
        let project = project.read(cx);
        for worktree in project.visible_worktrees(cx) {
            let worktree = worktree.read(cx);
            for relative_path in Self::candidate_relative_paths(
                candidate_path,
                &worktree.abs_path(),
                worktree.path_style(),
            ) {
                let project_path = ProjectPath {
                    worktree_id: worktree.id(),
                    path: relative_path.clone(),
                };
                let Some(entry) = project.entry_for_path(&project_path, cx) else {
                    continue;
                };
                if !entry.is_file() {
                    continue;
                }

                let abs_path = worktree.absolutize(&relative_path);
                let mention = match path_with_position.row.and_then(|row| row.checked_sub(1)) {
                    Some(line) => MentionUri::Selection {
                        abs_path: Some(abs_path),
                        line_range: line..=line,
                        column: path_with_position
                            .column
                            .map(|column| column.saturating_sub(1)),
                    },
                    None => MentionUri::File { abs_path },
                };

                return Some(mention.to_uri().to_string().into());
            }
        }

        None
    }

    fn candidate_relative_paths(
        path: &Path,
        worktree_abs_path: &Path,
        path_style: PathStyle,
    ) -> Vec<Arc<RelPath>> {
        let path_text = path.to_string_lossy();
        let relative_path: Option<Arc<RelPath>> =
            if util::paths::is_absolute(path_text.as_ref(), path_style) {
                path_style
                    .strip_prefix(path, worktree_abs_path)
                    .map(std::borrow::Cow::into_owned)
                    .map(Into::into)
            } else {
                RelPath::new(path, path_style)
                    .ok()
                    .map(std::borrow::Cow::into_owned)
                    .map(Into::into)
            };

        let Some(relative_path) = relative_path else {
            return Vec::new();
        };

        let mut paths = vec![relative_path.clone()];
        if let Some(root_name) = worktree_abs_path.file_name().and_then(|name| name.to_str())
            && let Ok(root_name) = RelPath::new(Path::new(root_name), path_style)
            && let Ok(stripped) = relative_path.strip_prefix(root_name.as_ref())
            && !stripped.is_empty()
        {
            paths.push(Arc::from(stripped));
        }
        paths
    }

    fn is_path_like(text: &str) -> bool {
        if text.len() < 3
            || text.contains("://")
            || text.contains('|')
            || text.chars().any(char::is_control)
            || text.chars().all(|character| character.is_ascii_digit())
        {
            return false;
        }

        let path = PathWithPosition::parse_str(text).path;
        let path_text = path.to_string_lossy();
        if path_text.contains('/') || path_text.contains('\\') {
            return true;
        }

        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| !extension.is_empty())
    }
}
