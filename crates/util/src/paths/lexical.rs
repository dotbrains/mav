use super::*;

pub fn path_ends_with(base: &Path, suffix: &Path) -> bool {
    strip_path_suffix(base, suffix).is_some()
}

/// Case-insensitive ASCII comparison of a path component to a literal
/// folder name. macOS and Windows use case-insensitive filesystems by
/// default, so a path like `.MAV/settings.json` resolves to the same
/// inode as the lowercase form. A case-sensitive `==` check would miss
/// those and let a malicious settings author bypass classifiers with
/// unusual casing. Callers should restrict `name` to ASCII; for ASCII
/// inputs `eq_ignore_ascii_case` is safe and stable across platforms.
pub fn component_matches_ignore_ascii_case(component: &OsStr, name: &str) -> bool {
    component
        .to_str()
        .is_some_and(|s| s.eq_ignore_ascii_case(name))
}

pub fn strip_path_suffix<'a>(base: &'a Path, suffix: &Path) -> Option<&'a Path> {
    if let Some(remainder) = base
        .as_os_str()
        .as_encoded_bytes()
        .strip_suffix(suffix.as_os_str().as_encoded_bytes())
    {
        if remainder
            .last()
            .is_none_or(|last_byte| std::path::is_separator(*last_byte as char))
        {
            let os_str = unsafe {
                OsStr::from_encoded_bytes_unchecked(
                    &remainder[0..remainder.len().saturating_sub(1)],
                )
            };
            return Some(Path::new(os_str));
        }
    }
    None
}

#[derive(Debug, PartialEq)]
#[non_exhaustive]
pub struct NormalizeError;

impl Error for NormalizeError {}

impl std::fmt::Display for NormalizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("parent reference `..` points outside of base directory")
    }
}

/// Copied from stdlib where it's unstable.
///
/// Normalize a path, including `..` without traversing the filesystem.
///
/// Returns an error if normalization would leave leading `..` components.
///
/// <div class="warning">
///
/// This function always resolves `..` to the "lexical" parent.
/// That is "a/b/../c" will always resolve to `a/c` which can change the meaning of the path.
/// In particular, `a/c` and `a/b/../c` are distinct on many systems because `b` may be a symbolic link, so its parent isn't `a`.
///
/// </div>
///
/// [`path::absolute`](absolute) is an alternative that preserves `..`.
/// Or [`Path::canonicalize`] can be used to resolve any `..` by querying the filesystem.
pub fn normalize_lexically(path: &Path) -> Result<PathBuf, NormalizeError> {
    use std::path::Component;

    let mut lexical = PathBuf::new();
    let mut iter = path.components().peekable();

    // Find the root, if any, and add it to the lexical path.
    // Here we treat the Windows path "C:\" as a single "root" even though
    // `components` splits it into two: (Prefix, RootDir).
    let root = match iter.peek() {
        Some(Component::ParentDir) => return Err(NormalizeError),
        Some(p @ Component::RootDir) | Some(p @ Component::CurDir) => {
            lexical.push(p);
            iter.next();
            lexical.as_os_str().len()
        }
        Some(Component::Prefix(prefix)) => {
            lexical.push(prefix.as_os_str());
            iter.next();
            if let Some(p @ Component::RootDir) = iter.peek() {
                lexical.push(p);
                iter.next();
            }
            lexical.as_os_str().len()
        }
        None => return Ok(PathBuf::new()),
        Some(Component::Normal(_)) => 0,
    };

    for component in iter {
        match component {
            Component::RootDir => unreachable!(),
            Component::Prefix(_) => return Err(NormalizeError),
            Component::CurDir => continue,
            Component::ParentDir => {
                // It's an error if ParentDir causes us to go above the "root".
                if lexical.as_os_str().len() == root {
                    return Err(NormalizeError);
                } else {
                    lexical.pop();
                }
            }
            Component::Normal(path) => lexical.push(path),
        }
    }
    Ok(lexical)
}

/// Insert `path` into a set of "subtree" grants, keeping the set minimal.
///
/// A subtree grant covers a path and all of its descendants. Insertion is a
/// no-op when `path` is already covered by an existing (equal-or-broader)
/// entry; otherwise `path` is added and any now-subsumed descendant entries
/// are pruned. Containment is purely lexical (component-wise `starts_with`),
/// so callers should normalize paths (e.g. via [`normalize_lexically`]) before
/// inserting, otherwise `..` components can defeat the containment checks.
pub fn insert_subtree(subtrees: &mut Vec<PathBuf>, path: PathBuf) {
    if subtrees.iter().any(|existing| path.starts_with(existing)) {
        return;
    }
    subtrees.retain(|existing| !existing.starts_with(&path));
    subtrees.push(path);
}

/// Whether `path` sits under (or exactly equals) any of the given subtree
/// grants. As with [`insert_subtree`], containment is purely lexical, so
/// callers should pass normalized paths.
pub fn path_within_subtree<'a>(path: &Path, mut subtrees: impl Iterator<Item = &'a Path>) -> bool {
    subtrees.any(|granted| path.starts_with(granted))
}
