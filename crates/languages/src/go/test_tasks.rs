use super::*;

pub(super) fn json_string_array(value: &serde_json::Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn go_test_task_template(arg: &serde_json::Value) -> Option<task::TaskTemplate> {
    let tests = json_string_array(arg, "Tests");
    let benchmarks = json_string_array(arg, "Benchmarks");
    if tests.is_empty() && benchmarks.is_empty() {
        return None;
    }

    let mut go_args = vec!["test".to_string(), "-test.fullpath=true".to_string()];

    if tests.is_empty() {
        go_args.push("-benchmem".to_string());
        go_args.push("-run=^$".to_string());
    } else {
        go_args.push("-timeout".to_string());
        go_args.push("30s".to_string());
        go_args.push("-run".to_string());
        if tests.len() == 1 {
            go_args.push(format!("^{}$", tests[0]));
        } else {
            go_args.push(format!("^({})$", tests.join("|")));
        }
    }

    if !benchmarks.is_empty() {
        go_args.push("-bench".to_string());
        if benchmarks.len() == 1 {
            go_args.push(format!("^{}$", benchmarks[0]));
        } else {
            go_args.push(format!("^({})$", benchmarks.join("|")));
        }
    }

    go_args.push(".".to_string());

    let label = if !tests.is_empty() {
        format!("go test {}", tests.join(", "))
    } else {
        format!("go bench {}", benchmarks.join(", "))
    };

    let cwd = arg
        .get("URI")
        .and_then(|v| v.as_str())
        .and_then(|uri| uri.strip_prefix("file://"))
        .and_then(|path| std::path::Path::new(path).parent())
        .map(|p| p.to_string_lossy().into_owned());

    Some(task::TaskTemplate {
        label,
        command: "go".to_string(),
        args: go_args,
        cwd,
        ..task::TaskTemplate::default()
    })
}

pub(super) fn parse_version_output(output: &Output) -> Result<&str> {
    let version_stdout =
        str::from_utf8(&output.stdout).context("version command produced invalid utf8 output")?;

    let version = VERSION_REGEX
        .find(version_stdout)
        .with_context(|| format!("failed to parse version output '{version_stdout}'"))?
        .as_str();

    Ok(version)
}

pub(super) async fn get_cached_server_binary(container_dir: &Path) -> Option<LanguageServerBinary> {
    maybe!(async {
        let mut last_binary_path = None;
        let mut entries = fs::read_dir(container_dir).await?;
        while let Some(entry) = entries.next().await {
            let entry = entry?;
            if entry.file_type().await?.is_file()
                && entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.starts_with("gopls_"))
            {
                last_binary_path = Some(entry.path());
            }
        }

        let path = last_binary_path.context("no cached binary")?;
        anyhow::Ok(LanguageServerBinary {
            path,
            arguments: server_binary_arguments(),
            env: None,
        })
    })
    .await
    .log_err()
}
