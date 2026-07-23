use super::*;

pub(crate) fn requested_examples_from_response<'a>(
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
            let mav_version = get_string("mav_version");

            match (request_id_str.clone(), device_id.clone(), time.clone(), input) {
                (Some(request_id), Some(device_id), Some(time), Some(input)) => {
                    Some(build_example_from_snowflake(
                        request_id,
                        device_id,
                        time,
                        input,
                        vec!["requested".to_string()],
                        None,
                        mav_version,
                    ))
                }
                _ => {
                    log::warn!(
                        "skipping row {row_index}: missing fields - request_id={:?} device_id={:?} time={:?} input={:?}",
                        request_id_str.is_some(),
                        device_id.is_some(),
                        time.is_some(),
                        input_json.is_some(),
                    );
                    None
                }
            }
        });

    Ok(Box::new(iter))
}

pub(crate) fn settled_examples_from_response<'a>(
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
            let get_value = |name: &str| -> Option<JsonValue> {
                let index = column_indices.get(name).copied()?;
                let value = data_row.get(index)?;
                if value.is_null() {
                    None
                } else {
                    Some(value.clone())
                }
            };

            let get_string = |name: &str| -> Option<String> {
                match get_value(name)? {
                    JsonValue::String(s) => Some(s),
                    other => Some(other.to_string()),
                }
            };

            let parse_json_value = |raw: Option<&JsonValue>| -> Option<JsonValue> {
                let value = raw?;
                match value {
                    JsonValue::String(s) => serde_json::from_str::<JsonValue>(s).ok(),
                    other => Some(other.clone()),
                }
            };

            let request_id_str = get_string("request_id");
            let device_id = get_string("device_id");
            let time = get_string("time");
            let input_raw = get_value("input");
            let input_json = parse_json_value(input_raw.as_ref());
            let input: Option<Zeta2PromptInput> = input_json
                .as_ref()
                .and_then(|parsed| serde_json::from_value(parsed.clone()).ok());
            let requested_output = get_string("requested_output");
            let settled_editable_region = get_string("settled_editable_region");
            let requested_format =
                get_string("requested_format").and_then(|s| ZetaFormat::parse(&s).ok());
            let mav_version = get_string("mav_version");

            match (
                request_id_str.clone(),
                device_id.clone(),
                time.clone(),
                input.clone(),
                requested_output.clone(),
                settled_editable_region.clone(),
                requested_format,
            ) {
                (
                    Some(request_id),
                    Some(device_id),
                    Some(time),
                    Some(input),
                    Some(requested_output),
                    Some(settled_editable_region),
                    Some(requested_format),
                ) => Some(build_settled_example(
                    request_id,
                    device_id,
                    time,
                    input,
                    requested_output,
                    settled_editable_region,
                    requested_format,
                    mav_version,
                )),
                _ => {
                    let mut missing_fields = Vec::new();

                    if request_id_str.is_none() {
                        missing_fields.push("request_id");
                    }
                    if device_id.is_none() {
                        missing_fields.push("device_id");
                    }
                    if time.is_none() {
                        missing_fields.push("time");
                    }
                    if input_raw.is_none() || input_json.is_none() || input.is_none() {
                        missing_fields.push("input");
                    }
                    if requested_output.is_none() {
                        missing_fields.push("requested_output");
                    }
                    if settled_editable_region.is_none() {
                        missing_fields.push("settled_editable_region");
                    }
                    if requested_format.is_none() {
                        missing_fields.push("requested_format");
                    }

                    log::warn!(
                        "skipping settled row {row_index}: [{}]",
                        missing_fields.join(", "),
                    );
                    None
                }
            }
        });

    Ok(Box::new(iter))
}

pub(crate) fn captured_examples_from_response<'a>(
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
            let get_value = |name: &str| -> Option<JsonValue> {
                let index = column_indices.get(name).copied()?;
                let value = data_row.get(index)?;
                if value.is_null() {
                    None
                } else {
                    Some(value.clone())
                }
            };

            let get_string = |name: &str| -> Option<String> {
                match get_value(name)? {
                    JsonValue::String(s) => Some(s),
                    other => Some(other.to_string()),
                }
            };

            let parse_json_value = |raw: Option<&JsonValue>| -> Option<JsonValue> {
                let value = raw?;
                match value {
                    JsonValue::String(s) => serde_json::from_str::<JsonValue>(s).ok(),
                    other => Some(other.clone()),
                }
            };

            let request_id = get_string("request_id");
            let device_id = get_string("device_id");
            let time = get_string("time");
            let input_raw = get_value("input");
            let input_json = parse_json_value(input_raw.as_ref());
            let input: Option<Zeta2PromptInput> = input_json
                .as_ref()
                .and_then(|parsed| serde_json::from_value(parsed.clone()).ok());
            let example_raw = get_value("example");
            let example_json = parse_json_value(example_raw.as_ref());
            let example_spec: Option<ExampleSpec> = example_json.as_ref().and_then(|parsed| {
                serde_json::from_value(parsed.clone())
                    .or_else(|_| {
                        parsed
                            .as_str()
                            .and_then(|markdown| ExampleSpec::from_markdown(markdown).ok())
                            .ok_or_else(|| {
                                serde_json::Error::io(std::io::Error::other("not markdown"))
                            })
                    })
                    .ok()
            });
            let has_example_spec = example_spec.is_some();
            let settled_editable_region = get_string("settled_editable_region");
            let mav_version = get_string("mav_version");

            match (
                request_id.clone(),
                device_id.clone(),
                time.clone(),
                input.clone(),
                example_spec,
                settled_editable_region.clone(),
            ) {
                (
                    Some(request_id),
                    Some(device_id),
                    Some(time),
                    Some(input),
                    Some(example_spec),
                    Some(settled_editable_region),
                ) => Some(build_captured_example(
                    request_id,
                    device_id,
                    time,
                    input,
                    example_spec,
                    settled_editable_region,
                    mav_version,
                )),
                _ => {
                    let mut missing_fields = Vec::new();

                    if request_id.is_none() {
                        missing_fields.push("request_id");
                    }
                    if device_id.is_none() {
                        missing_fields.push("device_id");
                    }
                    if time.is_none() {
                        missing_fields.push("time");
                    }
                    if input_raw.is_none() || input_json.is_none() || input.is_none() {
                        missing_fields.push("input");
                    }
                    if example_raw.is_none() || !has_example_spec {
                        missing_fields.push("example");
                    }
                    if settled_editable_region.is_none() {
                        missing_fields.push("settled_editable_region");
                    }

                    log::warn!(
                        "skipping captured row {row_index}: [{}]",
                        missing_fields.join(", "),
                    );
                    None
                }
            }
        });

    Ok(Box::new(iter))
}

pub(crate) fn build_settled_example(
    request_id: String,
    device_id: String,
    time: String,
    input: Zeta2PromptInput,
    requested_output: String,
    settled_editable_region: String,
    requested_format: ZetaFormat,
    mav_version: Option<String>,
) -> Example {
    let requested_editable_range =
        excerpt_range_for_format(requested_format, &input.excerpt_ranges).0;

    let base_cursor_excerpt = input.cursor_excerpt.to_string();

    let requested_range_is_valid = requested_editable_range.start <= requested_editable_range.end
        && requested_editable_range.end <= base_cursor_excerpt.len();
    let mut example = build_example_from_snowflake(
        request_id.clone(),
        device_id,
        time,
        input,
        vec!["settled".to_string()],
        None,
        mav_version,
    );

    if !requested_range_is_valid {
        log::warn!(
            "skipping malformed requested range for request {}: requested={:?} (base_len={})",
            request_id,
            requested_editable_range,
            base_cursor_excerpt.len(),
        );
        return example;
    }

    let settled_replacement = settled_editable_region.as_str();
    let rejected_patch = build_output_patch(
        &example.spec.cursor_path,
        &base_cursor_excerpt,
        &requested_editable_range,
        &requested_output,
    );
    let expected_patch = build_output_patch(
        &example.spec.cursor_path,
        &base_cursor_excerpt,
        &requested_editable_range,
        settled_replacement,
    );

    example.spec.expected_patches = vec![expected_patch];
    example.spec.rejected_patch = Some(rejected_patch);
    example
}

pub(crate) fn build_captured_example(
    request_id: String,
    device_id: String,
    time: String,
    input: Zeta2PromptInput,
    mut example_spec: ExampleSpec,
    settled_editable_region: String,
    mav_version: Option<String>,
) -> Example {
    let expected_patch = build_output_patch(
        &input.cursor_path,
        input.cursor_excerpt.as_ref(),
        &input.excerpt_ranges.editable_350,
        settled_editable_region.as_str(),
    );

    example_spec.expected_patches = vec![expected_patch];
    example_spec.telemetry = Some(TelemetrySource {
        request_id,
        device_id,
        time,
        rejection_reason: String::new(),
        was_shown: false,
    });

    Example {
        spec: example_spec,
        mav_version,
        prompt_inputs: Some(input),
        prompt: None,
        predictions: Vec::new(),
        score: Vec::new(),
        qa: Vec::new(),
        state: None,
    }
}
