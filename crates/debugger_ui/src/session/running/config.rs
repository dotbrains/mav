use super::*;

impl RunningState {
    pub(crate) fn substitute_variables_in_config(
        config: &mut serde_json::Value,
        context: &TaskContext,
    ) {
        match config {
            serde_json::Value::Object(obj) => {
                obj.values_mut()
                    .for_each(|value| Self::substitute_variables_in_config(value, context));
            }
            serde_json::Value::Array(array) => {
                array
                    .iter_mut()
                    .for_each(|value| Self::substitute_variables_in_config(value, context));
            }
            serde_json::Value::String(s) => {
                // Some built-in mav tasks wrap their arguments in quotes as they might contain spaces.
                if s.starts_with("\"$MAV_") && s.ends_with('"') {
                    *s = s[1..s.len() - 1].to_string();
                }
                if let Some(substituted) = substitute_variables_in_str(s, context) {
                    *s = substituted;
                }
            }
            _ => {}
        }
    }

    pub(crate) fn contains_substring(config: &serde_json::Value, substring: &str) -> bool {
        match config {
            serde_json::Value::Object(obj) => obj
                .values()
                .any(|value| Self::contains_substring(value, substring)),
            serde_json::Value::Array(array) => array
                .iter()
                .any(|value| Self::contains_substring(value, substring)),
            serde_json::Value::String(s) => s.contains(substring),
            _ => false,
        }
    }

    pub(crate) fn substitute_process_id_in_config(config: &mut serde_json::Value, process_id: i32) {
        match config {
            serde_json::Value::Object(obj) => {
                obj.values_mut().for_each(|value| {
                    Self::substitute_process_id_in_config(value, process_id);
                });
            }
            serde_json::Value::Array(array) => {
                array.iter_mut().for_each(|value| {
                    Self::substitute_process_id_in_config(value, process_id);
                });
            }
            serde_json::Value::String(s) => {
                if s.contains(PROCESS_ID_PLACEHOLDER.as_str()) {
                    *s = s.replace(PROCESS_ID_PLACEHOLDER.as_str(), &process_id.to_string());
                }
            }
            _ => {}
        }
    }

    pub(crate) fn relativize_paths(
        key: Option<&str>,
        config: &mut serde_json::Value,
        context: &TaskContext,
    ) {
        match config {
            serde_json::Value::Object(obj) => {
                obj.iter_mut()
                    .for_each(|(key, value)| Self::relativize_paths(Some(key), value, context));
            }
            serde_json::Value::Array(array) => {
                array
                    .iter_mut()
                    .for_each(|value| Self::relativize_paths(None, value, context));
            }
            serde_json::Value::String(s) if key == Some("program") || key == Some("cwd") => {
                // Some built-in mav tasks wrap their arguments in quotes as they might contain spaces.
                if s.starts_with("\"$MAV_") && s.ends_with('"') {
                    *s = s[1..s.len() - 1].to_string();
                }
                resolve_path(s);

                if let Some(substituted) = substitute_variables_in_str(s, context) {
                    *s = substituted;
                }
            }
            _ => {}
        }
    }
}
