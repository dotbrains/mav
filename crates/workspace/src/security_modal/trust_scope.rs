use std::path::{Path, PathBuf};

use ui::SharedString;

/// Validates a user-edited trust-scope path. Returns the absolute folder to
/// trust when `typed` is an ancestor of (or equal to) `project`; otherwise an
/// error message suitable for display. A leading `~` is expanded via `home_dir`.
pub(super) fn validate_trust_scope(
    typed: &str,
    project: &Path,
    home_dir: Option<&Path>,
) -> Result<PathBuf, SharedString> {
    let trimmed = typed.trim();
    if trimmed.is_empty() {
        return Err("Enter a folder to trust".into());
    }
    let expanded = match (trimmed.strip_prefix('~'), home_dir) {
        (Some(rest), Some(home_dir)) => {
            home_dir.join(rest.strip_prefix(std::path::MAIN_SEPARATOR).unwrap_or(rest))
        }
        _ => PathBuf::from(trimmed),
    };
    if !expanded.is_absolute() {
        return Err("Enter an absolute folder path".into());
    }
    if !project.starts_with(&expanded) {
        return Err("Must be a parent folder of the project".into());
    }
    Ok(expanded)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn accepts_ancestor_or_equal() {
        let project = Path::new("/Users/me/dev/delta/wt/t1");
        assert_eq!(
            validate_trust_scope("/Users/me/dev/delta/wt", project, None).unwrap(),
            PathBuf::from("/Users/me/dev/delta/wt"),
        );
        // Equal to the project itself is allowed.
        assert_eq!(
            validate_trust_scope("/Users/me/dev/delta/wt/t1", project, None).unwrap(),
            PathBuf::from("/Users/me/dev/delta/wt/t1"),
        );
        // A distant ancestor is allowed.
        assert!(validate_trust_scope("/Users/me/dev", project, None).is_ok());
    }

    #[test]
    fn rejects_non_ancestor_relative_or_empty() {
        let project = Path::new("/Users/me/dev/delta/wt/t1");
        assert!(validate_trust_scope("/Users/other", project, None).is_err());
        assert!(validate_trust_scope("relative/path", project, None).is_err());
        assert!(validate_trust_scope("   ", project, None).is_err());
        // Deeper than the project is not an ancestor.
        assert!(validate_trust_scope("/Users/me/dev/delta/wt/t1/sub", project, None).is_err());
    }

    #[test]
    fn expands_leading_tilde() {
        let home = Path::new("/Users/me");
        let project = Path::new("/Users/me/dev/wt/t1");
        assert_eq!(
            validate_trust_scope("~/dev/wt", project, Some(home)).unwrap(),
            PathBuf::from("/Users/me/dev/wt"),
        );
        assert_eq!(
            validate_trust_scope("~", project, Some(home)).unwrap(),
            PathBuf::from("/Users/me"),
        );
    }
}
