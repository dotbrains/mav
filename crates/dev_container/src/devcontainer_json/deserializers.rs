use crate::devcontainer_api::DevContainerError;

use super::{DevContainer, MountDefinition};

pub(crate) fn deserialize_devcontainer_json_to_value(
    json: &str,
) -> Result<serde_json_lenient::Value, DevContainerError> {
    serde_json_lenient::from_str(json).map_err(|e| {
        log::error!("Unable to deserialize json values: {e}");
        DevContainerError::DevContainerParseFailed
    })
}

pub(crate) fn deserialize_devcontainer_json_from_value(
    json: serde_json_lenient::Value,
) -> Result<DevContainer, DevContainerError> {
    serde_json_lenient::from_value(json).map_err(|e| {
        log::error!("Unable to deserialize devcontainer from json values: {e}");
        DevContainerError::DevContainerParseFailed
    })
}

pub(crate) fn deserialize_devcontainer_json(json: &str) -> Result<DevContainer, DevContainerError> {
    deserialize_devcontainer_json_to_value(json).and_then(deserialize_devcontainer_json_from_value)
}

pub(super) fn deserialize_mount_definition<'de, D>(
    deserializer: D,
) -> Result<Option<MountDefinition>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    use serde::de::Error;

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum MountItem {
        Object(MountDefinition),
        String(String),
    }

    let item = MountItem::deserialize(deserializer)?;

    let mount = match item {
        MountItem::Object(mount) => mount,
        MountItem::String(s) => {
            let mut source = None;
            let mut target = None;
            let mut mount_type = None;

            for part in s.split(',') {
                let part = part.trim();
                if let Some((key, value)) = part.split_once('=') {
                    match key.trim() {
                        "source" => source = Some(value.trim().to_string()),
                        "target" => target = Some(value.trim().to_string()),
                        "type" => mount_type = Some(value.trim().to_string()),
                        _ => {} // Ignore unknown keys
                    }
                }
            }

            let target = target
                .ok_or_else(|| D::Error::custom(format!("mount string missing 'target': {}", s)))?;

            MountDefinition {
                source,
                target,
                mount_type,
            }
        }
    };

    Ok(Some(mount))
}

pub(super) fn deserialize_mount_definitions<'de, D>(
    deserializer: D,
) -> Result<Option<Vec<MountDefinition>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    use serde::de::Error;

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum MountItem {
        Object(MountDefinition),
        String(String),
    }

    let items = Vec::<MountItem>::deserialize(deserializer)?;
    let mut mounts = Vec::new();

    for item in items {
        match item {
            MountItem::Object(mount) => mounts.push(mount),
            MountItem::String(s) => {
                let mut source = None;
                let mut target = None;
                let mut mount_type = None;

                for part in s.split(',') {
                    let part = part.trim();
                    if let Some((key, value)) = part.split_once('=') {
                        match key.trim() {
                            "source" => source = Some(value.trim().to_string()),
                            "target" => target = Some(value.trim().to_string()),
                            "type" => mount_type = Some(value.trim().to_string()),
                            _ => {} // Ignore unknown keys
                        }
                    }
                }

                let target = target.ok_or_else(|| {
                    D::Error::custom(format!("mount string missing 'target': {}", s))
                })?;

                mounts.push(MountDefinition {
                    source,
                    target,
                    mount_type,
                });
            }
        }
    }

    Ok(Some(mounts))
}

pub(super) fn deserialize_app_port<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrInt {
        String(String),
        Int(u32),
    }

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum AppPort {
        Array(Vec<StringOrInt>),
        Single(StringOrInt),
    }

    fn normalize_port(value: StringOrInt) -> String {
        match value {
            StringOrInt::String(s) => {
                if s.contains(':') {
                    s
                } else {
                    format!("{s}:{s}")
                }
            }
            StringOrInt::Int(n) => format!("{n}:{n}"),
        }
    }

    match AppPort::deserialize(deserializer)? {
        AppPort::Single(value) => Ok(vec![normalize_port(value)]),
        AppPort::Array(values) => Ok(values.into_iter().map(normalize_port).collect()),
    }
}

pub(super) fn deserialize_string_or_array<'de, D>(
    deserializer: D,
) -> Result<Option<Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrArray {
        String(String),
        Array(Vec<String>),
    }

    match StringOrArray::deserialize(deserializer)? {
        StringOrArray::String(s) => Ok(Some(vec![s])),
        StringOrArray::Array(b) => Ok(Some(b)),
    }
}
