use std::collections::HashMap;

use serde::{Deserialize, Deserializer};

use super::DockerConfigLabels;

pub(super) fn deserialize_nullable_labels<'de, D>(
    deserializer: D,
) -> Result<DockerConfigLabels, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<DockerConfigLabels>::deserialize(deserializer).map(|opt| opt.unwrap_or_default())
}

pub(super) fn deserialize_metadata<'de, D>(
    deserializer: D,
) -> Result<Option<Vec<HashMap<String, serde_json_lenient::Value>>>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: Option<String> = Option::deserialize(deserializer)?;
    match s {
        Some(json_string) => {
            // The devcontainer metadata label can be either a JSON array (e.g. from
            // image-based devcontainers) or a single JSON object (e.g. from
            // docker-compose-based devcontainers created by the devcontainer CLI).
            // Handle both formats.
            let parsed: Vec<HashMap<String, serde_json_lenient::Value>> =
                serde_json_lenient::from_str(&json_string).or_else(|_| {
                    let single: HashMap<String, serde_json_lenient::Value> =
                        serde_json_lenient::from_str(&json_string).map_err(|e| {
                            log::error!("Error deserializing metadata: {e}");
                            serde::de::Error::custom(e)
                        })?;
                    Ok(vec![single])
                })?;
            Ok(Some(parsed))
        }
        None => Ok(None),
    }
}
