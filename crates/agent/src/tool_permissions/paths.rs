use super::*;

/// Normalizes a path by collapsing `.` and `..` segments without touching the filesystem.
pub fn normalize_path(raw: &str) -> String {
    let is_absolute = Path::new(raw).has_root();
    let mut components: Vec<&str> = Vec::new();
    for component in Path::new(raw).components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if components.last() == Some(&"..") {
                    components.push("..");
                } else if !components.is_empty() {
                    components.pop();
                } else if !is_absolute {
                    components.push("..");
                }
            }
            Component::Normal(segment) => {
                if let Some(s) = segment.to_str() {
                    components.push(s);
                }
            }
            Component::RootDir | Component::Prefix(_) => {}
        }
    }
    let joined = components.join("/");
    if is_absolute {
        format!("/{joined}")
    } else {
        joined
    }
}

/// Decides permission by checking both the raw input path and a simplified/canonicalized
/// version. Returns the most restrictive decision (Deny > Confirm > Allow).
pub fn decide_permission_for_paths(
    tool_name: &str,
    raw_paths: &[String],
    settings: &AgentSettings,
) -> ToolPermissionDecision {
    let raw_inputs: Vec<String> = raw_paths.to_vec();
    let raw_decision = decide_permission_from_settings(tool_name, &raw_inputs, settings);

    let normalized: Vec<String> = raw_paths.iter().map(|p| normalize_path(p)).collect();
    let any_changed = raw_paths
        .iter()
        .zip(&normalized)
        .any(|(raw, norm)| raw != norm);
    if !any_changed {
        return raw_decision;
    }

    let normalized_decision = decide_permission_from_settings(tool_name, &normalized, settings);

    most_restrictive(raw_decision, normalized_decision)
}

pub fn decide_permission_for_path(
    tool_name: &str,
    raw_path: &str,
    settings: &AgentSettings,
) -> ToolPermissionDecision {
    decide_permission_for_paths(tool_name, &[raw_path.to_string()], settings)
}

pub fn most_restrictive(
    a: ToolPermissionDecision,
    b: ToolPermissionDecision,
) -> ToolPermissionDecision {
    match (&a, &b) {
        (ToolPermissionDecision::Deny(_), _) => a,
        (_, ToolPermissionDecision::Deny(_)) => b,
        (ToolPermissionDecision::Confirm, _) | (_, ToolPermissionDecision::Confirm) => {
            ToolPermissionDecision::Confirm
        }
        _ => a,
    }
}
