use super::*;

#[cfg(not(windows))]
impl MasterProcess {
    pub fn new(
        askpass_script_path: &std::ffi::OsStr,
        additional_args: Vec<String>,
        socket_path: &std::path::Path,
        destination: &str,
    ) -> Result<Self> {
        let args = [
            "-N",
            "-o",
            "ControlPersist=no",
            "-o",
            "ControlMaster=yes",
            "-o",
        ];

        let mut master_process = util::command::new_command("ssh");
        master_process
            .kill_on_drop(true)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("SSH_ASKPASS_REQUIRE", "force")
            .env("SSH_ASKPASS", askpass_script_path)
            .args(additional_args)
            .args(args);

        master_process.arg(format!("ControlPath={}", socket_path.display()));

        let process = master_process.arg(&destination).spawn()?;

        Ok(MasterProcess { process })
    }

    pub async fn wait_connected(&mut self) -> Result<()> {
        let Some(mut stdout) = self.process.stdout.take() else {
            anyhow::bail!("ssh process stdout capture failed");
        };

        let mut output = Vec::new();
        stdout.read_to_end(&mut output).await?;
        Ok(())
    }
}

#[cfg(windows)]
impl MasterProcess {
    const CONNECTION_ESTABLISHED_MAGIC: &str = "MAV_SSH_CONNECTION_ESTABLISHED";

    pub fn new(
        askpass_script_path: &std::ffi::OsStr,
        askpass_socket_path: &std::ffi::OsStr,
        additional_args: Vec<String>,
        destination: &str,
    ) -> Result<Self> {
        // On Windows, `ControlMaster` and `ControlPath` are not supported:
        // https://github.com/PowerShell/Win32-OpenSSH/issues/405
        // https://github.com/PowerShell/Win32-OpenSSH/wiki/Project-Scope
        //
        // Using an ugly workaround to detect connection establishment
        // -N doesn't work with JumpHosts as windows openssh never closes stdin in that case
        let args = [
            "-t",
            &format!("echo '{}'; exec $0", Self::CONNECTION_ESTABLISHED_MAGIC),
        ];

        let mut master_process = util::command::new_command("ssh");
        master_process
            .kill_on_drop(true)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("SSH_ASKPASS_REQUIRE", "force")
            .env("SSH_ASKPASS", askpass_script_path)
            .env("MAV_ASKPASS_SOCKET", askpass_socket_path)
            .args(additional_args)
            .arg(destination)
            .args(args);

        let process = master_process.spawn()?;

        Ok(MasterProcess { process })
    }

    pub async fn wait_connected(&mut self) -> Result<()> {
        use smol::io::AsyncBufReadExt;

        let Some(stdout) = self.process.stdout.take() else {
            anyhow::bail!("ssh process stdout capture failed");
        };

        let mut reader = smol::io::BufReader::new(stdout);

        let mut line = String::new();

        loop {
            let n = reader.read_line(&mut line).await?;
            if n == 0 {
                anyhow::bail!("ssh process exited before connection established");
            }

            if line.contains(Self::CONNECTION_ESTABLISHED_MAGIC) {
                return Ok(());
            }
        }
    }
}

impl AsRef<Child> for MasterProcess {
    fn as_ref(&self) -> &Child {
        &self.process
    }
}

impl AsMut<Child> for MasterProcess {
    fn as_mut(&mut self) -> &mut Child {
        &mut self.process
    }
}
