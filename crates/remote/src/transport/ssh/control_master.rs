use super::*;

/// Check if the user already has an active SSH ControlMaster session for the
/// given destination. See: https://github.com/mav-industries/mav/issues/45271
#[cfg(not(windows))]
pub(super) async fn find_existing_control_master(
    destination: &str,
    additional_args: &[String],
) -> Option<PathBuf> {
    // Use `ssh -G` to resolve the user's effective SSH config for this host.
    // This expands ControlPath tokens (%h, %p, %r, %C, etc.) into actual paths.
    let output = match util::command::new_command("ssh")
        .args(additional_args)
        .arg("-G")
        .arg(destination)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
    {
        Ok(output) => output,
        Err(e) => {
            log::debug!("failed to run ssh -G: {e}");
            return None;
        }
    };

    if !output.status.success() {
        log::debug!("ssh -G failed for {destination}, skipping ControlMaster reuse");
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let control_path = stdout.lines().find_map(|line| {
        let path = line.strip_prefix("controlpath ")?.trim();
        if path == "none" || path.is_empty() {
            None
        } else {
            Some(PathBuf::from(path))
        }
    })?;

    // Verify the master is actually alive by sending a control command.
    let check = match util::command::new_command("ssh")
        .args(additional_args)
        .args(["-O", "check"])
        .arg("-o")
        .arg(format!("ControlPath={}", control_path.display()))
        .arg(destination)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .await
    {
        Ok(output) => output,
        Err(e) => {
            log::debug!("failed to run ssh -O check: {e}");
            return None;
        }
    };

    if check.status.success() {
        log::info!(
            "reusing existing SSH ControlMaster at {}",
            control_path.display()
        );
        Some(control_path)
    } else {
        log::debug!(
            "ControlMaster socket at {} is not alive, creating new connection",
            control_path.display()
        );
        None
    }
}
