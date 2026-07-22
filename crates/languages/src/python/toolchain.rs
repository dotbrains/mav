use super::*;

#[path = "toolchain/environment_api.rs"]
mod environment_api;
#[path = "toolchain/environment_kind.rs"]
mod environment_kind;

use environment_api::EnvironmentApi;
use environment_kind::{is_python_env_global, python_env_kind_display};

pub(crate) struct PythonToolchainProvider {
    fs: Arc<dyn Fs>,
}

impl PythonToolchainProvider {
    pub fn new(fs: Arc<dyn Fs>) -> Self {
        Self { fs }
    }
}

static ENV_PRIORITY_LIST: &[PythonEnvironmentKind] = &[
    // Prioritize non-Conda environments.
    PythonEnvironmentKind::UvWorkspace,
    PythonEnvironmentKind::Uv,
    PythonEnvironmentKind::Poetry,
    PythonEnvironmentKind::Pipenv,
    PythonEnvironmentKind::VirtualEnvWrapper,
    PythonEnvironmentKind::Venv,
    PythonEnvironmentKind::VirtualEnv,
    PythonEnvironmentKind::PyenvVirtualEnv,
    PythonEnvironmentKind::Pixi,
    PythonEnvironmentKind::Conda,
    PythonEnvironmentKind::Pyenv,
    PythonEnvironmentKind::GlobalPaths,
    PythonEnvironmentKind::Homebrew,
];

fn env_priority(kind: Option<PythonEnvironmentKind>) -> usize {
    if let Some(kind) = kind {
        ENV_PRIORITY_LIST
            .iter()
            .position(|blessed_env| blessed_env == &kind)
            .unwrap_or(ENV_PRIORITY_LIST.len())
    } else {
        // Unknown toolchains are less useful than non-blessed ones.
        ENV_PRIORITY_LIST.len() + 1
    }
}

/// Return the name of environment declared in <worktree-root/.venv.
///
/// https://virtualfish.readthedocs.io/en/latest/plugins.html#auto-activation-auto-activation
async fn get_worktree_venv_declaration(worktree_root: &Path) -> Option<String> {
    let file = async_fs::File::open(worktree_root.join(".venv"))
        .await
        .ok()?;
    let mut venv_name = String::new();
    smol::io::BufReader::new(file)
        .read_line(&mut venv_name)
        .await
        .ok()?;
    Some(venv_name.trim().to_string())
}

fn get_venv_parent_dir(env: &PythonEnvironment) -> Option<PathBuf> {
    // If global, we aren't a virtual environment
    if let Some(kind) = env.kind
        && is_python_env_global(&kind)
    {
        return None;
    }

    // Check to be sure we are a virtual environment using pet's most generic
    // virtual environment type, VirtualEnv
    let venv = env
        .executable
        .as_ref()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .filter(|p| is_virtualenv_dir(p))?;

    venv.parent().map(|parent| parent.to_path_buf())
}

// How far is this venv from the root of our current project?
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum SubprojectDistance {
    WithinSubproject(Reverse<usize>),
    WithinWorktree(Reverse<usize>),
    NotInWorktree,
}

fn wr_distance(
    wr: &PathBuf,
    subroot_relative_path: &RelPath,
    venv: Option<&PathBuf>,
) -> SubprojectDistance {
    if let Some(venv) = venv
        && let Ok(p) = venv.strip_prefix(wr)
    {
        if subroot_relative_path.components().next().is_some()
            && let Ok(distance) = p
                .strip_prefix(subroot_relative_path.as_std_path())
                .map(|p| p.components().count())
        {
            SubprojectDistance::WithinSubproject(Reverse(distance))
        } else {
            SubprojectDistance::WithinWorktree(Reverse(p.components().count()))
        }
    } else {
        SubprojectDistance::NotInWorktree
    }
}

fn micromamba_shell_name(kind: ShellKind) -> &'static str {
    match kind {
        ShellKind::Csh => "csh",
        ShellKind::Fish => "fish",
        ShellKind::Nushell => "nu",
        ShellKind::PowerShell | ShellKind::Pwsh => "powershell",
        ShellKind::Cmd => "cmd.exe",
        // default / catch-all:
        _ => "posix",
    }
}

#[async_trait]
impl ToolchainLister for PythonToolchainProvider {
    async fn list(
        &self,
        worktree_root: PathBuf,
        subroot_relative_path: Arc<RelPath>,
        project_env: Option<HashMap<String, String>>,
    ) -> ToolchainList {
        let fs = &*self.fs;
        let env = project_env.unwrap_or_default();
        let environment = EnvironmentApi::from_env(&env);
        let locators = pet::locators::create_locators(
            Arc::new(pet_conda::Conda::from(&environment)),
            Arc::new(pet_poetry::Poetry::from(&environment)),
            &environment,
        );
        let mut config = Configuration::default();

        // `.ancestors()` will yield at least one path, so in case of empty `subroot_relative_path`, we'll just use
        // worktree root as the workspace directory.
        config.workspace_directories = Some(
            subroot_relative_path
                .ancestors()
                .map(|ancestor| {
                    // remove trailing separator as it alters the environment name hash used by Poetry.
                    let path = worktree_root.join(ancestor.as_std_path());
                    let path_str = path.to_string_lossy();
                    if path_str.ends_with(std::path::MAIN_SEPARATOR) && path_str.len() > 1 {
                        PathBuf::from(path_str.trim_end_matches(std::path::MAIN_SEPARATOR))
                    } else {
                        path
                    }
                })
                .collect(),
        );
        for locator in locators.iter() {
            locator.configure(&config);
        }

        let reporter = pet_reporter::collect::create_reporter();
        pet::find::find_and_report_envs(&reporter, config, &locators, &environment, None);

        let mut toolchains = reporter
            .environments
            .lock()
            .map_or(Vec::new(), |mut guard| std::mem::take(&mut guard));

        let wr = worktree_root;
        let wr_venv = get_worktree_venv_declaration(&wr).await;
        // Sort detected environments by:
        //     environment name matching activation file (<workdir>/.venv)
        //     environment project dir matching worktree_root
        //     general env priority
        //     environment path matching the CONDA_PREFIX env var
        //     executable path
        toolchains.sort_by(|lhs, rhs| {
            // Compare venv names against worktree .venv file
            let venv_ordering =
                wr_venv
                    .as_ref()
                    .map_or(Ordering::Equal, |venv| match (&lhs.name, &rhs.name) {
                        (Some(l), Some(r)) => (r == venv).cmp(&(l == venv)),
                        (Some(l), None) if l == venv => Ordering::Less,
                        (None, Some(r)) if r == venv => Ordering::Greater,
                        _ => Ordering::Equal,
                    });

            // Compare project paths against worktree root
            let proj_ordering =
                || {
                    let lhs_project = lhs.project.clone().or_else(|| get_venv_parent_dir(lhs));
                    let rhs_project = rhs.project.clone().or_else(|| get_venv_parent_dir(rhs));
                    wr_distance(&wr, &subroot_relative_path, lhs_project.as_ref()).cmp(
                        &wr_distance(&wr, &subroot_relative_path, rhs_project.as_ref()),
                    )
                };

            // Compare environment priorities
            let priority_ordering = || env_priority(lhs.kind).cmp(&env_priority(rhs.kind));

            // Compare conda prefixes
            let conda_ordering = || {
                if lhs.kind == Some(PythonEnvironmentKind::Conda) {
                    environment
                        .get_env_var("CONDA_PREFIX".to_string())
                        .map(|conda_prefix| {
                            let is_match = |exe: &Option<PathBuf>| {
                                exe.as_ref().is_some_and(|e| e.starts_with(&conda_prefix))
                            };
                            match (is_match(&lhs.executable), is_match(&rhs.executable)) {
                                (true, false) => Ordering::Less,
                                (false, true) => Ordering::Greater,
                                _ => Ordering::Equal,
                            }
                        })
                        .unwrap_or(Ordering::Equal)
                } else {
                    Ordering::Equal
                }
            };

            // Compare Python executables
            let exe_ordering = || lhs.executable.cmp(&rhs.executable);

            venv_ordering
                .then_with(proj_ordering)
                .then_with(priority_ordering)
                .then_with(conda_ordering)
                .then_with(exe_ordering)
        });

        let mut out_toolchains = Vec::new();
        for toolchain in toolchains {
            let Some(toolchain) = venv_to_toolchain(toolchain, fs).await else {
                continue;
            };
            out_toolchains.push(toolchain);
        }
        out_toolchains.dedup();
        ToolchainList {
            toolchains: out_toolchains,
            default: None,
            groups: Default::default(),
        }
    }
    fn meta(&self) -> ToolchainMetadata {
        ToolchainMetadata {
            term: SharedString::new_static("Virtual Environment"),
            new_toolchain_placeholder: SharedString::new_static(
                "A path to the python3 executable within a virtual environment, or path to virtual environment itself",
            ),
            manifest_name: ManifestName::from(SharedString::new_static("pyproject.toml")),
        }
    }

    async fn resolve(
        &self,
        path: PathBuf,
        env: Option<HashMap<String, String>>,
    ) -> anyhow::Result<Toolchain> {
        let fs = &*self.fs;
        let env = env.unwrap_or_default();
        let environment = EnvironmentApi::from_env(&env);
        let locators = pet::locators::create_locators(
            Arc::new(pet_conda::Conda::from(&environment)),
            Arc::new(pet_poetry::Poetry::from(&environment)),
            &environment,
        );
        let toolchain = pet::resolve::resolve_environment(&path, &locators, &environment)
            .context("Could not find a virtual environment in provided path")?;
        let venv = toolchain.resolved.unwrap_or(toolchain.discovered);
        venv_to_toolchain(venv, fs)
            .await
            .context("Could not convert a venv into a toolchain")
    }

    fn activation_script(
        &self,
        toolchain: &Toolchain,
        shell: ShellKind,
        cx: &App,
    ) -> BoxFuture<'static, Vec<String>> {
        let settings = TerminalSettings::get_global(cx);
        let conda_manager = settings
            .detect_venv
            .as_option()
            .map(|venv| venv.conda_manager)
            .unwrap_or(settings::CondaManager::Auto);

        let toolchain_clone = toolchain.clone();
        Box::pin(async move {
            let Ok(toolchain) =
                serde_json::from_value::<PythonToolchainData>(toolchain_clone.as_json.clone())
            else {
                return vec![];
            };

            log::debug!("(Python) Composing activation script for toolchain {toolchain:?}");

            let mut activation_script = vec![];

            match toolchain.environment.kind {
                Some(PythonEnvironmentKind::Conda) => {
                    if toolchain.environment.manager.is_none() {
                        return vec![];
                    };

                    let manager = match conda_manager {
                        settings::CondaManager::Conda => "conda",
                        settings::CondaManager::Mamba => "mamba",
                        settings::CondaManager::Micromamba => "micromamba",
                        settings::CondaManager::Auto => toolchain
                            .environment
                            .manager
                            .as_ref()
                            .and_then(|m| m.executable.file_name())
                            .and_then(|name| name.to_str())
                            .filter(|name| matches!(*name, "conda" | "mamba" | "micromamba"))
                            .unwrap_or("conda"),
                    };

                    // Activate micromamba shell in the child shell
                    // [required for micromamba]
                    if manager == "micromamba" {
                        match shell {
                            ShellKind::PowerShell | ShellKind::Pwsh => {
                                activation_script.push(format!(r#"(& {manager} shell hook --shell powershell) | Out-String | Invoke-Expression"#));
                            }
                            _ => {
                                let shell_name = micromamba_shell_name(shell);
                                activation_script.push(format!(
                                    r#"eval "$({manager} shell hook --shell {shell_name})""#
                                ));
                            }
                        }
                    }

                    // Only inject `{manager} activate <name>` when we have a
                    // safely-quotable name. Never silently fall back to
                    // `activate base`: a user with miniforge installed but a
                    // local uv/venv project should not have their terminal
                    // hijacked just because we couldn't resolve a name.
                    if let Some(name) = &toolchain.environment.name {
                        if let Some(quoted_name) = shell.try_quote(name) {
                            activation_script.push(format!("{manager} activate {quoted_name}"));
                        } else {
                            log::warn!(
                                "Conda environment name {:?} could not be safely quoted; \
                                 skipping terminal activation",
                                name
                            );
                        }
                    } else {
                        log::warn!("Conda toolchain has no name; skipping terminal activation");
                    }
                }
                Some(
                    PythonEnvironmentKind::Venv
                    | PythonEnvironmentKind::VirtualEnv
                    | PythonEnvironmentKind::Uv
                    | PythonEnvironmentKind::UvWorkspace
                    | PythonEnvironmentKind::Poetry,
                ) => {
                    if let Some(activation_scripts) = &toolchain.activation_scripts {
                        if let Some(activate_script_path) = activation_scripts.get(&shell) {
                            let activate_keyword = shell.activate_keyword();
                            if let Some(quoted) =
                                shell.try_quote(&activate_script_path.to_string_lossy())
                            {
                                activation_script.push(format!("{activate_keyword} {quoted}"));
                            }
                        }
                    }
                }
                Some(PythonEnvironmentKind::Pyenv) => {
                    let Some(manager) = &toolchain.environment.manager else {
                        return vec![];
                    };
                    let version = toolchain.environment.version.as_deref().unwrap_or("system");
                    let pyenv = &manager.executable;
                    let pyenv = pyenv.display();
                    activation_script.extend(match shell {
                        ShellKind::Fish => Some(format!("\"{pyenv}\" shell - fish {version}")),
                        ShellKind::Posix => Some(format!("\"{pyenv}\" shell - sh {version}")),
                        ShellKind::Nushell => Some(format!("^\"{pyenv}\" shell - nu {version}")),
                        ShellKind::PowerShell | ShellKind::Pwsh => None,
                        ShellKind::Csh => None,
                        ShellKind::Tcsh => None,
                        ShellKind::Cmd => None,
                        ShellKind::Rc => None,
                        ShellKind::Xonsh => None,
                        ShellKind::Elvish => None,
                    })
                }
                _ => {}
            }
            activation_script
        })
    }
}

async fn venv_to_toolchain(venv: PythonEnvironment, fs: &dyn Fs) -> Option<Toolchain> {
    let mut name = String::from("Python");
    if let Some(ref version) = venv.version {
        _ = write!(name, " {version}");
    }

    let name_and_kind = match (&venv.name, &venv.kind) {
        (Some(name), Some(kind)) => Some(format!("({name}; {})", python_env_kind_display(kind))),
        (Some(name), None) => Some(format!("({name})")),
        (None, Some(kind)) => Some(format!("({})", python_env_kind_display(kind))),
        (None, None) => None,
    };

    if let Some(nk) = name_and_kind {
        _ = write!(name, " {nk}");
    }

    let mut activation_scripts = HashMap::default();
    match venv.kind {
        Some(
            PythonEnvironmentKind::Venv
            | PythonEnvironmentKind::VirtualEnv
            | PythonEnvironmentKind::Uv
            | PythonEnvironmentKind::UvWorkspace
            | PythonEnvironmentKind::Poetry,
        ) => resolve_venv_activation_scripts(&venv, fs, &mut activation_scripts).await,
        _ => {}
    }
    let data = PythonToolchainData {
        environment: venv,
        activation_scripts: Some(activation_scripts),
    };

    Some(Toolchain {
        name: name.into(),
        path: data
            .environment
            .executable
            .as_ref()?
            .to_str()?
            .to_owned()
            .into(),
        language_name: LanguageName::new_static("Python"),
        as_json: serde_json::to_value(data).ok()?,
    })
}

async fn resolve_venv_activation_scripts(
    venv: &PythonEnvironment,
    fs: &dyn Fs,
    activation_scripts: &mut HashMap<ShellKind, PathBuf>,
) {
    log::debug!("(Python) Resolving activation scripts for venv toolchain {venv:?}");
    if let Some(prefix) = &venv.prefix {
        for (shell_kind, script_name) in &[
            (ShellKind::Posix, "activate"),
            (ShellKind::Rc, "activate"),
            (ShellKind::Csh, "activate.csh"),
            (ShellKind::Tcsh, "activate.csh"),
            (ShellKind::Fish, "activate.fish"),
            (ShellKind::Nushell, "activate.nu"),
            (ShellKind::PowerShell, "activate.ps1"),
            (ShellKind::Pwsh, "activate.ps1"),
            (ShellKind::Cmd, "activate.bat"),
            (ShellKind::Xonsh, "activate.xsh"),
        ] {
            let path = prefix.join(BINARY_DIR).join(script_name);

            log::debug!("Trying path: {}", path.display());

            if fs.is_file(&path).await {
                activation_scripts.insert(*shell_kind, path);
            }
        }
    }
}
