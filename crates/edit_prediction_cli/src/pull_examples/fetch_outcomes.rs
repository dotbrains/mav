use super::*;

pub async fn fetch_rejected_examples_after(
    http_client: Arc<dyn HttpClient>,
    after_timestamps: &[(bool, String)],
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

    for (explicit, after_date) in after_timestamps.iter() {
        let step_progress_name = format!("rejected>{after_date}");
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
                prompt AS prompt,
                requested_output AS output,
                settled_editable_region AS settled_editable_region,
                is_ep_shown_before_rejected AS was_shown,
                ep_rejected_reason AS reason,
                mav_version AS mav_version
            FROM MAV_DBT.DBT_PROD.fct_edit_prediction_examples
            WHERE ep_outcome LIKE ?
                AND is_ep_shown_before_rejected = true
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
                    "1": { "type": "TEXT", "value": if *explicit { "Rejected (Explicit)" } else { "Rejected%" } },
                    "2": { "type": "TEXT", "value": retry_state.resume_after },
                    "3": { "type": "FIXED", "value": min_version_ref },
                    "4": { "type": "FIXED", "value": min_version_ref },
                    "5": { "type": "FIXED", "value": format_limit(retry_state.remaining_limit) },
                    "6": { "type": "FIXED", "value": retry_state.offset.to_string() }
                })
            },
            &[
                "request_id",
                "device_id",
                "time",
                "input",
                "prompt",
                "output",
                "settled_editable_region",
                "was_shown",
                "reason",
                "mav_version",
            ],
            rejected_examples_from_response,
        )
        .await?;

        all_examples.extend(examples);
    }

    Ok(all_examples)
}

pub async fn fetch_accepted_examples_after(
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
        let step_progress_name = format!("accepted>{after_date}");
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
                prompt AS prompt,
                requested_output AS output,
                settled_editable_region AS settled_editable_region,
                mav_version AS mav_version
            FROM MAV_DBT.DBT_PROD.fct_edit_prediction_examples
            WHERE ep_outcome = 'Accepted'
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
                "prompt",
                "output",
                "settled_editable_region",
                "mav_version",
            ],
            accepted_examples_from_response,
        )
        .await?;

        all_examples.extend(examples);
    }

    Ok(all_examples)
}

fn format_limit(limit: Option<usize>) -> String {
    return limit.map(|l| l.to_string()).unwrap_or("NULL".to_string());
}
