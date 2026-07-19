use regex::Regex;
use schemars::{SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serializer, de};

pub fn auto_indent_using_last_non_empty_line_default() -> bool {
    true
}

pub fn deserialize_regex<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Regex>, D::Error> {
    let source = Option::<String>::deserialize(d)?;
    if let Some(source) = source {
        Ok(Some(regex::Regex::new(&source).map_err(de::Error::custom)?))
    } else {
        Ok(None)
    }
}

pub fn regex_json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    json_schema!({
        "type": "string"
    })
}

pub fn serialize_regex<S>(regex: &Option<Regex>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match regex {
        Some(regex) => serializer.serialize_str(regex.as_str()),
        None => serializer.serialize_none(),
    }
}

pub fn deserialize_regex_vec<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Regex>, D::Error> {
    let sources = Vec::<String>::deserialize(d)?;
    sources
        .into_iter()
        .map(|source| regex::Regex::new(&source))
        .collect::<Result<_, _>>()
        .map_err(de::Error::custom)
}

pub fn regex_vec_json_schema(_: &mut SchemaGenerator) -> schemars::Schema {
    json_schema!({
        "type": "array",
        "items": { "type": "string" }
    })
}
