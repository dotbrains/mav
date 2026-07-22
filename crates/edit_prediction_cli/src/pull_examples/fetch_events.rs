use super::*;

pub async fn fetch_requested_examples_after(
    http_client: Arc<dyn HttpClient>,
    after_timestamps: &[String],
    max_rows_per_timestamp: Option<usize>,
    offset: usize,
    background_executor: BackgroundExecutor,
    min_capture_version: Option<MinCaptureVersion>,
) -> Result<Vec<Example>> {
    if after_timestamps.is_empty() {
        return Ok(Vec::new());
    }

    let progress = Progress::global();

    let mut all_examples = Vec::new();

    for after_date in after_timestamps.iter() {
        let step_progress_name = format!("requested>{after_date}");
        let step_progress = progress.start(Step::PullExamples, &step_progress_name);
        step_progress.set_substatus("querying");

        let min_version_str = min_capture_version.map(|version| {
            (version.major as u64 * 1_000_000 + version.minor as u64 * 1_000 + version.patch as u64)
                .to_string()
        });
        let min_version_ref = min_version_str.as_deref();

        let statement = indoc! {r#"
            SELECT
                ep_request_id AS request_id,
                device_id AS device_id,
                requested_at::string AS continuation_time,
                requested_at::string AS time,
                input_payload AS input,
                mav_version AS mav_version
            FROM MAV_DBT.DBT_PROD.fct_edit_prediction_examples
            WHERE requested_at > TRY_TO_TIMESTAMP_NTZ(?)
                AND (? IS NULL OR (
                    COALESCE(TRY_CAST(SPLIT_PART(mav_version, '.', 1) AS INTEGER), 0) * 1000000
                    + COALESCE(TRY_CAST(SPLIT_PART(mav_version, '.', 2) AS INTEGER), 0) * 1000
                    + COALESCE(TRY_CAST(SPLIT_PART(SPLIT_PART(mav_version, '.', 3), '+', 1) AS INTEGER), 0)
                ) >= ?)
            ORDER BY requested_at ASC
            LIMIT ?
            OFFSET ?
        "#};

        let examples = fetch_examples_with_query(
            http_client.clone(),
            &step_progress,
            background_executor.clone(),
            statement,
            QueryRetryState {
                resume_after: after_date.clone(),
                remaining_limit: max_rows_per_timestamp,
                offset,
            },
            |retry_state| {
                json!({
                    "1": { "type": "TEXT", "value": retry_state.resume_after },
                    "2": { "type": "FIXED", "value": min_version_ref },
                    "3": { "type": "FIXED", "value": min_version_ref },
                    "4": { "type": "FIXED", "value": format_limit(retry_state.remaining_limit) },
                    "5": { "type": "FIXED", "value": retry_state.offset.to_string() }
                })
            },
            &["request_id", "device_id", "time", "input", "mav_version"],
            requested_examples_from_response,
        )
        .await?;

        all_examples.extend(examples);
    }

    Ok(all_examples)
}

pub async fn fetch_captured_examples_after(
    http_client: Arc<dyn HttpClient>,
    after_timestamps: &[String],
    max_rows_per_timestamp: Option<usize>,
    offset: usize,
    background_executor: BackgroundExecutor,
    min_capture_version: Option<MinCaptureVersion>,
) -> Result<Vec<Example>> {
    if after_timestamps.is_empty() {
        return Ok(Vec::new());
    }

    let progress = Progress::global();

    let mut all_examples = Vec::new();

    for after_date in after_timestamps.iter() {
        let step_progress_name = format!("captured>{after_date}");
        let step_progress = progress.start(Step::PullExamples, &step_progress_name);
        step_progress.set_substatus("querying");

        let min_version_str = min_capture_version.map(|version| {
            (version.major as u64 * 1_000_000 + version.minor as u64 * 1_000 + version.patch as u64)
                .to_string()
        });
        let min_version_ref = min_version_str.as_deref();

        let statement = indoc! {r#"
            SELECT
                ep_request_id AS request_id,
                device_id AS device_id,
                requested_at::string AS continuation_time,
                requested_at::string AS time,
                input_payload AS input,
                settled_editable_region AS settled_editable_region,
                example_payload AS example,
                mav_version AS mav_version
            FROM MAV_DBT.DBT_PROD.fct_edit_prediction_examples
            WHERE settled_editable_region IS NOT NULL
                AND example_payload IS NOT NULL
                AND requested_at > TRY_TO_TIMESTAMP_NTZ(?)
                AND (? IS NULL OR (
                    COALESCE(TRY_CAST(SPLIT_PART(mav_version, '.', 1) AS INTEGER), 0) * 1000000
                    + COALESCE(TRY_CAST(SPLIT_PART(mav_version, '.', 2) AS INTEGER), 0) * 1000
                    + COALESCE(TRY_CAST(SPLIT_PART(SPLIT_PART(mav_version, '.', 3), '+', 1) AS INTEGER), 0)
                ) >= ?)
            ORDER BY requested_at ASC
            LIMIT ?
            OFFSET ?
        "#};

        let examples = fetch_examples_with_query(
            http_client.clone(),
            &step_progress,
            background_executor.clone(),
            statement,
            QueryRetryState {
                resume_after: after_date.clone(),
                remaining_limit: max_rows_per_timestamp,
                offset,
            },
            |retry_state| {
                json!({
                    "1": { "type": "TEXT", "value": retry_state.resume_after },
                    "2": { "type": "FIXED", "value": min_version_ref },
                    "3": { "type": "FIXED", "value": min_version_ref },
                    "4": { "type": "FIXED", "value": format_limit(retry_state.remaining_limit) },
                    "5": { "type": "FIXED", "value": retry_state.offset.to_string() }
                })
            },
            &[
                "request_id",
                "device_id",
                "time",
                "input",
                "settled_editable_region",
                "example",
                "mav_version",
            ],
            captured_examples_from_response,
        )
        .await?;

        all_examples.extend(examples);
    }

    Ok(all_examples)
}

pub async fn fetch_settled_examples_after(
    http_client: Arc<dyn HttpClient>,
    after_timestamps: &[String],
    max_rows_per_timestamp: Option<usize>,
    offset: usize,
    background_executor: BackgroundExecutor,
    min_capture_version: Option<MinCaptureVersion>,
) -> Result<Vec<Example>> {
    if after_timestamps.is_empty() {
        return Ok(Vec::new());
    }

    let progress = Progress::global();

    let mut all_examples = Vec::new();

    for after_date in after_timestamps.iter() {
        let step_progress_name = format!("settled>{after_date}");
        let step_progress = progress.start(Step::PullExamples, &step_progress_name);
        step_progress.set_substatus("querying");

        let _ = min_capture_version;

        let statement = indoc! {r#"
            SELECT
                ep_request_id AS request_id,
                device_id AS device_id,
                requested_at::string AS continuation_time,
                requested_at::string AS time,
                input_payload AS input,
                requested_output AS requested_output,
                settled_editable_region AS settled_editable_region,
                requested_format AS requested_format,
                mav_version AS mav_version
            FROM MAV_DBT.DBT_PROD.fct_edit_prediction_examples
            WHERE settled_editable_region IS NOT NULL
                AND requested_at > TRY_TO_TIMESTAMP_NTZ(?)
            ORDER BY requested_at ASC
            LIMIT ?
            OFFSET ?
        "#};

        let examples = fetch_examples_with_query(
            http_client.clone(),
            &step_progress,
            background_executor.clone(),
            statement,
            QueryRetryState {
                resume_after: after_date.clone(),
                remaining_limit: max_rows_per_timestamp,
                offset,
            },
            |retry_state| {
                json!({
                    "1": { "type": "TEXT", "value": retry_state.resume_after },
                    "2": { "type": "FIXED", "value": format_limit(retry_state.remaining_limit) },
                    "3": { "type": "FIXED", "value": retry_state.offset.to_string() }
                })
            },
            &[
                "request_id",
                "device_id",
                "time",
                "input",
                "requested_output",
                "settled_editable_region",
                "requested_format",
                "mav_version",
            ],
            settled_examples_from_response,
        )
        .await?;

        all_examples.extend(examples);
    }

    Ok(all_examples)
}

pub async fn fetch_rated_examples_after(
    http_client: Arc<dyn HttpClient>,
    inputs: &[(String, Option<EditPredictionRating>)],
    max_rows_per_timestamp: Option<usize>,
    offset: usize,
    background_executor: BackgroundExecutor,
    _min_capture_version: Option<MinCaptureVersion>,
) -> Result<Vec<Example>> {
    if inputs.is_empty() {
        return Ok(Vec::new());
    }

    let progress = Progress::global();

    let mut all_examples = Vec::new();

    for (after_date, rating_filter) in inputs.iter() {
        let filter_label = match rating_filter {
            None => "",
            Some(EditPredictionRating::Positive) => ":positive",
            Some(EditPredictionRating::Negative) => ":negative",
        };
        let step_progress_name = format!("rated{filter_label}>{after_date}");
        let step_progress = progress.start(Step::PullExamples, &step_progress_name);
        step_progress.set_substatus("querying");

        let rating_value = rating_filter.as_ref().map(|rating| match rating {
            EditPredictionRating::Positive => "Positive",
            EditPredictionRating::Negative => "Negative",
        });

        let statement = indoc! {r#"
            SELECT
                ep_request_id AS request_id,
                rated_inputs AS inputs,
                rated_output AS output,
                settled_editable_region AS settled_editable_region,
                rating AS rating,
                feedback AS feedback,
                device_id AS device_id,
                requested_at::string AS continuation_time,
                requested_at::string AS time,
                NULL AS experiment_name,
                NULL AS environment,
                mav_version AS mav_version
            FROM MAV_DBT.DBT_PROD.fct_edit_prediction_examples
            WHERE rating IS NOT NULL
                AND (? IS NULL OR rating = ?)
                AND requested_at > TRY_TO_TIMESTAMP_NTZ(?)
                AND rated_inputs IS NOT NULL
                AND rated_inputs:cursor_excerpt IS NOT NULL
                AND rated_output IS NOT NULL
            ORDER BY requested_at ASC
            LIMIT ?
            OFFSET ?
        "#};

        let examples = fetch_examples_with_query(
            http_client.clone(),
            &step_progress,
            background_executor.clone(),
            statement,
            QueryRetryState {
                resume_after: after_date.clone(),
                remaining_limit: max_rows_per_timestamp,
                offset,
            },
            |retry_state| {
                json!({
                    "1": { "type": "TEXT", "value": rating_value },
                    "2": { "type": "TEXT", "value": rating_value },
                    "3": { "type": "TEXT", "value": retry_state.resume_after },
                    "4": { "type": "FIXED", "value": format_limit(retry_state.remaining_limit) },
                    "5": { "type": "FIXED", "value": retry_state.offset.to_string() }
                })
            },
            &[
                "request_id",
                "inputs",
                "output",
                "settled_editable_region",
                "rating",
                "feedback",
                "device_id",
                "time",
                "experiment_name",
                "environment",
                "mav_version",
            ],
            rated_examples_from_response,
        )
        .await?;

        all_examples.extend(examples);
    }

    Ok(all_examples)
}
