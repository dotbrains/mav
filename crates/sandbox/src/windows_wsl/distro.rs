use super::*;

fn split_resolved_paths(
    has_cwd: bool,
    writable_path_count: usize,
    writable_git_path_count: usize,
    resolved: Vec<Option<String>>,
) -> Result<(Option<String>, Vec<String>, Vec<String>)> {
    let mut resolved = resolved.into_iter();
    let cwd = if has_cwd {
        Some(
            resolved
                .next()
                .context("bug: missing resolved terminal cwd")?
                .context("bug: required terminal cwd resolved as missing")?,
        )
    } else {
        None
    };

    let mut writable_paths = Vec::with_capacity(writable_path_count + writable_git_path_count);
    for _ in 0..writable_path_count {
        writable_paths.push(
            resolved
                .next()
                .context("bug: missing resolved writable path")?
                .context("bug: required writable path resolved as missing")?,
        );
    }
    for _ in 0..writable_git_path_count {
        if let Some(path) = resolved
            .next()
            .context("bug: missing resolved writable Git metadata path")?
        {
            writable_paths.push(path);
        }
    }

    Ok((cwd, writable_paths, resolved.flatten().collect()))
}

fn select_distro<'a>(
    cwd: Option<&PathMapping>,
    paths: impl IntoIterator<Item = &'a PathMapping>,
) -> Result<Option<String>> {
    let mut distro = cwd.and_then(|mapping| mapping.distro().map(str::to_string));
    for mapping in paths {
        let Some(path_distro) = mapping.distro() else {
            continue;
        };
        match distro.as_deref() {
            // A bad request, not an environment problem: the model (or
            // project layout) asked for paths spanning two distros, which a
            // single bwrap invocation can't serve.
            Some(distro) => ensure!(
                distro == path_distro,
                "cannot sandbox a command whose paths mix WSL distros `{}` and `{}`",
                distro,
                path_distro
            ),
            None => distro = Some(path_distro.to_string()),
        }
    }
    Ok(distro)
}
