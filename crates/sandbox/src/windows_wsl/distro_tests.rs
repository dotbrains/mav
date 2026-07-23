#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    fn split_resolved_paths_keeps_existing_writable_git_paths_and_skips_missing_ones() {
        let (cwd, writable_paths, protected_git_paths) = split_resolved_paths(
            true,
            1,
            2,
            vec![
                Some("/home/me/project".to_string()),
                Some("/home/me/project".to_string()),
                None,
                Some("/home/me/project/.git".to_string()),
                None,
                Some("/mnt/c/external/.git".to_string()),
            ],
        )
        .unwrap();

        assert_eq!(cwd.as_deref(), Some("/home/me/project"));
        assert_eq!(
            writable_paths,
            vec![
                "/home/me/project".to_string(),
                "/home/me/project/.git".to_string()
            ]
        );
        assert_eq!(
            protected_git_paths,
            vec!["/mnt/c/external/.git".to_string()]
        );
    }

    #[test]
    fn split_resolved_paths_rejects_missing_required_writable_paths() {
        let error = split_resolved_paths(false, 1, 0, vec![None]).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("required writable path resolved as missing"),
            "unexpected error: {error:#}"
        );
    }
}
