use super::{
    KernelSpecification, LocalKernelSpecification, SshRemoteKernelSpecification,
    WslKernelSpecification,
};
use anyhow::Result;
use futures::StreamExt;
use gpui::{App, Entity};
use jupyter_protocol::JupyterKernelspec;
use language::LanguageName;
use project::{Project, ProjectPath, Toolchains, WorktreeId};
use remote::RemoteConnectionOptions;
use std::{collections::HashMap, future::Future, path::PathBuf};
use util::rel_path::RelPath;

pub(crate) const VENV_DIR_NAMES: &[&str] = &[".venv", "venv", ".env", "env"];

// Build a POSIX shell script that attempts to find and exec the best Python binary to run with the given arguments.
pub(crate) fn build_python_exec_shell_script(
    python_args: &str,
    cd_command: &str,
    env_command: &str,
) -> String {
    let venv_dirs = VENV_DIR_NAMES.join(" ");
    format!(
        "set -e; \
         {cd_command}\
         {env_command}\
         for venv_dir in {venv_dirs}; do \
           if [ -f \"$venv_dir/pyvenv.cfg\" ] || [ -f \"$venv_dir/bin/activate\" ]; then \
             if [ -x \"$venv_dir/bin/python\" ]; then \
               exec \"$venv_dir/bin/python\" {python_args}; \
             elif [ -x \"$venv_dir/bin/python3\" ]; then \
               exec \"$venv_dir/bin/python3\" {python_args}; \
             fi; \
           fi; \
         done; \
         if command -v python3 >/dev/null 2>&1; then \
           exec python3 {python_args}; \
         elif command -v python >/dev/null 2>&1; then \
           exec python {python_args}; \
         else \
           echo 'Error: Python not found in virtual environment or PATH' >&2; \
           exit 127; \
         fi"
    )
}

/// Build a POSIX shell script that outputs the best Python binary.
#[cfg(target_os = "windows")]
pub(crate) fn build_python_discovery_shell_script() -> String {
    let venv_dirs = VENV_DIR_NAMES.join(" ");
    format!(
        "for venv_dir in {venv_dirs}; do \
           if [ -f \"$venv_dir/pyvenv.cfg\" ] || [ -f \"$venv_dir/bin/activate\" ]; then \
             if [ -x \"$venv_dir/bin/python\" ]; then \
               echo \"$venv_dir/bin/python\"; exit 0; \
             elif [ -x \"$venv_dir/bin/python3\" ]; then \
               echo \"$venv_dir/bin/python3\"; exit 0; \
             fi; \
           fi; \
         done; \
         if command -v python3 >/dev/null 2>&1; then \
           echo python3; exit 0; \
         elif command -v python >/dev/null 2>&1; then \
           echo python; exit 0; \
         fi; \
         exit 1"
    )
}

#[derive(Debug, Clone)]
pub struct PythonEnvKernelSpecification {
    pub name: String,
    pub path: PathBuf,
    pub kernelspec: JupyterKernelspec,
    pub has_ipykernel: bool,
    /// Display label for the environment type: "venv", "Conda", "Pyenv", etc.
    pub environment_kind: Option<String>,
}

impl PartialEq for PythonEnvKernelSpecification {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.path == other.path
    }
}

impl Eq for PythonEnvKernelSpecification {}

impl PythonEnvKernelSpecification {
    pub fn as_local_spec(&self) -> LocalKernelSpecification {
        LocalKernelSpecification {
            name: self.name.clone(),
            path: self.path.clone(),
            kernelspec: self.kernelspec.clone(),
        }
    }

    pub fn is_uv(&self) -> bool {
        matches!(
            self.environment_kind.as_deref(),
            Some("uv" | "uv (Workspace)")
        )
    }
}

fn extract_environment_kind(toolchain_json: &serde_json::Value) -> Option<String> {
    let kind_str = toolchain_json.get("kind")?.as_str()?;
    let label = match kind_str {
        "Conda" => "Conda",
        "Pixi" => "pixi",
        "Homebrew" => "Homebrew",
        "Pyenv" => "global (Pyenv)",
        "GlobalPaths" => "global",
        "PyenvVirtualEnv" => "Pyenv",
        "Pipenv" => "Pipenv",
        "Poetry" => "Poetry",
        "MacPythonOrg" => "global (Python.org)",
        "MacCommandLineTools" => "global (Command Line Tools for Xcode)",
        "LinuxGlobal" => "global",
        "MacXCode" => "global (Xcode)",
        "Venv" => "venv",
        "VirtualEnv" => "virtualenv",
        "VirtualEnvWrapper" => "virtualenvwrapper",
        "WindowsStore" => "global (Windows Store)",
        "WindowsRegistry" => "global (Windows Registry)",
        "Uv" => "uv",
        "UvWorkspace" => "uv (Workspace)",
        _ => kind_str,
    };
    Some(label.to_string())
}

pub fn python_env_kernel_specifications(
    project: &Entity<Project>,
    worktree_id: WorktreeId,
    cx: &mut App,
) -> impl Future<Output = Result<Vec<KernelSpecification>>> + use<> {
    let python_language = LanguageName::new_static("Python");
    let is_remote = project.read(cx).is_remote();
    let wsl_distro = project
        .read(cx)
        .remote_connection_options(cx)
        .and_then(|opts| {
            if let RemoteConnectionOptions::Wsl(wsl) = opts {
                Some(wsl.distro_name)
            } else {
                None
            }
        });

    let toolchains = project.read(cx).available_toolchains(
        ProjectPath {
            worktree_id,
            path: RelPath::empty_arc(),
        },
        python_language,
        cx,
    );
    #[allow(unused)]
    let worktree_root_path: Option<std::sync::Arc<std::path::Path>> = project
        .read(cx)
        .worktree_for_id(worktree_id, cx)
        .map(|w| w.read(cx).abs_path());

    let background_executor = cx.background_executor().clone();

    async move {
        let (toolchains, user_toolchains) = if let Some(Toolchains {
            toolchains,
            root_path: _,
            user_toolchains,
        }) = toolchains.await
        {
            (toolchains, user_toolchains)
        } else {
            return Ok(Vec::new());
        };

        let kernelspecs = user_toolchains
            .into_values()
            .flatten()
            .chain(toolchains.toolchains)
            .map(|toolchain| {
                let wsl_distro = wsl_distro.clone();
                background_executor.spawn(async move {
                    if is_remote {
                        let default_kernelspec = JupyterKernelspec {
                            argv: vec![
                                toolchain.path.to_string(),
                                "-m".to_string(),
                                "ipykernel_launcher".to_string(),
                                "-f".to_string(),
                                "{connection_file}".to_string(),
                            ],
                            display_name: toolchain.name.to_string(),
                            language: "python".to_string(),
                            interrupt_mode: None,
                            metadata: None,
                            env: None,
                        };

                        if let Some(distro) = wsl_distro {
                            log::debug!(
                                "python_env_kernel_specifications: returning WslRemote for toolchain {}",
                                toolchain.name
                            );
                            return Some(KernelSpecification::WslRemote(WslKernelSpecification {
                                name: toolchain.name.to_string(),
                                kernelspec: default_kernelspec,
                                distro,
                            }));
                        }

                        log::debug!(
                            "python_env_kernel_specifications: returning SshRemote for toolchain {}",
                            toolchain.name
                        );
                        return Some(KernelSpecification::SshRemote(
                            SshRemoteKernelSpecification {
                                name: format!("Remote {}", toolchain.name),
                                path: toolchain.path.clone(),
                                kernelspec: default_kernelspec,
                            },
                        ));
                    }

                    let python_path = toolchain.path.to_string();
                    let environment_kind = extract_environment_kind(&toolchain.as_json);

                    let has_ipykernel = util::command::new_command(&python_path)
                        .args(&["-c", "import ipykernel"])
                        .output()
                        .await
                        .map(|output| output.status.success())
                        .unwrap_or(false);

                    let mut env = HashMap::new();
                    if let Some(python_bin_dir) = PathBuf::from(&python_path).parent() {
                        if let Some(path_var) = std::env::var_os("PATH") {
                            let mut paths = std::env::split_paths(&path_var).collect::<Vec<_>>();
                            paths.insert(0, python_bin_dir.to_path_buf());
                            if let Ok(new_path) = std::env::join_paths(paths) {
                                env.insert(
                                    "PATH".to_string(),
                                    new_path.to_string_lossy().to_string(),
                                );
                            }
                        }

                        if let Some(venv_root) = python_bin_dir.parent() {
                            env.insert(
                                "VIRTUAL_ENV".to_string(),
                                venv_root.to_string_lossy().to_string(),
                            );
                        }
                    }

                    log::info!("Preparing Python kernel for toolchain: {}", toolchain.name);
                    log::info!("Python path: {}", python_path);
                    if let Some(path) = env.get("PATH") {
                        log::info!("Kernel PATH: {}", path);
                    } else {
                        log::info!("Kernel PATH not set in env");
                    }
                    if let Some(venv) = env.get("VIRTUAL_ENV") {
                        log::info!("Kernel VIRTUAL_ENV: {}", venv);
                    }

                    let kernelspec = JupyterKernelspec {
                        argv: vec![
                            python_path.clone(),
                            "-m".to_string(),
                            "ipykernel_launcher".to_string(),
                            "-f".to_string(),
                            "{connection_file}".to_string(),
                        ],
                        display_name: toolchain.name.to_string(),
                        language: "python".to_string(),
                        interrupt_mode: None,
                        metadata: None,
                        env: Some(env),
                    };

                    Some(KernelSpecification::PythonEnv(PythonEnvKernelSpecification {
                        name: toolchain.name.to_string(),
                        path: PathBuf::from(&python_path),
                        kernelspec,
                        has_ipykernel,
                        environment_kind,
                    }))
                })
            });

        #[allow(unused_mut)]
        let mut kernel_specs: Vec<KernelSpecification> = futures::stream::iter(kernelspecs)
            .buffer_unordered(4)
            .filter_map(|x| async move { x })
            .collect::<Vec<_>>()
            .await;

        #[cfg(target_os = "windows")]
        if kernel_specs.is_empty() && !is_remote {
            if let Some(root_path) = worktree_root_path {
                let root_path_str: std::borrow::Cow<str> = root_path.to_string_lossy();
                let (distro, internal_path) =
                    if let Some(path_without_prefix) = root_path_str.strip_prefix(r"\\wsl$\") {
                        if let Some((distro, path)) = path_without_prefix.split_once('\\') {
                            let replaced_path: String = path.replace('\\', "/");
                            (Some(distro), Some(format!("/{}", replaced_path)))
                        } else {
                            (Some(path_without_prefix), Some("/".to_string()))
                        }
                    } else if let Some(path_without_prefix) =
                        root_path_str.strip_prefix(r"\\wsl.localhost\")
                    {
                        if let Some((distro, path)) = path_without_prefix.split_once('\\') {
                            let replaced_path: String = path.replace('\\', "/");
                            (Some(distro), Some(format!("/{}", replaced_path)))
                        } else {
                            (Some(path_without_prefix), Some("/".to_string()))
                        }
                    } else {
                        (None, None)
                    };

                if let (Some(distro), Some(internal_path)) = (distro, internal_path) {
                    let discovery_script = build_python_discovery_shell_script();
                    let script = format!(
                        "cd {} && {}",
                        shlex::try_quote(&internal_path)
                            .unwrap_or(std::borrow::Cow::Borrowed(&internal_path)),
                        discovery_script
                    );
                    let output = util::command::new_command("wsl")
                        .arg("-d")
                        .arg(distro)
                        .arg("bash")
                        .arg("-l")
                        .arg("-c")
                        .arg(&script)
                        .output()
                        .await;

                    if let Ok(output) = output {
                        if output.status.success() {
                            let python_cmd =
                                String::from_utf8_lossy(&output.stdout).trim().to_string();
                            let (python_path, display_suffix) = if python_cmd.contains('/') {
                                let venv_name = python_cmd.split('/').next().unwrap_or("venv");
                                (
                                    format!("{}/{}", internal_path, python_cmd),
                                    format!("({})", venv_name),
                                )
                            } else {
                                (python_cmd, "(System)".to_string())
                            };

                            let display_name = format!("WSL: {} {}", distro, display_suffix);
                            let default_kernelspec = JupyterKernelspec {
                                argv: vec![
                                    python_path,
                                    "-m".to_string(),
                                    "ipykernel_launcher".to_string(),
                                    "-f".to_string(),
                                    "{connection_file}".to_string(),
                                ],
                                display_name: display_name.clone(),
                                language: "python".to_string(),
                                interrupt_mode: None,
                                metadata: None,
                                env: None,
                            };

                            kernel_specs.push(KernelSpecification::WslRemote(
                                WslKernelSpecification {
                                    name: display_name,
                                    kernelspec: default_kernelspec,
                                    distro: distro.to_string(),
                                },
                            ));
                        }
                    }
                }
            }
        }

        anyhow::Ok(kernel_specs)
    }
}
