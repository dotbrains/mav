#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    fn path_resolution_args_flattens_mappings_into_triples() {
        let mappings = [
            PathMapping::NativeDrive {
                windows_path: "C:/Users/me/project".to_string(),
                fallback: WslPath {
                    distro: None,
                    path: "/mnt/c/Users/me/project".to_string(),
                },
            },
            PathMapping::Wsl(WslPath {
                distro: Some("Ubuntu".to_string()),
                path: "/home/me/project".to_string(),
            }),
        ];
        assert_eq!(
            path_resolution_args(mappings.iter()),
            [
                "W",
                "C:/Users/me/project",
                "/mnt/c/Users/me/project",
                "L",
                "/home/me/project",
                "",
            ]
        );
    }

    #[test]
    fn parse_path_resolution_output_reads_one_line_per_path() {
        let resolved = parse_path_resolution_output(
            "ok ok /mnt/c/Users/me/project\nfallback missing /mnt/d/workspace\n",
            2,
        )
        .unwrap();
        assert_eq!(
            resolved,
            [
                ResolvedPath {
                    path: "/mnt/c/Users/me/project".to_string(),
                    used_fallback: false,
                    exists: true,
                },
                ResolvedPath {
                    path: "/mnt/d/workspace".to_string(),
                    used_fallback: true,
                    exists: false,
                },
            ]
        );
    }

    #[test]
    fn parse_path_resolution_output_keeps_spaces_in_paths() {
        let resolved =
            parse_path_resolution_output("ok ok /mnt/c/Users/me/My Documents/project\n", 1)
                .unwrap();
        assert_eq!(resolved[0].path, "/mnt/c/Users/me/My Documents/project");
    }

    #[test]
    fn parse_path_resolution_output_rejects_wrong_line_count() {
        assert!(parse_path_resolution_output("ok ok /a\n", 2).is_err());
        assert!(parse_path_resolution_output("ok ok /a\nok ok /b\n", 1).is_err());
    }

    #[test]
    fn parse_path_resolution_output_rejects_corrupted_lines() {
        assert!(parse_path_resolution_output("garbage\n", 1).is_err());
        assert!(parse_path_resolution_output("weird ok /a\n", 1).is_err());
        assert!(parse_path_resolution_output("ok weird /a\n", 1).is_err());
        assert!(parse_path_resolution_output("ok ok not-absolute\n", 1).is_err());
    }
}
