#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    fn probe_output_reports_interop_and_bwrap_path() {
        let probe = parse_probe_output("mav-wsl-probe: interop /usr/bin/bwrap\n").unwrap();
        assert_eq!(
            probe,
            EnvironmentProbe {
                mask_interop_dir: true,
                bwrap_path: "/usr/bin/bwrap".to_string(),
            }
        );

        let probe =
            parse_probe_output("mav-wsl-probe: no-interop /home/me/.nix-profile/bin/bwrap\n")
                .unwrap();
        assert_eq!(
            probe,
            EnvironmentProbe {
                mask_interop_dir: false,
                bwrap_path: "/home/me/.nix-profile/bin/bwrap".to_string(),
            }
        );
    }

    #[test]
    fn probe_output_ignores_profile_noise_even_mentioning_interop() {
        // Login-shell profile scripts run before the probe body and may print
        // arbitrary text; only the marked result line counts.
        let probe = parse_probe_output(
            "welcome to my shell, interop fans\nmav-wsl-probe: no-interop /usr/bin/bwrap\n",
        )
        .unwrap();
        assert!(!probe.mask_interop_dir);
    }

    #[test]
    fn probe_output_rejects_missing_or_malformed_result_line() {
        assert!(parse_probe_output("").is_err());
        assert!(parse_probe_output("profile noise only\n").is_err());
        assert!(parse_probe_output("mav-wsl-probe: interop\n").is_err());
        assert!(parse_probe_output("mav-wsl-probe: maybe /usr/bin/bwrap\n").is_err());
    }

    #[test]
    fn probe_output_rejects_non_absolute_bwrap_path() {
        // `command -v` reports a bare name for shell functions and aliases,
        // which `wsl --exec` could never run.
        assert!(parse_probe_output("mav-wsl-probe: interop bwrap\n").is_err());
    }

    #[test]
    fn probe_script_smoke_tests_the_namespaces_the_real_invocation_uses() {
        // Presence isn't enough: unprivileged user namespaces can be
        // restricted (e.g. Ubuntu 24.04's AppArmor policy), so the probe must
        // actually exercise the namespace flags `build_bwrap_args` emits.
        let script = probe_script();
        for flag in [
            "--unshare-user",
            "--unshare-net",
            "--unshare-ipc",
            "--unshare-uts",
            "--unshare-pid",
            "--unshare-cgroup-try",
            "--ro-bind / /",
        ] {
            assert!(script.contains(flag), "probe script must contain {flag}");
        }
        assert!(script.contains("exit 41"));
        assert!(script.contains("exit 42"));
    }

    #[test]
    fn probe_script_rejects_setuid_root_bwrap_before_smoke_test() {
        let script = probe_script();
        let guard =
            "[ -u \"$bwrap_path\" ] && [ \"$(stat -c %u \"$bwrap_path\" 2>/dev/null)\" = 0 ]";
        let smoke_test = "\"$bwrap_path\" --ro-bind / /";
        let Some(guard_index) = script.find(guard) else {
            panic!("probe script must contain setuid-root guard: {script}");
        };
        let Some(smoke_test_index) = script.find(smoke_test) else {
            panic!("probe script must contain bwrap smoke test: {script}");
        };

        assert!(script.contains("setuid-root bwrap is not supported"));
        assert!(script.contains(&format!("exit {BWRAP_UNUSABLE_EXIT_CODE}; fi")));
        assert!(guard_index < smoke_test_index);
    }
}
