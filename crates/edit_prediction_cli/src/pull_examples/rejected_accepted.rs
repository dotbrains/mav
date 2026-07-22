use super::*;

fn rejected_examples_from_response<'a>(
    response: &'a SnowflakeStatementResponse,
    column_indices: &'a std::collections::HashMap<String, usize>,
) -> Result<Box<dyn Iterator<Item = Example> + 'a>> {
    if let Some(code) = &response.code {
        if code != SNOWFLAKE_SUCCESS_CODE {
            anyhow::bail!(
                "snowflake sql api returned error code={code} message={}",
                response.message.as_deref().unwrap_or("<no message>")
            );
        }
    }

    let iter = response
        .data
        .iter()
        .enumerate()
        .filter_map(move |(row_index, data_row)| {
            let get_string = |name: &str| -> Option<String> {
                let index = column_indices.get(name).copied()?;
                match data_row.get(index)? {
                    JsonValue::String(s) => Some(s.clone()),
                    JsonValue::Null => None,
                    other => Some(other.to_string()),
                }
            };

            let get_json = |name: &str| -> Option<JsonValue> {
                let index = column_indices.get(name).copied()?;
                let value = data_row.get(index)?;
                if value.is_null() {
                    return None;
                }
                match value {
                    JsonValue::String(s) => serde_json::from_str(s).ok(),
                    other => Some(other.clone()),
                }
            };

            let get_bool = |name: &str| -> Option<bool> {
                let index = column_indices.get(name).copied()?;
                match data_row.get(index)? {
                    JsonValue::Bool(b) => Some(*b),
                    JsonValue::String(s) => s.parse().ok(),
                    _ => None,
                }
            };

            let request_id_str = get_string("request_id");
            let device_id = get_string("device_id");
            let time = get_string("time");
            let input_json = get_json("input");
            let input: Option<Zeta2PromptInput> =
                input_json.clone().and_then(|v| serde_json::from_value(v).ok());
            let prompt = get_string("prompt");
            let output = get_string("output");
            let settled_editable_region = get_string("settled_editable_region");
            let was_shown = get_bool("was_shown");
            let reason = get_string("reason");
            let mav_version = get_string("mav_version");

            match (request_id_str.clone(), device_id.clone(), time.clone(), input, output.clone(), was_shown, reason.clone()) {
                (Some(request_id), Some(device_id), Some(time), Some(input), Some(output), Some(was_shown), Some(reason)) => {
                    Some(build_rejected_example(
                        request_id,
                        device_id,
                        time,
                        input,
                        prompt,
                        output,
                        settled_editable_region,
                        was_shown,
                        reason,
                        mav_version,
                    ))
                }
                _ => {
                    log::warn!(
                        "skipping row {row_index}: missing fields - request_id={:?} device_id={:?} time={:?} input={:?} output={:?} was_shown={:?} reason={:?}",
                        request_id_str.is_some(),
                        device_id.is_some(),
                        time.is_some(),
                        input_json.is_some(),
                        output.is_some(),
                        was_shown.is_some(),
                        reason.is_some()
                    );
                    None
                }
            }
        });

    Ok(Box::new(iter))
}

fn build_rejected_example(
    request_id: String,
    device_id: String,
    time: String,
    input: Zeta2PromptInput,
    prompt: Option<String>,
    output: String,
    settled_editable_region: Option<String>,
    was_shown: bool,
    reason: String,
    mav_version: Option<String>,
) -> Example {
    let rejected_patch = build_output_patch(
        &input.cursor_path,
        input.cursor_excerpt.as_ref(),
        &input.excerpt_ranges.editable_350,
        &output,
    );
    let expected_patch = settled_editable_region
        .as_ref()
        .map(|settled_editable_region| {
            build_output_patch(
                &input.cursor_path,
                input.cursor_excerpt.as_ref(),
                &input.excerpt_ranges.editable_350,
                settled_editable_region,
            )
        });
    let mut example = build_example_from_snowflake(
        request_id,
        device_id,
        time,
        input,
        vec![format!("rejection:{}", reason.to_lowercase())],
        Some(RejectionInfo { reason, was_shown }),
        mav_version,
    );
    example.spec.rejected_patch = Some(rejected_patch.clone());
    if let Some(expected_patch) = expected_patch {
        example.spec.expected_patches = vec![expected_patch];
    }
    example.predictions.push(ExamplePrediction {
        provider: PredictionProvider::default(),
        actual_output: output.clone(),
        actual_patch: Some(rejected_patch),
        actual_cursor: None,
        error: None,
        cumulative_logprob: None,
        avg_logprob: None,
    });
    example.prompt = prompt.map(|prompt| ExamplePrompt {
        input: prompt,
        expected_output: None,
        rejected_output: Some(output),
        prefill: None,
        provider: PredictionProvider::default(),
    });
    example
}

fn accepted_examples_from_response<'a>(
    response: &'a SnowflakeStatementResponse,
    column_indices: &'a std::collections::HashMap<String, usize>,
) -> Result<Box<dyn Iterator<Item = Example> + 'a>> {
    if let Some(code) = &response.code {
        if code != SNOWFLAKE_SUCCESS_CODE {
            anyhow::bail!(
                "snowflake sql api returned error code={code} message={}",
                response.message.as_deref().unwrap_or("<no message>")
            );
        }
    }

    let iter = response
        .data
        .iter()
        .enumerate()
        .filter_map(move |(row_index, data_row)| {
            let get_string = |name: &str| -> Option<String> {
                let index = column_indices.get(name).copied()?;
                match data_row.get(index)? {
                    JsonValue::String(s) => Some(s.clone()),
                    JsonValue::Null => None,
                    other => Some(other.to_string()),
                }
            };

            let get_json = |name: &str| -> Option<JsonValue> {
                let index = column_indices.get(name).copied()?;
                let value = data_row.get(index)?;
                if value.is_null() {
                    return None;
                }
                match value {
                    JsonValue::String(s) => serde_json::from_str(s).ok(),
                    other => Some(other.clone()),
                }
            };

            let request_id_str = get_string("request_id");
            let device_id = get_string("device_id");
            let time = get_string("time");
            let input_json = get_json("input");
            let input: Option<Zeta2PromptInput> =
                input_json.clone().and_then(|v| serde_json::from_value(v).ok());
            let prompt = get_string("prompt");
            let output = get_string("output");
            let settled_editable_region = get_string("settled_editable_region");
            let mav_version = get_string("mav_version");

            match (request_id_str.clone(), device_id.clone(), time.clone(), input, output.clone()) {
                (Some(request_id), Some(device_id), Some(time), Some(input), Some(output)) => {
                    Some(build_accepted_example(
                        request_id,
                        device_id,
                        time,
                        input,
                        prompt,
                        output,
                        settled_editable_region,
                        mav_version,
                    ))
                }
                _ => {
                    log::warn!(
                        "skipping row {row_index}: missing fields - request_id={:?} device_id={:?} time={:?} input={:?} output={:?}",
                        request_id_str.is_some(),
                        device_id.is_some(),
                        time.is_some(),
                        input_json.is_some(),
                        output.is_some(),
                    );
                    None
                }
            }
        });

    Ok(Box::new(iter))
}

fn build_accepted_example(
    request_id: String,
    device_id: String,
    time: String,
    input: Zeta2PromptInput,
    prompt: Option<String>,
    output: String,
    settled_editable_region: Option<String>,
    mav_version: Option<String>,
) -> Example {
    let accepted_patch = build_output_patch(
        &input.cursor_path,
        input.cursor_excerpt.as_ref(),
        &input.excerpt_ranges.editable_350,
        &output,
    );
    let expected_patch = settled_editable_region
        .as_ref()
        .map(|settled_editable_region| {
            build_output_patch(
                &input.cursor_path,
                input.cursor_excerpt.as_ref(),
                &input.excerpt_ranges.editable_350,
                settled_editable_region,
            )
        });
    let mut example = build_example_from_snowflake(
        request_id,
        device_id,
        time,
        input,
        vec!["accepted".to_string()],
        None,
        mav_version,
    );
    if let Some(expected_patch) = expected_patch {
        example.spec.expected_patches = vec![expected_patch];
    }
    example.predictions.push(ExamplePrediction {
        provider: PredictionProvider::default(),
        actual_output: output.clone(),
        actual_patch: Some(accepted_patch),
        actual_cursor: None, // todo: why no cursor?
        error: None,
        cumulative_logprob: None,
        avg_logprob: None,
    });
    example.prompt = prompt.map(|prompt| ExamplePrompt {
        input: prompt,
        expected_output: Some(output),
        rejected_output: None,
        prefill: None,
        provider: PredictionProvider::default(),
    });
    example
}
