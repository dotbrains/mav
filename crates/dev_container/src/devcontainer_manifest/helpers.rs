use super::*;
use util::normalize_path;

/// Replaces occurrences of `${KEY}` and `$KEY` in `line` with `value`.
/// Bare `$KEY` is only replaced when the character immediately after the key
/// is not a word character (`[A-Za-z0-9_]`), so `$RUBY_VERSION2` is not
/// partially consumed when expanding `$RUBY_VERSION`.
pub(super) fn expand_dockerfile_var(mut line: String, key: &str, value: &str) -> String {
    line = line.replace(&format!("${{{key}}}"), value);
    let pattern = format!("${key}");
    let mut result = String::with_capacity(line.len());
    let mut remaining = line.as_str();
    while let Some(pos) = remaining.find(pattern.as_str()) {
        result.push_str(&remaining[..pos]);
        let after = &remaining[pos + pattern.len()..];
        if after.starts_with(|c: char| c.is_alphanumeric() || c == '_') {
            result.push('$');
            remaining = &remaining[pos + 1..];
        } else {
            result.push_str(value);
            remaining = after;
        }
    }
    result.push_str(remaining);
    result
}

pub(super) fn find_primary_service(
    docker_compose: &DockerComposeResources,
    devcontainer: &DevContainerManifest,
) -> Result<(String, DockerComposeService), DevContainerError> {
    let Some(service_name) = &devcontainer.dev_container().service else {
        return Err(DevContainerError::DevContainerParseFailed);
    };

    match docker_compose.config.services.get(service_name) {
        Some(service) => Ok((service_name.clone(), service.clone())),
        None => Err(DevContainerError::DevContainerParseFailed),
    }
}

/// Resolves a compose service's dockerfile path according to the Docker Compose spec:
/// `dockerfile` is relative to the build `context`, and `context` is relative to
/// the compose file's directory.
pub(super) fn resolve_compose_dockerfile(
    compose_file: &Path,
    context: Option<&str>,
    dockerfile: &str,
) -> Option<PathBuf> {
    let dockerfile = PathBuf::from(dockerfile);
    if dockerfile.is_absolute() {
        return Some(dockerfile);
    }
    let compose_dir = compose_file.parent()?;
    let context_dir = match context {
        Some(ctx) => {
            let ctx = PathBuf::from(ctx);
            if ctx.is_absolute() {
                ctx
            } else {
                normalize_path(&compose_dir.join(ctx))
            }
        }
        None => compose_dir.to_path_buf(),
    };
    Some(context_dir.join(dockerfile))
}

/// Destination folder inside the container where feature content is staged during build.
/// Mirrors the CLI's `FEATURES_CONTAINER_TEMP_DEST_FOLDER`.
pub(super) const FEATURES_CONTAINER_TEMP_DEST_FOLDER: &str = "/tmp/dev-container-features";

/// Escapes regex special characters in a string.
pub(super) fn escape_regex_chars(input: &str) -> String {
    let mut result = String::with_capacity(input.len() * 2);
    for c in input.chars() {
        if ".*+?^${}()|[]\\".contains(c) {
            result.push('\\');
        }
        result.push(c);
    }
    result
}

/// Sanitize a string for use as a Docker Compose project name, matching
/// `@devcontainers/cli`'s `toProjectName` (modern Compose branch): lowercase
/// the input and strip any character outside `[-_a-z0-9]`.
pub(super) fn sanitize_compose_project_name(input: &str) -> String {
    input
        .chars()
        .flat_map(|c| c.to_lowercase())
        .filter(|c| c.is_ascii_digit() || c.is_ascii_lowercase() || *c == '-' || *c == '_')
        .collect()
}

/// Derive the Docker Compose project name, mirroring `getProjectName` in
/// `@devcontainers/cli`'s `src/spec-node/dockerCompose.ts`. Precedence:
///
/// 1. `COMPOSE_PROJECT_NAME` from the local environment.
/// 2. `COMPOSE_PROJECT_NAME` from the workspace `.env` file.
/// 3. The top-level `name:` field of the merged compose config, but only
///    when at least one compose fragment explicitly declared `name:`.
///    Compose injects a default `name: devcontainer` into its merged
///    output whenever no fragment declared one — that default must NOT be
///    treated as a user-provided name, so rule 4 applies instead.
/// 4. Basename of the first compose file's directory, appending
///    `_devcontainer` only when that directory is
///    `<workspace_root>/.devcontainer`.
///
/// The caller is responsible for computing `compose_name_explicitly_declared`
/// by scanning the original compose fragments for a top-level `name:` key
/// (the reference CLI does the same). This keeps the helper a pure function
/// of its inputs.
///
/// All branches pass through `sanitize_compose_project_name` — the CLI's
/// final normalization step.
pub(super) fn derive_project_name(
    local_environment: &HashMap<String, String>,
    workspace_dotenv_contents: Option<&str>,
    compose_config_name: Option<&str>,
    compose_name_explicitly_declared: bool,
    first_compose_file: Option<&Path>,
    workspace_root: &Path,
    workspace_fallback: &str,
) -> String {
    if let Some(env_name) = local_environment.get("COMPOSE_PROJECT_NAME")
        && !env_name.is_empty()
    {
        return sanitize_compose_project_name(env_name);
    }
    if let Some(contents) = workspace_dotenv_contents
        && let Some(dotenv_name) = parse_dotenv_compose_project_name(contents)
        && !dotenv_name.is_empty()
    {
        return sanitize_compose_project_name(&dotenv_name);
    }
    if let Some(name) = compose_config_name
        && !name.is_empty()
        && compose_name_explicitly_declared
    {
        return sanitize_compose_project_name(name);
    }
    let compose_dir = first_compose_file.and_then(Path::parent);
    let canonical_devcontainer_dir = normalize_path(&workspace_root.join(".devcontainer"));
    let raw = match compose_dir {
        Some(dir) if dir == canonical_devcontainer_dir => {
            // Matches the CLI's `configDir/.devcontainer` branch: use the
            // *workspace root's* basename with the `_devcontainer` suffix,
            // NOT the `.devcontainer` dir's basename.
            format!("{workspace_fallback}_devcontainer")
        }
        Some(dir) => dir
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_else(|| workspace_fallback.to_string()),
        None => format!("{workspace_fallback}_devcontainer"),
    };
    sanitize_compose_project_name(&raw)
}

/// Classify an anyhow error from `Fs::load` as "file does not exist" vs a
/// real I/O failure. Used on the `.env` read in `project_name()`, where the
/// CLI's `getProjectName` catches only `ENOENT`/`EISDIR` and rethrows
/// everything else; any other error must propagate so callers can surface
/// the problem instead of silently falling back to a non-canonical project
/// name. (The fragment-rescan loop uses a different, broader swallow —
/// the CLI wraps its fragment read+parse in one try/catch that ignores
/// every failure.)
pub(super) fn is_missing_file_error(err: &anyhow::Error) -> bool {
    err.downcast_ref::<std::io::Error>().is_some_and(|e| {
        matches!(
            e.kind(),
            std::io::ErrorKind::NotFound | std::io::ErrorKind::IsADirectory
        )
    })
}

/// Extract `COMPOSE_PROJECT_NAME` from a `.env` file's contents. Matches
/// the subset of dotenv syntax that `@devcontainers/cli`'s regex parser
/// recognizes: a bare `COMPOSE_PROJECT_NAME=value` line (no `export` prefix,
/// no quoting, no line continuation). Comment lines are skipped.
pub(super) fn parse_dotenv_compose_project_name(contents: &str) -> Option<String> {
    for line in contents.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("COMPOSE_PROJECT_NAME=") {
            return Some(value.trim().to_string());
        }
    }
    None
}

/// Detect whether a compose-file fragment declares a top-level `name:` key.
/// Matches the reference CLI's approach: parse the fragment as YAML and check
/// for a `name` key on the root mapping. This handles all valid styles —
/// block mappings, quoted keys (`"name":`), flow-style root mappings, anchors,
/// etc. On parse failure we fall through (return `false`), matching the CLI's
/// own behavior when fragment parsing errors.
pub(super) fn compose_fragment_declares_name(contents: &str) -> bool {
    let Ok(docs) = yaml_rust2::YamlLoader::load_from_str(contents) else {
        return false;
    };
    let Some(yaml_rust2::Yaml::Hash(h)) = docs.into_iter().next() else {
        return false;
    };
    h.contains_key(&yaml_rust2::Yaml::String("name".to_string()))
}

/// Extracts the short feature ID from a full feature reference string.
///
/// Examples:
/// - `ghcr.io/devcontainers/features/aws-cli:1` → `aws-cli`
/// - `ghcr.io/user/repo/go` → `go`
/// - `ghcr.io/devcontainers/features/rust@sha256:abc` → `rust`
/// - `./myFeature` → `myFeature`
pub(super) fn extract_feature_id(feature_ref: &str) -> &str {
    let without_version = if let Some(at_idx) = feature_ref.rfind('@') {
        &feature_ref[..at_idx]
    } else {
        let last_slash = feature_ref.rfind('/');
        let last_colon = feature_ref.rfind(':');
        match (last_slash, last_colon) {
            (Some(slash), Some(colon)) if colon > slash => &feature_ref[..colon],
            _ => feature_ref,
        }
    };
    match without_version.rfind('/') {
        Some(idx) => &without_version[idx + 1..],
        None => without_version,
    }
}

pub(super) fn is_local_feature_ref(feature_ref: &str) -> bool {
    feature_ref.starts_with("./") || feature_ref.starts_with("../")
}

/// Generates a shell command that looks up a user's passwd entry.
///
/// Mirrors the CLI's `getEntPasswdShellCommand` in `commonUtils.ts`.
/// Tries `getent passwd` first, then falls back to grepping `/etc/passwd`.
pub(super) fn get_ent_passwd_shell_command(user: &str) -> String {
    let escaped_for_shell = user.replace('\\', "\\\\").replace('\'', "\\'");
    let escaped_for_regex = escape_regex_chars(user).replace('\'', "\\'");
    format!(
        " (command -v getent >/dev/null 2>&1 && getent passwd '{shell}' || grep -E '^{re}|^[^:]*:[^:]*:{re}:' /etc/passwd || true)",
        shell = escaped_for_shell,
        re = escaped_for_regex,
    )
}

/// Determines feature installation order, respecting `overrideFeatureInstallOrder`.
///
/// Features listed in the override come first (in the specified order), followed
/// by any remaining features sorted lexicographically by their full reference ID.
pub(super) fn resolve_feature_order<'a>(
    features: &'a HashMap<String, FeatureOptions>,
    override_order: &Option<Vec<String>>,
) -> Vec<(&'a String, &'a FeatureOptions)> {
    if let Some(order) = override_order {
        let mut ordered: Vec<(&'a String, &'a FeatureOptions)> = Vec::new();
        for ordered_id in order {
            if let Some((key, options)) = features.get_key_value(ordered_id) {
                ordered.push((key, options));
            }
        }
        let mut remaining: Vec<_> = features
            .iter()
            .filter(|(id, _)| !order.iter().any(|o| o == *id))
            .collect();
        remaining.sort_by_key(|(id, _)| id.as_str());
        ordered.extend(remaining);
        ordered
    } else {
        let mut entries: Vec<_> = features.iter().collect();
        entries.sort_by_key(|(id, _)| id.as_str());
        entries
    }
}

/// Generates the `devcontainer-features-install.sh` wrapper script for one feature.
///
/// Mirrors the CLI's `getFeatureInstallWrapperScript` in
/// `containerFeaturesConfiguration.ts`.
pub(super) fn generate_install_wrapper(
    feature_ref: &str,
    feature_id: &str,
    env_variables: &str,
) -> Result<String, DevContainerError> {
    let escaped_id = shlex::try_quote(feature_ref).map_err(|e| {
        log::error!("Error escaping feature ref {feature_ref}: {e}");
        DevContainerError::DevContainerParseFailed
    })?;
    let escaped_name = shlex::try_quote(feature_id).map_err(|e| {
        log::error!("Error escaping feature {feature_id}: {e}");
        DevContainerError::DevContainerParseFailed
    })?;
    let options_indented: String = env_variables
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| format!("    {}", l))
        .collect::<Vec<_>>()
        .join("\n");
    let escaped_options = shlex::try_quote(&options_indented).map_err(|e| {
        log::error!("Error escaping options {options_indented}: {e}");
        DevContainerError::DevContainerParseFailed
    })?;

    let script = format!(
        r#"#!/bin/sh
set -e

on_exit () {{
    [ $? -eq 0 ] && exit
    echo 'ERROR: Feature "{escaped_name}" ({escaped_id}) failed to install!'
}}

trap on_exit EXIT

echo ===========================================================================
echo 'Feature       : {escaped_name}'
echo 'Id            : {escaped_id}'
echo 'Options       :'
echo {escaped_options}
echo ===========================================================================

set -a
. ../devcontainer-features.builtin.env
. ./devcontainer-features.env
set +a

chmod +x ./install.sh
./install.sh
"#
    );

    Ok(script)
}

pub(super) fn dockerfile_inject_alias(
    dockerfile_content: &str,
    alias: &str,
    build_target: Option<String>,
) -> String {
    let from_lines: Vec<(usize, &str)> = dockerfile_content
        .lines()
        .enumerate()
        .filter(|(_, line)| line.starts_with("FROM"))
        .collect();

    let target_entry = match &build_target {
        Some(target) => from_lines.iter().rfind(|(_, line)| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            parts.len() >= 3
                && parts
                    .get(parts.len() - 2)
                    .map_or(false, |p| p.eq_ignore_ascii_case("as"))
                && parts
                    .last()
                    .map_or(false, |p| p.eq_ignore_ascii_case(target))
        }),
        None => from_lines.last(),
    };

    let Some(&(line_idx, from_line)) = target_entry else {
        return dockerfile_content.to_string();
    };

    let parts: Vec<&str> = from_line.split_whitespace().collect();
    let has_alias = parts.len() >= 3
        && parts
            .get(parts.len() - 2)
            .map_or(false, |p| p.eq_ignore_ascii_case("as"));

    if has_alias {
        let Some(existing_alias) = parts.last() else {
            return dockerfile_content.to_string();
        };
        format!("{dockerfile_content}\nFROM {existing_alias} AS {alias}")
    } else {
        let lines: Vec<&str> = dockerfile_content.lines().collect();
        let mut result = String::new();
        for (i, line) in lines.iter().enumerate() {
            if i > 0 {
                result.push('\n');
            }
            if i == line_idx {
                result.push_str(&format!("{line} AS {alias}"));
            } else {
                result.push_str(line);
            }
        }
        if dockerfile_content.ends_with('\n') {
            result.push('\n');
        }
        result
    }
}

pub(super) fn image_from_dockerfile(
    dockerfile_contents: String,
    target: &Option<String>,
) -> Option<String> {
    dockerfile_contents
        .lines()
        .filter(|line| line.starts_with("FROM"))
        .rfind(|from_line| match &target {
            Some(target) => {
                let parts = from_line.split(' ').collect::<Vec<&str>>();
                if parts.len() >= 3
                    && parts.get(parts.len() - 2).unwrap_or(&"").to_lowercase() == "as"
                {
                    parts.last().unwrap_or(&"").to_lowercase() == target.to_lowercase()
                } else {
                    false
                }
            }
            None => true,
        })
        .and_then(|from_line| {
            from_line
                .split(' ')
                .collect::<Vec<&str>>()
                .get(1)
                .map(|s| s.to_string())
        })
}

pub(super) fn get_remote_user_from_config(
    docker_config: &DockerInspect,
    devcontainer: &DevContainerManifest,
) -> Result<String, DevContainerError> {
    if let DevContainer {
        remote_user: Some(user),
        ..
    } = &devcontainer.dev_container()
    {
        return Ok(user.clone());
    }
    if let Some(metadata) = &docker_config.config.labels.metadata {
        for metadatum in metadata {
            if let Some(remote_user) = metadatum.get("remoteUser") {
                if let Some(remote_user_str) = remote_user.as_str() {
                    return Ok(remote_user_str.to_string());
                }
            }
        }
    }
    if let Some(image_user) = &docker_config.config.image_user {
        if !image_user.is_empty() {
            return Ok(image_user.to_string());
        }
    }
    Ok("root".to_string())
}

// This should come from spec - see the docs
pub(super) fn get_container_user_from_config(
    docker_config: &DockerInspect,
    devcontainer: &DevContainerManifest,
) -> Result<String, DevContainerError> {
    if let Some(user) = &devcontainer.dev_container().container_user {
        return Ok(user.to_string());
    }
    if let Some(metadata) = &docker_config.config.labels.metadata {
        for metadatum in metadata {
            if let Some(container_user) = metadatum.get("containerUser") {
                if let Some(container_user_str) = container_user.as_str() {
                    return Ok(container_user_str.to_string());
                }
            }
        }
    }
    if let Some(image_user) = &docker_config.config.image_user {
        return Ok(image_user.to_string());
    }

    Ok("root".to_string())
}
