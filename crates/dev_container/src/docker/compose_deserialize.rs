use std::collections::HashMap;

use serde::{Deserialize, Deserializer, de};

use crate::devcontainer_json::MountDefinition;

use super::DockerComposeVolume;

pub(super) fn deserialize_labels<'de, D>(
    deserializer: D,
) -> Result<Option<HashMap<String, String>>, D::Error>
where
    D: Deserializer<'de>,
{
    struct LabelsVisitor;

    impl<'de> de::Visitor<'de> for LabelsVisitor {
        type Value = Option<HashMap<String, String>>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a sequence of strings or a map of string key-value pairs")
        }

        fn visit_seq<A>(self, seq: A) -> Result<Self::Value, A::Error>
        where
            A: de::SeqAccess<'de>,
        {
            let values = Vec::<String>::deserialize(de::value::SeqAccessDeserializer::new(seq))?;

            Ok(Some(
                values
                    .iter()
                    .filter_map(|v| {
                        let (key, value) = v.split_once('=')?;
                        Some((key.to_string(), value.to_string()))
                    })
                    .collect(),
            ))
        }

        fn visit_map<M>(self, map: M) -> Result<Self::Value, M::Error>
        where
            M: de::MapAccess<'de>,
        {
            HashMap::<String, String>::deserialize(de::value::MapAccessDeserializer::new(map))
                .map(|v| Some(v))
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(None)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(None)
        }
    }

    deserializer.deserialize_any(LabelsVisitor)
}

pub(super) fn deserialize_compose_volumes<'de, D>(
    deserializer: D,
) -> Result<Vec<MountDefinition>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum VolumeItem {
        Object(MountDefinition),
        String(String),
    }

    let items = Vec::<VolumeItem>::deserialize(deserializer)?;
    items
        .into_iter()
        .map(|item| match item {
            VolumeItem::Object(mount) => Ok(mount),
            VolumeItem::String(s) => parse_compose_volume_string(&s)
                .ok_or_else(|| de::Error::custom(format!("invalid volume string: {s}"))),
        })
        .collect()
}

/// Parses Docker Compose short volume syntax: `[SOURCE:]TARGET[:MODE]`.
/// A leading drive letter (e.g. `C:`) on the source is treated as part of the
/// path rather than as a source/target separator.
fn parse_compose_volume_string(s: &str) -> Option<MountDefinition> {
    let bytes = s.as_bytes();

    // Find the colon that separates source from target, skipping a possible
    // Windows drive-letter prefix (single ASCII letter followed by `:`).
    let separator_start = if bytes.len() >= 2
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && bytes.get(2).map_or(false, |&b| b == b'/' || b == b'\\')
    {
        // Skip past the drive letter prefix (e.g. "C:\")
        3
    } else {
        0
    };

    if let Some(colon_pos) = s[separator_start..].find(':') {
        let colon_pos = colon_pos + separator_start;
        let source = &s[..colon_pos];

        let rest = &s[colon_pos + 1..];

        // `rest` may itself start with a Windows drive letter, so skip past
        // that before looking for a second colon that would delimit the mode.
        let mode_search_start = if rest.len() >= 2
            && rest.as_bytes()[0].is_ascii_alphabetic()
            && rest.as_bytes()[1] == b':'
        {
            2
        } else {
            0
        };

        let (target, _mode) = if let Some(pos) = rest[mode_search_start..].find(':') {
            let pos = pos + mode_search_start;
            (&rest[..pos], Some(&rest[pos + 1..]))
        } else {
            (rest, None)
        };

        if target.is_empty() {
            return None;
        }

        Some(MountDefinition {
            source: Some(source.to_string()),
            target: target.to_string(),
            mount_type: None,
        })
    } else {
        // No colon at all — anonymous volume with only a target path
        if s.is_empty() {
            return None;
        }
        Some(MountDefinition {
            source: None,
            target: s.to_string(),
            mount_type: None,
        })
    }
}

pub(super) fn deserialize_compose_top_level_volumes<'de, D>(
    deserializer: D,
) -> Result<HashMap<String, DockerComposeVolume>, D::Error>
where
    D: Deserializer<'de>,
{
    let map: HashMap<String, Option<DockerComposeVolume>> = HashMap::deserialize(deserializer)?;
    Ok(map
        .into_iter()
        .map(|(key, value)| (key, value.unwrap_or_default()))
        .collect())
}

pub(super) fn deserialize_nullable_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<Vec<T>>::deserialize(deserializer).map(|opt| opt.unwrap_or_default())
}
