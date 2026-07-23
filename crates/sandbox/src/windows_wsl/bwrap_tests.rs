#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    fn bwrap_denies_network_by_default() {
        let args = build_bwrap_args(
            &["/home/me/project".to_string()],
            &[],
            SandboxPermissions::default(),
            Some("/home/me/project"),
            true,
            &HashMap::new(),
        );
        assert!(args.iter().any(|arg| arg == "--unshare-net"));
        assert!(
            args.windows(3)
                .any(|window| window == ["--bind", "/home/me/project", "/home/me/project"])
        );
    }

    #[test]
    fn bwrap_allows_network_when_requested() {
        let args = build_bwrap_args(
            &[],
            &[],
            SandboxPermissions {
                allow_network: true,
                allow_fs_write: false,
            },
            None,
            true,
            &HashMap::new(),
        );
        assert!(!args.iter().any(|arg| arg == "--unshare-net"));
    }

    #[test]
    fn bwrap_binds_explicit_writable_file_paths() {
        let args = build_bwrap_args(
            &["/mnt/c/Users/me/AppData/Roaming/Mav/AGENTS.md".to_string()],
            &[],
            SandboxPermissions::default(),
            None,
            true,
            &HashMap::new(),
        );
        assert!(args.windows(3).any(|window| window
            == [
                "--bind",
                "/mnt/c/Users/me/AppData/Roaming/Mav/AGENTS.md",
                "/mnt/c/Users/me/AppData/Roaming/Mav/AGENTS.md"
            ]));
    }

    #[test]
    fn bwrap_protects_git_paths_after_writable_paths() {
        let args = build_bwrap_args(
            &["/home/me/project".to_string()],
            &["/home/me/project/.git".to_string()],
            SandboxPermissions::default(),
            Some("/home/me/project"),
            true,
            &HashMap::new(),
        );
        let writable_index = args
            .windows(3)
            .position(|window| window == ["--bind", "/home/me/project", "/home/me/project"])
            .expect("project should be writable");
        let protected_index = args
            .windows(3)
            .position(|window| {
                window
                    == [
                        "--ro-bind",
                        "/home/me/project/.git",
                        "/home/me/project/.git",
                    ]
            })
            .expect("Git metadata should be protected read-only");
        assert!(protected_index > writable_index);
    }

    #[test]
    fn bwrap_skips_git_protection_when_fs_writes_are_unrestricted() {
        let args = build_bwrap_args(
            &[],
            &["/home/me/project/.git".to_string()],
            SandboxPermissions {
                allow_network: false,
                allow_fs_write: true,
            },
            None,
            true,
            &HashMap::new(),
        );
        assert!(!args.windows(3).any(|window| window
            == [
                "--ro-bind",
                "/home/me/project/.git",
                "/home/me/project/.git"
            ]));
    }

    #[test]
    fn bwrap_blocks_wsl_interop_by_default() {
        let args = build_bwrap_args(
            &["/home/me/project".to_string()],
            &[],
            SandboxPermissions::default(),
            Some("/home/me/project"),
            true,
            &HashMap::new(),
        );
        assert!(
            args.windows(2)
                .any(|window| window == ["--unsetenv", "WSL_INTEROP"])
        );
        assert!(
            args.windows(2)
                .any(|window| window == ["--tmpfs", "/run/WSL"])
        );
    }

    #[test]
    fn bwrap_blocks_wsl_interop_even_with_fs_write() {
        let args = build_bwrap_args(
            &[],
            &[],
            SandboxPermissions {
                allow_network: true,
                allow_fs_write: true,
            },
            None,
            true,
            &HashMap::new(),
        );
        // Interop is host code execution, not just a filesystem write, so it
        // stays blocked even when the user has granted unrestricted writes
        // and network.
        assert!(
            args.windows(2)
                .any(|window| window == ["--unsetenv", "WSL_INTEROP"])
        );
        assert!(
            args.windows(2)
                .any(|window| window == ["--tmpfs", "/run/WSL"])
        );
    }

    #[test]
    fn bwrap_skips_interop_dir_mask_when_absent() {
        // When the interop socket directory doesn't exist (interop disabled),
        // there's nothing to mask and a `--tmpfs /run/WSL` would abort bwrap,
        // so the mount must be omitted. Unsetting the variable is harmless and
        // stays.
        let args = build_bwrap_args(
            &[],
            &[],
            SandboxPermissions::default(),
            None,
            false,
            &HashMap::new(),
        );
        assert!(
            args.windows(2)
                .any(|window| window == ["--unsetenv", "WSL_INTEROP"])
        );
        assert!(!args.iter().any(|arg| arg == "/run/WSL"));
    }

    #[test]
    fn bwrap_forwards_env_via_setenv() {
        let env = HashMap::from([
            ("PAGER".to_string(), String::new()),
            ("CARGO_TERM_COLOR".to_string(), "always".to_string()),
        ]);
        let args = build_bwrap_args(&[], &[], SandboxPermissions::default(), None, false, &env);
        assert!(
            args.windows(3)
                .any(|window| window == ["--setenv", "PAGER", ""])
        );
        assert!(
            args.windows(3)
                .any(|window| window == ["--setenv", "CARGO_TERM_COLOR", "always"])
        );
    }

    #[test]
    fn bwrap_does_not_forward_wsl_interop_env() {
        let env = HashMap::from([
            (
                "WSL_INTEROP".to_string(),
                "/run/WSL/123_interop".to_string(),
            ),
            ("WsLeNv".to_string(), "WSL_INTEROP/u".to_string()),
            ("PAGER".to_string(), String::new()),
        ]);
        let args = build_bwrap_args(&[], &[], SandboxPermissions::default(), None, false, &env);

        assert!(
            args.windows(2)
                .any(|window| window == ["--unsetenv", "WSL_INTEROP"])
        );
        assert!(
            args.windows(2)
                .any(|window| window == ["--unsetenv", "WSLENV"])
        );
        assert!(
            args.windows(3)
                .any(|window| window == ["--setenv", "PAGER", ""])
        );
        assert!(!args.windows(3).any(|window| {
            matches!(window, [flag, name, _]
                if flag.as_str() == "--setenv"
                    && name.eq_ignore_ascii_case("WSL_INTEROP"))
        }));
        assert!(!args.windows(3).any(|window| {
            matches!(window, [flag, name, _]
                if flag.as_str() == "--setenv"
                    && name.eq_ignore_ascii_case("WSLENV"))
        }));
    }

    #[test]
    fn bwrap_does_not_forward_windows_specific_env() {
        // These hold Windows paths/values that would break or be meaningless
        // inside WSL, so they must never cross the boundary. Names are matched
        // case-insensitively, as Windows env var names are. `(x86)` variants
        // contain parentheses but no `=`, so they'd pass the `setenv` filter
        // and must be blocked by name.
        let env = HashMap::from([
            ("Path".to_string(), r"C:\Windows\System32".to_string()),
            ("PATHEXT".to_string(), ".COM;.EXE;.BAT".to_string()),
            (
                "TEMP".to_string(),
                r"C:\Users\me\AppData\Local\Temp".to_string(),
            ),
            (
                "Tmp".to_string(),
                r"C:\Users\me\AppData\Local\Temp".to_string(),
            ),
            ("TMPDIR".to_string(), r"C:\tmp".to_string()),
            ("OS".to_string(), "Windows_NT".to_string()),
            (
                "ComSpec".to_string(),
                r"C:\Windows\system32\cmd.exe".to_string(),
            ),
            ("windir".to_string(), r"C:\Windows".to_string()),
            ("SystemRoot".to_string(), r"C:\Windows".to_string()),
            ("HOME".to_string(), r"C:\Users\me".to_string()),
            ("USERPROFILE".to_string(), r"C:\Users\me".to_string()),
            (
                "APPDATA".to_string(),
                r"C:\Users\me\AppData\Roaming".to_string(),
            ),
            (
                "LOCALAPPDATA".to_string(),
                r"C:\Users\me\AppData\Local".to_string(),
            ),
            (
                "ProgramFiles(x86)".to_string(),
                r"C:\Program Files (x86)".to_string(),
            ),
            ("USERNAME".to_string(), "me".to_string()),
            ("COMPUTERNAME".to_string(), "DESKTOP-ABC".to_string()),
            ("PROCESSOR_ARCHITECTURE".to_string(), "AMD64".to_string()),
            ("NUMBER_OF_PROCESSORS".to_string(), "16".to_string()),
        ]);
        let args = build_bwrap_args(&[], &[], SandboxPermissions::default(), None, false, &env);
        assert!(!args.iter().any(|arg| arg == "--setenv"));
    }

    #[test]
    fn bwrap_forwards_portable_env_alongside_windows_specific_env() {
        // A blocklist (not an allowlist) means genuinely portable variables
        // still reach the command even when Windows-only ones are present.
        let env = HashMap::from([
            ("USERPROFILE".to_string(), r"C:\Users\me".to_string()),
            ("LANG".to_string(), "en_US.UTF-8".to_string()),
            ("CARGO_TERM_COLOR".to_string(), "always".to_string()),
        ]);
        let args = build_bwrap_args(&[], &[], SandboxPermissions::default(), None, false, &env);
        assert!(
            args.windows(3)
                .any(|window| window == ["--setenv", "LANG", "en_US.UTF-8"])
        );
        assert!(
            args.windows(3)
                .any(|window| window == ["--setenv", "CARGO_TERM_COLOR", "always"])
        );
        assert!(!args.windows(3).any(|window| {
            matches!(window, [flag, name, _]
                if flag.as_str() == "--setenv"
                    && name.eq_ignore_ascii_case("USERPROFILE"))
        }));
    }

    #[test]
    fn bwrap_skips_env_names_setenv_would_reject() {
        // bwrap's `--setenv` calls `setenv(3)`, which rejects empty names and
        // names containing `=`. Windows environments include the per-drive
        // current-directory pseudo-variables (`=C:`, ...); forwarding them
        // would abort bwrap with "setenv failed".
        let env = HashMap::from([
            ("=C:".to_string(), r"C:\Users\me".to_string()),
            (String::new(), "value".to_string()),
            ("OK".to_string(), "value".to_string()),
        ]);
        let args = build_bwrap_args(&[], &[], SandboxPermissions::default(), None, false, &env);
        assert!(
            args.windows(3)
                .any(|window| window == ["--setenv", "OK", "value"])
        );
        assert_eq!(args.iter().filter(|arg| *arg == "--setenv").count(), 1);
    }
}
