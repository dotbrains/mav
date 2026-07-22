use super::*;

fn rated_examples_from_response<'a>(
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

            let request_id = get_string("request_id");
            let inputs_json = get_json("inputs");
            let inputs: Option<Zeta2PromptInput> = match &inputs_json {
                Some(v) => match serde_json::from_value(v.clone()) {
                    Ok(parsed) => Some(parsed),
                    Err(e) => {
                        log::warn!(
                            "skipping row {row_index}: failed to parse inputs - {e}",
                        );
                        return None;
                    }
                },
                None => None,
            };
            let output = get_string("output");
            let settled_editable_region = get_string("settled_editable_region");
            let rating = get_string("rating");
            let feedback = get_string("feedback").unwrap_or_default();
            let device_id = get_string("device_id");
            let time = get_string("time");
            let experiment_name = get_string("experiment_name");
            let environment = get_string("environment");
            let mav_version = get_string("mav_version");

            match (inputs, output.clone(), rating.clone(), time.clone()) {
                (Some(inputs), Some(output), Some(rating), Some(time)) => {
                    Some(build_rated_example(
                        request_id,
                        device_id.unwrap_or_default(),
                        time,
                        inputs,
                        output,
                        settled_editable_region,
                        rating,
                        feedback,
                        experiment_name,
                        environment,
                        mav_version,
                    ))
                }
                _ => {
                    log::warn!(
                        "skipping row {row_index}: missing fields - inputs={:?} output={:?} rating={:?} time={:?}",
                        inputs_json.is_some(),
                        output.is_some(),
                        rating.is_some(),
                        time.is_some(),
                    );
                    None
                }
            }
        });

    Ok(Box::new(iter))
}

fn build_rated_example(
    request_id: Option<String>,
    device_id: String,
    time: String,
    input: Zeta2PromptInput,
    output: String,
    settled_editable_region: Option<String>,
    rating: String,
    feedback: String,
    experiment_name: Option<String>,
    environment: Option<String>,
    mav_version: Option<String>,
) -> Example {
    let parsed_rating = if rating == "Positive" {
        EditPredictionRating::Positive
    } else {
        EditPredictionRating::Negative
    };
    let is_positive = parsed_rating == EditPredictionRating::Positive;
    let request_id = request_id.unwrap_or_else(|| format!("rated-{}-{}", device_id, time));

    let mut tags = Vec::with_capacity(3);
    tags.push(if is_positive {
        "rated:positive".to_string()
    } else {
        "rated:negative".to_string()
    });
    if let Some(experiment) = experiment_name {
        tags.push(format!("experiment:{experiment}"));
    }
    if let Some(env) = environment {
        tags.push(format!("environment:{env}"));
    }

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
    let mut example =
        build_example_from_snowflake(request_id, device_id, time, input, tags, None, mav_version);

    example.spec.rating = Some(parsed_rating);

    if !feedback.is_empty() {
        example
            .spec
            .human_feedback
            .push(edit_prediction::example_spec::HumanFeedback { message: feedback });
    }

    if let Some(expected_patch) = expected_patch {
        example.spec.expected_patches = vec![expected_patch];
    } else if is_positive {
        example.spec.expected_patches = vec![output.clone()];
    }

    if !is_positive {
        example.spec.rejected_patch = Some(output);
    }

    example
}
