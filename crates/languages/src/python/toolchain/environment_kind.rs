use super::*;

fn is_python_env_global(k: &PythonEnvironmentKind) -> bool {
    matches!(
        k,
        PythonEnvironmentKind::Homebrew
            | PythonEnvironmentKind::Pyenv
            | PythonEnvironmentKind::GlobalPaths
            | PythonEnvironmentKind::MacPythonOrg
            | PythonEnvironmentKind::MacCommandLineTools
            | PythonEnvironmentKind::LinuxGlobal
            | PythonEnvironmentKind::MacXCode
            | PythonEnvironmentKind::WindowsStore
            | PythonEnvironmentKind::WindowsRegistry
    )
}

fn python_env_kind_display(k: &PythonEnvironmentKind) -> &'static str {
    match k {
        PythonEnvironmentKind::Conda => "Conda",
        PythonEnvironmentKind::Pixi => "pixi",
        PythonEnvironmentKind::Homebrew => "Homebrew",
        PythonEnvironmentKind::Pyenv => "global (Pyenv)",
        PythonEnvironmentKind::GlobalPaths => "global",
        PythonEnvironmentKind::PyenvVirtualEnv => "Pyenv",
        PythonEnvironmentKind::Pipenv => "Pipenv",
        PythonEnvironmentKind::Poetry => "Poetry",
        PythonEnvironmentKind::MacPythonOrg => "global (Python.org)",
        PythonEnvironmentKind::MacCommandLineTools => "global (Command Line Tools for Xcode)",
        PythonEnvironmentKind::LinuxGlobal => "global",
        PythonEnvironmentKind::MacXCode => "global (Xcode)",
        PythonEnvironmentKind::Venv => "venv",
        PythonEnvironmentKind::VirtualEnv => "virtualenv",
        PythonEnvironmentKind::VirtualEnvWrapper => "virtualenvwrapper",
        PythonEnvironmentKind::WinPython => "WinPython",
        PythonEnvironmentKind::WindowsStore => "global (Windows Store)",
        PythonEnvironmentKind::WindowsRegistry => "global (Windows Registry)",
        PythonEnvironmentKind::Uv => "uv",
        PythonEnvironmentKind::UvWorkspace => "uv (Workspace)",
    }
}
