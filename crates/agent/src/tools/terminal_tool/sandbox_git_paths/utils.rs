use fs::Fs;
use std::path::{Path, PathBuf};

pub(super) fn parse_core_worktree(config: &str) -> Option<String> {
    let mut in_core_section = false;
    let mut core_worktree = None;

    for raw_line in config.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.ends_with('\\') {
            return None;
        }

        if line.starts_with('[') {
            if !line.ends_with(']') {
                return None;
            }
            let section = line[1..line.len() - 1].trim();
            if section.to_lowercase().starts_with("include") {
                return None;
            }
            in_core_section = section.eq_ignore_ascii_case("core");
            continue;
        }

        if !in_core_section {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if !key.trim().eq_ignore_ascii_case("worktree") {
            continue;
        }
        if core_worktree.is_some() {
            return None;
        }
        core_worktree = Some(parse_git_config_path_value(value.trim())?);
    }

    core_worktree
}

pub(super) fn parse_git_config_path_value(value: &str) -> Option<String> {
    if value.is_empty() {
        return None;
    }

    if !value.starts_with('"') {
        if value.contains('"') || value.starts_with('~') {
            return None;
        }
        return Some(value.to_string());
    }

    let mut chars = value.chars();
    chars.next()?;
    let mut parsed = String::new();
    let mut escaped = false;
    let mut closed = false;
    while let Some(character) = chars.next() {
        if escaped {
            match character {
                '"' | '\\' => parsed.push(character),
                _ => return None,
            }
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '"' {
            closed = true;
            break;
        } else {
            parsed.push(character);
        }
    }

    if escaped || !closed {
        return None;
    }

    let remaining = &value[value.len() - chars.as_str().len()..];
    if !remaining.trim().is_empty() {
        return None;
    }

    if parsed.is_empty() || parsed.starts_with('~') {
        return None;
    }

    Some(parsed)
}

pub(super) fn path_is_within_any(path: &Path, roots: &[PathBuf]) -> bool {
    // `Path::starts_with` matches whole components, so `/projectX` is not
    // treated as being within `/project`.
    roots.iter().any(|root| path.starts_with(root))
}

pub(super) async fn normalize_sandbox_git_path(
    path: impl AsRef<Path>,
    fs: &dyn Fs,
) -> Option<PathBuf> {
    if let Ok(path) = fs.canonicalize(path.as_ref()).await {
        Some(path)
    } else {
        util::paths::normalize_lexically(path.as_ref()).ok()
    }
}
