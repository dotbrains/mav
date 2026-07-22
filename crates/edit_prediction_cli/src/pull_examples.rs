use anyhow::{Context as _, Result};
use flate2::read::GzDecoder;
use gpui::BackgroundExecutor;
use http_client::{AsyncBody, HttpClient, Method, Request};
use indoc::indoc;
use serde::Deserialize;
use serde_json::{Value as JsonValue, json};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::Read;
use std::sync::Arc;
use std::time::Duration;
use telemetry_events::EditPredictionRating;

use zeta_prompt::{Zeta2PromptInput, ZetaFormat, excerpt_range_for_format};

use crate::PredictionProvider;
use crate::example::{Example, ExamplePrediction, ExamplePrompt};
use crate::progress::{InfoStyle, Progress, Step};
use edit_prediction::example_spec::{ExampleSpec, TelemetrySource};

pub(crate) const SNOWFLAKE_SUCCESS_CODE: &str = "090001";
pub(crate) const SNOWFLAKE_ASYNC_IN_PROGRESS_CODE: &str = "333334";
const SNOWFLAKE_TIMEOUT_CODE: &str = "000630";

/// Minimum Mav version for filtering captured examples.
/// For example, `MinCaptureVersion { major: 0, minor: 224, patch: 1 }` means only pull
/// examples where `mav_version >= 0.224.1`. The `major` component is required because Mav
/// moved from the `0.<minor>.<patch>` scheme to `1.<minor>.<patch>`; comparing on `minor`
/// alone would exclude all `1.*` versions (whose `minor` resets to small values).
#[derive(Clone, Copy, Debug)]
pub struct MinCaptureVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

pub(crate) const POLL_INTERVAL: Duration = Duration::from_secs(2);
const PARTITION_FETCH_MAX_RETRIES: usize = 3;
const PARTITION_FETCH_RETRY_DELAYS: [Duration; PARTITION_FETCH_MAX_RETRIES] = [
    Duration::from_millis(500),
    Duration::from_secs(1),
    Duration::from_secs(2),
];

/// Parse an input token of the form `captured-after:{timestamp}`.
mod builders;
mod fetch_events;
mod fetch_outcomes;
mod input;
mod rated;
mod rejected_accepted;
mod requested_settled_captured;
mod snowflake;
mod status;

pub use fetch_events::{
    fetch_captured_examples_after, fetch_rated_examples_after, fetch_requested_examples_after,
    fetch_settled_examples_after,
};
pub use fetch_outcomes::{fetch_accepted_examples_after, fetch_rejected_examples_after};
pub use input::{
    parse_accepted_after_input, parse_captured_after_input, parse_rated_after_input,
    parse_rejected_after_input, parse_requested_after_input, parse_settled_after_input,
};
pub(crate) use snowflake::{
    SnowflakeResultSetMetaData, SnowflakeStatementResponse, fetch_partition, run_sql,
};

use builders::{build_captured_example, build_output_patch, build_settled_example};
use rated::rated_examples_from_response;
use rejected_accepted::{accepted_examples_from_response, rejected_examples_from_response};
use requested_settled_captured::{
    captured_examples_from_response, requested_examples_from_response,
    settled_examples_from_response,
};
use snowflake::{QueryRetryState, fetch_examples_with_query};

struct RejectionInfo {
    reason: String,
    was_shown: bool,
}
