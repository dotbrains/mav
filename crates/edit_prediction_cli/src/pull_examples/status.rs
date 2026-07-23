use super::*;

pub(crate) fn is_timeout_response(response: &SnowflakeStatementResponse) -> bool {
    response.code.as_deref() == Some(SNOWFLAKE_TIMEOUT_CODE)
        && response
            .message
            .as_deref()
            .map(|message| message.to_ascii_lowercase().contains("timeout"))
            .unwrap_or(false)
}

pub(crate) fn is_snowflake_timeout_error(error: &anyhow::Error) -> bool {
    error
        .chain()
        .any(|cause| cause.to_string().contains(SNOWFLAKE_TIMEOUT_CODE))
}

pub(crate) fn last_continuation_timestamp_from_response(
    response: &SnowflakeStatementResponse,
    column_indices: &HashMap<String, usize>,
) -> Option<String> {
    let continuation_time_index = column_indices.get("continuation_time").copied()?;
    response
        .data
        .iter()
        .rev()
        .find_map(|row| match row.get(continuation_time_index)? {
            JsonValue::String(value) => Some(value.clone()),
            JsonValue::Null => None,
            other => Some(other.to_string()),
        })
}

pub(crate) fn get_column_indices(
    meta: &Option<SnowflakeResultSetMetaData>,
    names: &[&str],
) -> HashMap<String, usize> {
    let mut indices = HashMap::new();
    if let Some(meta) = meta {
        for (index, col) in meta.row_type.iter().enumerate() {
            for &name in names {
                if col.name.eq_ignore_ascii_case(name) {
                    indices.insert(name.to_string(), index);
                }
            }
        }
    }
    indices
}
