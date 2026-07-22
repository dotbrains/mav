use super::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_command() -> Result<()> {
        let mut input_env = HashMap::default();
        input_env.insert("INPUT_VA".to_string(), "val".to_string());
        let mut env = HashMap::default();
        env.insert("SSH_VAR".to_string(), "ssh-val".to_string());

        // Test non-interactive command (interactive=false should use -T)
        let command = build_command_posix(
            Some("remote_program".to_string()),
            &["arg1".to_string(), "arg2".to_string()],
            &input_env,
            Some("~/work".to_string()),
            None,
            env.clone(),
            PathStyle::Posix,
            "/bin/bash",
            ShellKind::Posix,
            vec!["-o".to_string(), "ControlMaster=auto".to_string()],
            "user@host",
            Interactive::No,
        )?;
        assert_eq!(command.program, "ssh");
        // Should contain -T for non-interactive
        assert!(command.args.iter().any(|arg| arg == "-T"));
        assert!(!command.args.iter().any(|arg| arg == "-t"));

        // Test interactive command (interactive=true should use -t)
        let command = build_command_posix(
            Some("remote_program".to_string()),
            &["arg1".to_string(), "arg2".to_string()],
            &input_env,
            Some("~/work".to_string()),
            None,
            env.clone(),
            PathStyle::Posix,
            "/bin/fish",
            ShellKind::Fish,
            vec!["-p".to_string(), "2222".to_string()],
            "user@host",
            Interactive::Yes,
        )?;

        assert_eq!(command.program, "ssh");
        assert_eq!(
            command.args.iter().map(String::as_str).collect::<Vec<_>>(),
            [
                "-p",
                "2222",
                "-o",
                "LogLevel=ERROR",
                "-t",
                "user@host",
                "cd \"$HOME\"/work && exec env 'INPUT_VA=val' remote_program arg1 arg2"
            ]
        );
        assert_eq!(command.env, env);

        let mut input_env = HashMap::default();
        input_env.insert("INPUT_VA".to_string(), "val".to_string());
        let mut env = HashMap::default();
        env.insert("SSH_VAR".to_string(), "ssh-val".to_string());

        let command = build_command_posix(
            None,
            &[],
            &input_env,
            None,
            Some((1, "foo".to_owned(), 2)),
            env.clone(),
            PathStyle::Posix,
            "/bin/fish",
            ShellKind::Fish,
            vec!["-p".to_string(), "2222".to_string()],
            "user@host",
            Interactive::Yes,
        )?;

        assert_eq!(command.program, "ssh");
        assert_eq!(
            command.args.iter().map(String::as_str).collect::<Vec<_>>(),
            [
                "-p",
                "2222",
                "-L",
                "1:foo:2",
                "-o",
                "LogLevel=ERROR",
                "-t",
                "user@host",
                "cd && exec env 'INPUT_VA=val' /bin/fish -l"
            ]
        );
        assert_eq!(command.env, env);

        Ok(())
    }

    #[test]
    fn test_build_command_quotes_env_assignment() -> Result<()> {
        let mut input_env = HashMap::default();
        input_env.insert("MAV$(echo foo)".to_string(), "value".to_string());

        let command = build_command_posix(
            Some("remote_program".to_string()),
            &[],
            &input_env,
            None,
            None,
            HashMap::default(),
            PathStyle::Posix,
            "/bin/bash",
            ShellKind::Posix,
            vec![],
            "user@host",
            Interactive::No,
        )?;

        let remote_command = command
            .args
            .last()
            .context("missing remote command argument")?;
        assert!(
            remote_command.contains("exec env 'MAV$(echo foo)=value' remote_program"),
            "expected env assignment to be quoted, got: {remote_command}"
        );

        Ok(())
    }

    #[test]
    fn scp_args_exclude_port_forward_flags() {
        let options = SshConnectionOptions {
            host: "example.com".into(),
            args: Some(vec![
                "-p".to_string(),
                "2222".to_string(),
                "-o".to_string(),
                "StrictHostKeyChecking=no".to_string(),
            ]),
            port_forwards: Some(vec![SshPortForwardOption {
                local_host: Some("127.0.0.1".to_string()),
                local_port: 8080,
                remote_host: Some("127.0.0.1".to_string()),
                remote_port: 80,
            }]),
            ..Default::default()
        };

        let ssh_args = options.additional_args();
        assert!(
            ssh_args.iter().any(|arg| arg.starts_with("-L")),
            "expected ssh args to include port-forward: {ssh_args:?}"
        );

        let scp_args = options.additional_args_for_scp();
        assert_eq!(
            scp_args,
            vec![
                "-p".to_string(),
                "2222".to_string(),
                "-o".to_string(),
                "StrictHostKeyChecking=no".to_string(),
            ]
        );
    }

    #[test]
    fn test_host_parsing() -> Result<()> {
        let opts = SshConnectionOptions::parse_command_line("user@2001:db8::1")?;
        assert_eq!(opts.host, "2001:db8::1".into());
        assert_eq!(opts.username, Some("user".to_string()));
        assert_eq!(opts.port, None);

        let opts = SshConnectionOptions::parse_command_line("user@[2001:db8::1]:2222")?;
        assert_eq!(opts.host, "2001:db8::1".into());
        assert_eq!(opts.username, Some("user".to_string()));
        assert_eq!(opts.port, Some(2222));

        let opts = SshConnectionOptions::parse_command_line("user@[2001:db8::1]")?;
        assert_eq!(opts.host, "2001:db8::1".into());
        assert_eq!(opts.username, Some("user".to_string()));
        assert_eq!(opts.port, None);

        let opts = SshConnectionOptions::parse_command_line("2001:db8::1")?;
        assert_eq!(opts.host, "2001:db8::1".into());
        assert_eq!(opts.username, None);
        assert_eq!(opts.port, None);

        let opts = SshConnectionOptions::parse_command_line("[2001:db8::1]:2222")?;
        assert_eq!(opts.host, "2001:db8::1".into());
        assert_eq!(opts.username, None);
        assert_eq!(opts.port, Some(2222));

        let opts = SshConnectionOptions::parse_command_line("user@example.com:2222")?;
        assert_eq!(opts.host, "example.com".into());
        assert_eq!(opts.username, Some("user".to_string()));
        assert_eq!(opts.port, Some(2222));

        let opts = SshConnectionOptions::parse_command_line("user@192.168.1.1:2222")?;
        assert_eq!(opts.host, "192.168.1.1".into());
        assert_eq!(opts.username, Some("user".to_string()));
        assert_eq!(opts.port, Some(2222));

        Ok(())
    }

    #[test]
    fn test_parse_port_forward_spec_ipv6() -> Result<()> {
        let pf = parse_port_forward_spec("[::1]:8080:[::1]:80")?;
        assert_eq!(pf.local_host, Some("::1".to_string()));
        assert_eq!(pf.local_port, 8080);
        assert_eq!(pf.remote_host, Some("::1".to_string()));
        assert_eq!(pf.remote_port, 80);

        let pf = parse_port_forward_spec("8080:[::1]:80")?;
        assert_eq!(pf.local_host, None);
        assert_eq!(pf.local_port, 8080);
        assert_eq!(pf.remote_host, Some("::1".to_string()));
        assert_eq!(pf.remote_port, 80);

        let pf = parse_port_forward_spec("[2001:db8::1]:3000:[fe80::1]:4000")?;
        assert_eq!(pf.local_host, Some("2001:db8::1".to_string()));
        assert_eq!(pf.local_port, 3000);
        assert_eq!(pf.remote_host, Some("fe80::1".to_string()));
        assert_eq!(pf.remote_port, 4000);

        let pf = parse_port_forward_spec("127.0.0.1:8080:localhost:80")?;
        assert_eq!(pf.local_host, Some("127.0.0.1".to_string()));
        assert_eq!(pf.local_port, 8080);
        assert_eq!(pf.remote_host, Some("localhost".to_string()));
        assert_eq!(pf.remote_port, 80);

        Ok(())
    }

    #[test]
    fn test_port_forward_ipv6_formatting() {
        let options = SshConnectionOptions {
            host: "example.com".into(),
            port_forwards: Some(vec![SshPortForwardOption {
                local_host: Some("::1".to_string()),
                local_port: 8080,
                remote_host: Some("::1".to_string()),
                remote_port: 80,
            }]),
            ..Default::default()
        };

        let args = options.additional_args();
        assert!(
            args.iter().any(|arg| arg == "-L[::1]:8080:[::1]:80"),
            "expected bracketed IPv6 in -L flag: {args:?}"
        );
    }

    #[test]
    fn test_build_command_with_ipv6_port_forward() -> Result<()> {
        let command = build_command_posix(
            None,
            &[],
            &HashMap::default(),
            None,
            Some((8080, "::1".to_owned(), 80)),
            HashMap::default(),
            PathStyle::Posix,
            "/bin/bash",
            ShellKind::Posix,
            vec![],
            "user@host",
            Interactive::No,
        )?;

        assert!(
            command.args.iter().any(|arg| arg == "8080:[::1]:80"),
            "expected bracketed IPv6 in port forward arg: {:?}",
            command.args
        );

        Ok(())
    }
}
