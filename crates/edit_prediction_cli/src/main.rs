mod anthropic_client;
mod distill;
mod example;
mod filter_languages;
mod format_prompt;
mod git;
mod headless;

mod load_project;
mod metrics;
mod openai_client;
mod parse_output;
mod paths;
mod predict;
mod progress;
mod prompt_assets;
mod pull_examples;
mod qa;
mod reorder_patch;
mod repair;
mod retrieve_context;
mod score;
mod split_commit;
mod split_dataset;

mod synthesize;
mod truncate_expected_patch;
mod word_diff;
use anyhow::Context as _;
use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use collections::{HashMap, HashSet};
use edit_prediction::EditPredictionStore;
use futures::channel::mpsc;
use futures::{SinkExt as _, StreamExt as _};
use gaoya::minhash::{
    MinHashIndex, MinHasher, MinHasher32, calculate_minhash_params, compute_minhash_similarity,
};
use gpui::{AppContext as _, BackgroundExecutor, Task};
use zeta_prompt::{ContextSource, ZetaFormat};

use reqwest_client::ReqwestClient;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::VecDeque;
use std::env;
use std::fmt::Display;
use std::fs::{File, OpenOptions};
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::str::FromStr;
use std::sync::Mutex;
use std::{path::PathBuf, sync::Arc};

use crate::distill::run_distill;
use crate::example::{Example, group_examples_by_repo, read_example_files};
use crate::filter_languages::{FilterLanguagesArgs, run_filter_languages};
use crate::format_prompt::run_format_prompt;
use crate::load_project::run_load_project;
use crate::paths::{FAILED_EXAMPLES_DIR, RUN_DIR};
use crate::predict::run_prediction;
use crate::progress::Progress;
use crate::pull_examples::{fetch_settled_examples_after, parse_settled_after_input};
use crate::retrieve_context::{
    ContextRetrievalType, context_sources_for_types, run_context_retrieval,
};
use crate::score::run_scoring;
use crate::split_commit::SplitCommitArgs;
use crate::split_dataset::SplitArgs;
use crate::synthesize::{SynthesizeConfig, run_synthesize};
use crate::truncate_expected_patch::TruncatePatchArgs;

#[path = "main/cli.rs"]
mod cli;
#[path = "main/examples.rs"]
mod examples;
#[path = "main/output.rs"]
mod output;
#[path = "main/runner.rs"]
mod runner;

pub(crate) use cli::*;
pub(crate) use examples::*;
use examples::*;
pub(crate) use output::*;
use output::*;

fn main() {
    runner::run();
}
