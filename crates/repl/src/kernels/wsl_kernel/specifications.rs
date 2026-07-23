use super::*;

#[derive(serde::Deserialize)]
struct LocalKernelSpecsResponse {
    kernelspecs: std::collections::HashMap<String, LocalKernelSpec>,
}

#[derive(serde::Deserialize)]
struct LocalKernelSpec {
    spec: LocalKernelSpecContent,
}

#[derive(serde::Deserialize)]
struct LocalKernelSpecContent {
    argv: Vec<String>,
    display_name: String,
    language: String,
    interrupt_mode: Option<String>,
    env: Option<std::collections::HashMap<String, String>>,
    metadata: Option<std::collections::HashMap<String, serde_json::Value>>,
}

pub async fn wsl_kernel_specifications(
    background_executor: BackgroundExecutor,
) -> Result<Vec<KernelSpecification>> {
    let output = util::command::new_command("wsl")
        .arg("-l")
        .arg("-q")
        .output()
        .await;

    if output.is_err() {
        return Ok(Vec::new());
    }

    let output = output.unwrap();
    if !output.status.success() {
        return Ok(Vec::new());
    }

    // wsl output is often UTF-16LE, but -l -q might be simpler or just ASCII compatible if not using weird charsets.
    // However, on Windows, wsl often outputs UTF-16LE.
    // We can try to detect or use from_utf16 if valid, or just use String::from_utf8_lossy and see.
    // Actually, `smol::process` on Windows might receive bytes that are UTF-16LE if wsl writes that.
    // But typically terminal output for wsl is UTF-16.
    // Let's try to parse as UTF-16LE if it looks like it (BOM or just 00 bytes).

    let stdout = output.stdout;
    let distros_str = if stdout.len() >= 2 && stdout[1] == 0 {
        // likely UTF-16LE
        let u16s: Vec<u16> = stdout
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&u16s)
    } else {
        String::from_utf8_lossy(&stdout).to_string()
    };

    let distros: Vec<String> = distros_str
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();

    let tasks = distros.into_iter().map(|distro| {
        background_executor.spawn(async move {
            let output = util::command::new_command("wsl")
                .arg("-d")
                .arg(&distro)
                .arg("bash")
                .arg("-l")
                .arg("-c")
                .arg("jupyter kernelspec list --json")
                .output()
                .await;

            if let Ok(output) = output {
                if output.status.success() {
                    let json_str = String::from_utf8_lossy(&output.stdout);
                    // Use local permissive struct instead of strict KernelSpecsResponse from jupyter-protocol
                    if let Ok(specs_response) =
                        serde_json::from_str::<LocalKernelSpecsResponse>(&json_str)
                    {
                        return specs_response
                            .kernelspecs
                            .into_iter()
                            .map(|(name, spec)| {
                                KernelSpecification::WslRemote(WslKernelSpecification {
                                    name,
                                    kernelspec: jupyter_protocol::JupyterKernelspec {
                                        argv: spec.spec.argv,
                                        display_name: spec.spec.display_name,
                                        language: spec.spec.language,
                                        interrupt_mode: spec.spec.interrupt_mode,
                                        env: spec.spec.env,
                                        metadata: spec.spec.metadata,
                                    },
                                    distro: distro.clone(),
                                })
                            })
                            .collect::<Vec<_>>();
                    } else if let Err(e) =
                        serde_json::from_str::<LocalKernelSpecsResponse>(&json_str)
                    {
                        log::error!(
                            "wsl_kernel_specifications parse error: {} \nJSON: {}",
                            e,
                            json_str
                        );
                    }
                } else {
                    log::error!("wsl_kernel_specifications command failed");
                }
            } else if let Err(e) = output {
                log::error!("wsl_kernel_specifications command execution failed: {}", e);
            }

            Vec::new()
        })
    });

    let specs: Vec<_> = futures::future::join_all(tasks)
        .await
        .into_iter()
        .flatten()
        .collect();

    Ok(specs)
}
