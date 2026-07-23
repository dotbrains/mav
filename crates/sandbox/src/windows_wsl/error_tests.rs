#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    fn select_distro_uses_wsl_distro_when_present() {
        let distro = select_distro(
            None,
            &[
                PathMapping::NativeDrive {
                    windows_path: "C:/project".to_string(),
                    fallback: WslPath {
                        distro: None,
                        path: "/mnt/c/project".to_string(),
                    },
                },
                PathMapping::Wsl(WslPath {
                    distro: Some("Ubuntu".to_string()),
                    path: "/home/me/project".to_string(),
                }),
            ],
        )
        .unwrap();
        assert_eq!(distro.as_deref(), Some("Ubuntu"));
    }

    #[test]
    fn bad_request_errors_do_not_claim_sandboxing_is_unavailable() {
        // Mixed distros and missing/unmappable paths are model-fixable bad
        // requests. They must not be typed as `WslSandboxUnavailable` (nor
        // carry its prefix), since the agent uses that type to offer the
        // run-unsandboxed fallback only for genuine environment failures.
        let mixed_distros = select_distro(
            Some(&PathMapping::Wsl(WslPath {
                distro: Some("Ubuntu".to_string()),
                path: "/home/me".to_string(),
            })),
            &[PathMapping::Wsl(WslPath {
                distro: Some("Debian".to_string()),
                path: "/home/me".to_string(),
            })],
        )
        .unwrap_err();
        assert!(
            mixed_distros
                .downcast_ref::<WslSandboxUnavailable>()
                .is_none()
        );
        assert!(!format!("{mixed_distros:#}").contains(WSL_SANDBOX_UNAVAILABLE_PREFIX));

        let missing_path =
            path_to_wsl(Path::new(r"C:\mav-test\definitely\does\not\exist-2769")).unwrap_err();
        assert!(
            missing_path
                .downcast_ref::<WslSandboxUnavailable>()
                .is_none()
        );
        assert!(!format!("{missing_path:#}").contains(WSL_SANDBOX_UNAVAILABLE_PREFIX));

        let unmappable_cwd = directory_to_wsl(Path::new(r"\\server\share\project")).unwrap_err();
        assert!(
            unmappable_cwd
                .downcast_ref::<WslSandboxUnavailable>()
                .is_none()
        );
        assert!(!format!("{unmappable_cwd:#}").contains(WSL_SANDBOX_UNAVAILABLE_PREFIX));
    }

    #[test]
    fn unavailable_errors_are_typed_and_prefixed() {
        // Environment failures are recognizable by type (so the agent doesn't
        // depend on message text) and still render with the shared prefix.
        let error = unavailable("Bubblewrap (`bwrap`) is not installed in the default WSL distro");
        let typed = error
            .downcast_ref::<WslSandboxUnavailable>()
            .expect("environment failure should downcast to WslSandboxUnavailable");
        assert_eq!(
            typed.message(),
            "Bubblewrap (`bwrap`) is not installed in the default WSL distro"
        );
        assert!(format!("{error:#}").starts_with(WSL_SANDBOX_UNAVAILABLE_PREFIX));
    }

    #[test]
    fn map_path_to_wsl_keeps_unc_paths_structural() {
        let mapping = map_path_to_wsl(Path::new(r"\\wsl.localhost\Ubuntu\home\me")).unwrap();
        assert_eq!(
            mapping,
            PathMapping::Wsl(WslPath {
                distro: Some("Ubuntu".to_string()),
                path: "/home/me".to_string(),
            })
        );
    }

    #[test]
    fn map_path_to_wsl_defers_native_paths_to_wslpath() {
        let mapping = map_path_to_wsl(Path::new(r"C:\Users\me\project")).unwrap();
        assert_eq!(
            mapping,
            PathMapping::NativeDrive {
                windows_path: "C:/Users/me/project".to_string(),
                fallback: WslPath {
                    distro: None,
                    path: "/mnt/c/Users/me/project".to_string(),
                },
            }
        );
    }

    #[test]
    fn map_path_to_wsl_strips_verbatim_prefix_for_wslpath() {
        let mapping = map_path_to_wsl(Path::new(r"\\?\D:\workspace")).unwrap();
        assert_eq!(
            mapping,
            PathMapping::NativeDrive {
                windows_path: "D:/workspace".to_string(),
                fallback: WslPath {
                    distro: None,
                    path: "/mnt/d/workspace".to_string(),
                },
            }
        );
    }
}
