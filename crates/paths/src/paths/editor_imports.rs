use super::*;

/// Returns candidate paths for the vscode user settings file
pub fn vscode_settings_file_paths() -> Vec<PathBuf> {
    let mut paths = vscode_user_data_paths();
    for path in paths.iter_mut() {
        path.push("User/settings.json");
    }
    paths
}

/// Returns candidate paths for the cursor user settings file
pub fn cursor_settings_file_paths() -> Vec<PathBuf> {
    let mut paths = cursor_user_data_paths();
    for path in paths.iter_mut() {
        path.push("User/settings.json");
    }
    paths
}

fn vscode_user_data_paths() -> Vec<PathBuf> {
    // https://github.com/microsoft/vscode/blob/23e7148cdb6d8a27f0109ff77e5b1e019f8da051/src/vs/platform/environment/node/userDataPath.ts#L45
    const VSCODE_PRODUCT_NAMES: &[&str] = &[
        "Code",
        "Code - Insiders",
        "Code - OSS",
        "VSCodium",
        "VSCodium - Insiders",
        "Code Dev",
        "Code - OSS Dev",
        "code-oss-dev",
    ];
    let mut paths = Vec::new();
    if let Ok(portable_path) = env::var("VSCODE_PORTABLE") {
        paths.push(Path::new(&portable_path).join("user-data"));
    }
    if let Ok(vscode_appdata) = env::var("VSCODE_APPDATA") {
        for product_name in VSCODE_PRODUCT_NAMES {
            paths.push(Path::new(&vscode_appdata).join(product_name));
        }
    }
    for product_name in VSCODE_PRODUCT_NAMES {
        add_vscode_user_data_paths(&mut paths, product_name);
    }
    paths
}

fn cursor_user_data_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    add_vscode_user_data_paths(&mut paths, "Cursor");
    paths
}

fn add_vscode_user_data_paths(paths: &mut Vec<PathBuf>, product_name: &str) {
    if cfg!(target_os = "macos") {
        paths.push(
            home_dir()
                .join("Library/Application Support")
                .join(product_name),
        );
    } else if cfg!(target_os = "windows") {
        if let Some(data_local_dir) = dirs::data_local_dir() {
            paths.push(data_local_dir.join(product_name));
        }
        if let Some(data_dir) = dirs::data_dir() {
            paths.push(data_dir.join(product_name));
        }
    } else {
        paths.push(
            dirs::config_dir()
                .unwrap_or(home_dir().join(".config"))
                .join(product_name),
        );
    }
}
