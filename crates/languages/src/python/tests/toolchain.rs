use super::*;

async fn test_conda_activation_script_injection(cx: &mut TestAppContext) {
    use language::{LanguageName, Toolchain, ToolchainLister};
    use settings::{CondaManager, VenvSettings};
    use task::ShellKind;

    use crate::python::PythonToolchainProvider;

    cx.executor().allow_parking();

    cx.update(|cx| {
        let test_settings = SettingsStore::test(cx);
        cx.set_global(test_settings);
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |s| {
                s.terminal
                    .get_or_insert_with(Default::default)
                    .project
                    .detect_venv = Some(VenvSettings::On {
                    activate_script: None,
                    venv_name: None,
                    directories: None,
                    conda_manager: Some(CondaManager::Conda),
                });
            });
        });
    });

    let fs = project::FakeFs::new(cx.executor());
    let provider = PythonToolchainProvider::new(fs);
    let malicious_name = "foo; rm -rf /";

    let manager_executable = std::env::current_exe().unwrap();

    let data = serde_json::json!({
        "name": malicious_name,
        "kind": "Conda",
        "executable": "/tmp/conda/bin/python",
        "version": serde_json::Value::Null,
        "prefix": serde_json::Value::Null,
        "arch": serde_json::Value::Null,
        "displayName": serde_json::Value::Null,
        "project": serde_json::Value::Null,
        "symlinks": serde_json::Value::Null,
        "manager": {
            "executable": manager_executable,
            "version": serde_json::Value::Null,
            "tool": "Conda",
        },
    });

    let toolchain = Toolchain {
        name: "test".into(),
        path: "/tmp/conda".into(),
        language_name: LanguageName::new_static("Python"),
        as_json: data,
    };

    let script = cx
        .update(|cx| provider.activation_script(&toolchain, ShellKind::Posix, cx))
        .await;

    assert!(
        script
            .iter()
            .any(|s| s.contains("conda activate 'foo; rm -rf /'")),
        "Script should contain quoted malicious name, actual: {:?}",
        script
    );
}

#[gpui::test]
async fn test_conda_activation_skips_when_name_missing(cx: &mut TestAppContext) {
    use language::{LanguageName, Toolchain, ToolchainLister};
    use settings::{CondaManager, VenvSettings};
    use task::ShellKind;

    use crate::python::PythonToolchainProvider;

    cx.executor().allow_parking();

    cx.update(|cx| {
        let test_settings = SettingsStore::test(cx);
        cx.set_global(test_settings);
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |s| {
                s.terminal
                    .get_or_insert_with(Default::default)
                    .project
                    .detect_venv = Some(VenvSettings::On {
                    activate_script: None,
                    venv_name: None,
                    directories: None,
                    conda_manager: Some(CondaManager::Conda),
                });
            });
        });
    });

    let fs = project::FakeFs::new(cx.executor());
    let provider = PythonToolchainProvider::new(fs);
    let manager_executable = std::env::current_exe().unwrap();

    let data = serde_json::json!({
        "name": serde_json::Value::Null,
        "kind": "Conda",
        "executable": "/tmp/conda/bin/python",
        "version": serde_json::Value::Null,
        "prefix": serde_json::Value::Null,
        "arch": serde_json::Value::Null,
        "displayName": serde_json::Value::Null,
        "project": serde_json::Value::Null,
        "symlinks": serde_json::Value::Null,
        "manager": {
            "executable": manager_executable,
            "version": serde_json::Value::Null,
            "tool": "Conda",
        },
    });

    let toolchain = Toolchain {
        name: "test".into(),
        path: "/tmp/conda".into(),
        language_name: LanguageName::new_static("Python"),
        as_json: data,
    };

    let script = cx
        .update(|cx| provider.activation_script(&toolchain, ShellKind::Posix, cx))
        .await;

    assert!(
        script.is_empty(),
        "Nameless conda toolchains must not fall back to `conda activate base`, actual: {:?}",
        script
    );
}

#[gpui::test]
async fn test_conda_activation_skips_unquotable_name(cx: &mut TestAppContext) {
    use language::{LanguageName, Toolchain, ToolchainLister};
    use settings::{CondaManager, VenvSettings};
    use task::ShellKind;

    use crate::python::PythonToolchainProvider;

    cx.executor().allow_parking();

    cx.update(|cx| {
        let test_settings = SettingsStore::test(cx);
        cx.set_global(test_settings);
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |s| {
                s.terminal
                    .get_or_insert_with(Default::default)
                    .project
                    .detect_venv = Some(VenvSettings::On {
                    activate_script: None,
                    venv_name: None,
                    directories: None,
                    conda_manager: Some(CondaManager::Conda),
                });
            });
        });
    });

    let fs = project::FakeFs::new(cx.executor());
    let provider = PythonToolchainProvider::new(fs);
    // shlex::try_quote rejects strings containing a NUL byte, so this name
    // is guaranteed to fail the Posix quoting path.
    let unquotable_name = "foo\0bar";
    let manager_executable = std::env::current_exe().unwrap();

    let data = serde_json::json!({
        "name": unquotable_name,
        "kind": "Conda",
        "executable": "/tmp/conda/bin/python",
        "version": serde_json::Value::Null,
        "prefix": serde_json::Value::Null,
        "arch": serde_json::Value::Null,
        "displayName": serde_json::Value::Null,
        "project": serde_json::Value::Null,
        "symlinks": serde_json::Value::Null,
        "manager": {
            "executable": manager_executable,
            "version": serde_json::Value::Null,
            "tool": "Conda",
        },
    });

    let toolchain = Toolchain {
        name: "test".into(),
        path: "/tmp/conda".into(),
        language_name: LanguageName::new_static("Python"),
        as_json: data,
    };

    let script = cx
        .update(|cx| provider.activation_script(&toolchain, ShellKind::Posix, cx))
        .await;

    assert!(
        !script.iter().any(|s| s.contains("conda activate")),
        "Unquotable conda env names must not emit any `conda activate` line, actual: {:?}",
        script
    );
}
