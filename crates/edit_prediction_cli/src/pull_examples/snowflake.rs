use super::*;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SnowflakeStatementResponse {
    #[serde(default)]
    pub(crate) data: Vec<Vec<JsonValue>>,
    #[serde(default)]
    pub(crate) result_set_meta_data: Option<SnowflakeResultSetMetaData>,
    #[serde(default)]
    pub(crate) code: Option<String>,
    #[serde(default)]
    pub(crate) message: Option<String>,
    #[serde(default)]
    pub(crate) statement_handle: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SnowflakeResultSetMetaData {
    #[serde(default, rename = "rowType")]
    pub(crate) row_type: Vec<SnowflakeColumnMeta>,
    #[serde(default)]
    pub(crate) num_rows: Option<i64>,
    #[serde(default)]
    pub(crate) partition_info: Vec<SnowflakePartitionInfo>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SnowflakePartitionInfo {}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SnowflakeColumnMeta {
    #[serde(default)]
    pub(crate) name: String,
}

async fn run_sql_with_polling(
    http_client: Arc<dyn HttpClient>,
    base_url: &str,
    token: &str,
    request: &serde_json::Value,
    step_progress: &crate::progress::StepProgress,
    background_executor: BackgroundExecutor,
) -> Result<SnowflakeStatementResponse> {
    let mut response = run_sql(http_client.clone(), base_url, token, request).await?;

    if response.code.as_deref() == Some(SNOWFLAKE_ASYNC_IN_PROGRESS_CODE) {
        let statement_handle = response
            .statement_handle
            .as_ref()
            .context("async query response missing statementHandle")?
            .clone();

        for attempt in 0.. {
            step_progress.set_substatus(format!("polling ({attempt})"));

            background_executor.timer(POLL_INTERVAL).await;

            response = fetch_partition_with_retries(
                http_client.clone(),
                base_url,
                token,
                &statement_handle,
                0,
                background_executor.clone(),
            )
            .await?;

            if response.code.as_deref() != Some(SNOWFLAKE_ASYNC_IN_PROGRESS_CODE) {
                break;
            }
        }
    }

    Ok(response)
}

struct SnowflakeConfig {
    token: String,
    base_url: String,
    role: Option<String>,
}

#[derive(Clone)]
pub(crate) struct QueryRetryState {
    pub(crate) resume_after: String,
    pub(crate) remaining_limit: Option<usize>,
    pub(crate) offset: usize,
}

pub(crate) async fn fetch_examples_with_query<MakeBindings>(
    http_client: Arc<dyn HttpClient>,
    step_progress: &crate::progress::StepProgress,
    background_executor: BackgroundExecutor,
    statement: &str,
    initial_retry_state: QueryRetryState,
    make_bindings: MakeBindings,
    required_columns: &[&str],
    parse_response: for<'a> fn(
        &'a SnowflakeStatementResponse,
        &'a HashMap<String, usize>,
    ) -> Result<Box<dyn Iterator<Item = Example> + 'a>>,
) -> Result<Vec<Example>>
where
    MakeBindings: Fn(&QueryRetryState) -> JsonValue,
{
    let snowflake = SnowflakeConfig {
        token: std::env::var("EP_SNOWFLAKE_API_KEY")
            .context("missing required environment variable EP_SNOWFLAKE_API_KEY")?,
        base_url: std::env::var("EP_SNOWFLAKE_BASE_URL").context(
            "missing required environment variable EP_SNOWFLAKE_BASE_URL (e.g. https://<account>.snowflakecomputing.com)",
        )?,
        role: std::env::var("EP_SNOWFLAKE_ROLE").ok(),
    };

    let mut requested_columns = required_columns.to_vec();
    if !requested_columns.contains(&"continuation_time") {
        requested_columns.push("continuation_time");
    }

    let mut parsed_examples = Vec::new();
    let mut retry_state = initial_retry_state;
    let mut retry_count = 0usize;

    loop {
        let bindings = make_bindings(&retry_state);
        let request = json!({
            "statement": statement,
            "database": "EVENTS",
            "schema": "PUBLIC",
            "warehouse": "DBT",
            "role": snowflake.role.as_deref(),
            "bindings": bindings
        });

        let response = match run_sql_with_polling(
            http_client.clone(),
            &snowflake.base_url,
            &snowflake.token,
            &request,
            step_progress,
            background_executor.clone(),
        )
        .await
        {
            Ok(response) => response,
            Err(error) => {
                if is_snowflake_timeout_error(&error) && !parsed_examples.is_empty() {
                    retry_count += 1;
                    step_progress.set_substatus(format!(
                        "retrying from {} ({retry_count})",
                        retry_state.resume_after
                    ));
                    continue;
                }

                return Err(error);
            }
        };

        let total_rows = response
            .result_set_meta_data
            .as_ref()
            .and_then(|meta| meta.num_rows)
            .unwrap_or(response.data.len() as i64);
        let partition_count = response
            .result_set_meta_data
            .as_ref()
            .map(|meta| meta.partition_info.len())
            .unwrap_or(1)
            .max(1);

        step_progress.set_info(format!("{} rows", total_rows), InfoStyle::Normal);
        step_progress.set_substatus("parsing");

        let column_indices = get_column_indices(&response.result_set_meta_data, &requested_columns);
        let mut rows_fetched_this_attempt = 0usize;
        let mut timed_out_fetching_partition = false;

        parsed_examples.extend(parse_response(&response, &column_indices)?);
        rows_fetched_this_attempt += response.data.len();
        let mut last_continuation_time_this_attempt =
            last_continuation_timestamp_from_response(&response, &column_indices);

        if partition_count > 1 {
            let statement_handle = response
                .statement_handle
                .as_ref()
                .context("response has multiple partitions but no statementHandle")?;

            for partition in 1..partition_count {
                step_progress.set_substatus(format!(
                    "fetching partition {}/{}",
                    partition + 1,
                    partition_count
                ));

                let partition_response = match fetch_partition_with_retries(
                    http_client.clone(),
                    &snowflake.base_url,
                    &snowflake.token,
                    statement_handle,
                    partition,
                    background_executor.clone(),
                )
                .await
                {
                    Ok(response) => response,
                    Err(error) => {
                        if is_snowflake_timeout_error(&error) && rows_fetched_this_attempt > 0 {
                            timed_out_fetching_partition = true;
                            break;
                        }

                        return Err(error);
                    }
                };

                parsed_examples.extend(parse_response(&partition_response, &column_indices)?);
                rows_fetched_this_attempt += partition_response.data.len();

                if let Some(partition_continuation_time) =
                    last_continuation_timestamp_from_response(&partition_response, &column_indices)
                {
                    last_continuation_time_this_attempt = Some(partition_continuation_time);
                }
            }
        }

        if rows_fetched_this_attempt == 0 {
            step_progress.set_substatus("done");
            return Ok(parsed_examples);
        }

        if let Some(remaining_limit_value) = &mut retry_state.remaining_limit {
            *remaining_limit_value =
                remaining_limit_value.saturating_sub(rows_fetched_this_attempt);
            if *remaining_limit_value == 0 {
                step_progress.set_substatus("done");
                return Ok(parsed_examples);
            }
        }

        if !timed_out_fetching_partition {
            step_progress.set_substatus("done");
            return Ok(parsed_examples);
        }

        let Some(last_continuation_time_this_attempt) = last_continuation_time_this_attempt else {
            step_progress.set_substatus("done");
            return Ok(parsed_examples);
        };

        retry_state.resume_after = last_continuation_time_this_attempt;
        retry_state.offset = 0;
        retry_count += 1;
        step_progress.set_substatus(format!(
            "retrying from {} ({retry_count})",
            retry_state.resume_after
        ));
    }
}

pub(crate) async fn fetch_partition(
    http_client: Arc<dyn HttpClient>,
    base_url: &str,
    token: &str,
    statement_handle: &str,
    partition: usize,
) -> Result<SnowflakeStatementResponse> {
    let url = format!(
        "{}/api/v2/statements/{}?partition={}",
        base_url.trim_end_matches('/'),
        statement_handle,
        partition
    );

    let http_request = Request::builder()
        .method(Method::GET)
        .uri(url.as_str())
        .header("Authorization", format!("Bearer {token}"))
        .header(
            "X-Snowflake-Authorization-Token-Type",
            "PROGRAMMATIC_ACCESS_TOKEN",
        )
        .header("Accept", "application/json")
        .header("Accept-Encoding", "gzip")
        .header("User-Agent", "edit_prediction_cli")
        .body(AsyncBody::empty())?;

    let response = http_client
        .send(http_request)
        .await
        .context("failed to send partition request to Snowflake SQL API")?;

    let status = response.status();
    let content_encoding = response
        .headers()
        .get("content-encoding")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_lowercase());

    let body_bytes = {
        use futures::AsyncReadExt as _;

        let mut body = response.into_body();
        let mut bytes = Vec::new();
        body.read_to_end(&mut bytes)
            .await
            .context("failed to read Snowflake SQL API partition response body")?;
        bytes
    };

    let body_bytes = if content_encoding.as_deref() == Some("gzip") {
        let mut decoder = GzDecoder::new(&body_bytes[..]);
        let mut decompressed = Vec::new();
        decoder
            .read_to_end(&mut decompressed)
            .context("failed to decompress gzip response")?;
        decompressed
    } else {
        body_bytes
    };

    if !status.is_success() && status.as_u16() != 202 {
        let body_text = String::from_utf8_lossy(&body_bytes);
        anyhow::bail!(
            "snowflake sql api partition request http {}: {}",
            status.as_u16(),
            body_text
        );
    }

    if body_bytes.is_empty() {
        anyhow::bail!(
            "snowflake sql api partition {} returned empty response body (http {})",
            partition,
            status.as_u16()
        );
    }

    serde_json::from_slice::<SnowflakeStatementResponse>(&body_bytes).with_context(|| {
        let body_preview = String::from_utf8_lossy(&body_bytes[..body_bytes.len().min(500)]);
        format!(
            "failed to parse Snowflake SQL API partition {} response JSON (http {}): {}",
            partition,
            status.as_u16(),
            body_preview
        )
    })
}

async fn fetch_partition_with_retries(
    http_client: Arc<dyn HttpClient>,
    base_url: &str,
    token: &str,
    statement_handle: &str,
    partition: usize,
    background_executor: BackgroundExecutor,
) -> Result<SnowflakeStatementResponse> {
    let mut last_error = None;

    for retry_attempt in 0..=PARTITION_FETCH_MAX_RETRIES {
        match fetch_partition(
            http_client.clone(),
            base_url,
            token,
            statement_handle,
            partition,
        )
        .await
        {
            Ok(response) => return Ok(response),
            Err(error) => {
                if retry_attempt == PARTITION_FETCH_MAX_RETRIES
                    || !is_transient_partition_fetch_error(&error)
                {
                    return Err(error);
                }

                last_error = Some(error);
                background_executor
                    .timer(PARTITION_FETCH_RETRY_DELAYS[retry_attempt])
                    .await;
            }
        }
    }

    match last_error {
        Some(error) => Err(error),
        None => anyhow::bail!("partition fetch retry loop exited without a result"),
    }
}

fn is_transient_partition_fetch_error(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        let message = cause.to_string();
        message.contains("failed to read Snowflake SQL API partition response body")
            || message.contains("unexpected EOF")
            || message.contains("peer closed connection without sending TLS close_notify")
    })
}

pub(crate) async fn run_sql(
    http_client: Arc<dyn HttpClient>,
    base_url: &str,
    token: &str,
    request: &serde_json::Value,
) -> Result<SnowflakeStatementResponse> {
    let url = format!("{}/api/v2/statements", base_url.trim_end_matches('/'));

    let request_body =
        serde_json::to_vec(request).context("failed to serialize Snowflake SQL API request")?;

    let http_request = Request::builder()
        .method(Method::POST)
        .uri(url.as_str())
        .header("Authorization", format!("Bearer {token}"))
        .header(
            "X-Snowflake-Authorization-Token-Type",
            "PROGRAMMATIC_ACCESS_TOKEN",
        )
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header("User-Agent", "edit_prediction_cli")
        .body(AsyncBody::from(request_body.clone()))?;

    let response = http_client
        .send(http_request)
        .await
        .context("failed to send request to Snowflake SQL API")?;

    let status = response.status();
    let body_bytes = {
        use futures::AsyncReadExt as _;

        let mut body = response.into_body();
        let mut bytes = Vec::new();
        body.read_to_end(&mut bytes)
            .await
            .context("failed to read Snowflake SQL API response body")?;
        bytes
    };

    let snowflake_response = serde_json::from_slice::<SnowflakeStatementResponse>(&body_bytes)
        .context("failed to parse Snowflake SQL API response JSON")?;

    if !status.is_success() && status.as_u16() != 202 && !is_timeout_response(&snowflake_response) {
        let body_text = String::from_utf8_lossy(&body_bytes);
        anyhow::bail!("snowflake sql api http {}: {}", status.as_u16(), body_text);
    }

    if is_timeout_response(&snowflake_response) {
        anyhow::bail!(
            "snowflake sql api timed out code={} message={}",
            snowflake_response.code.as_deref().unwrap_or("<no code>"),
            snowflake_response
                .message
                .as_deref()
                .unwrap_or("<no message>")
        );
    }

    Ok(snowflake_response)
}
