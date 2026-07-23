use super::*;

mod provider;
pub(crate) use provider::*;

#[derive(Parser, Debug)]
#[command(name = "ep")]
pub(crate) struct EpArgs {
    #[arg(long, default_value_t = false)]
    pub(crate) printenv: bool,
    #[clap(long, default_value_t = 10, global = true)]
    pub(crate) max_parallelism: usize,
    /// Process all examples from a repository together instead of distributing examples across workers.
    #[clap(long, default_value_t = false, global = true)]
    pub(crate) group_by_repo: bool,
    /// The limit for the number of examples to process
    /// Default is unlimited for processing local datasets, 5000 when pulling from snowflake
    #[clap(long, global = true)]
    pub(crate) limit: Option<usize>,
    #[clap(long, global = true)]
    pub(crate) offset: Option<usize>,
    /// Filter examples by name
    #[clap(long, global = true)]
    pub(crate) name: Option<String>,
    /// Filter examples by repository
    #[clap(long, global = true)]
    pub(crate) repo: Option<String>,
    /// Deduplicate by cursor position and keep at most this many examples per cluster
    #[clap(long, global = true)]
    pub(crate) max_duplicates: Option<usize>,
    #[command(subcommand)]
    pub(crate) command: Option<Command>,
    /// Input file paths
    #[clap(global = true)]
    pub(crate) inputs: Vec<PathBuf>,
    #[arg(long, short, global = true)]
    pub(crate) output: Option<PathBuf>,
    #[arg(long, short, global = true)]
    pub(crate) in_place: bool,
    #[arg(long, global = true)]
    pub(crate) failfast: bool,
    /// How to handle failed examples in output: keep them or skip them.
    /// Failed examples are always logged to the run's failed directory.
    #[arg(long, global = true, default_value = "keep")]
    pub(crate) failed: FailedHandling,
    /// Output as markdown files instead of JSONL. When set, -o specifies a directory
    /// where one .md file per example will be written (named after each example).
    #[arg(long, short, global = true)]
    pub(crate) markdown: bool,
}

/// Controls whether failed examples are included in the main output.
/// Failed examples are always logged to the run's failed/ directory regardless of this setting.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum FailedHandling {
    /// Include failed examples in the main output (default)
    #[default]
    Keep,
    /// Exclude failed examples from the main output
    Skip,
    /// Skip writing files
    SkipNoFiles,
}

#[derive(Args, Debug, Clone)]
pub(crate) struct ContextArgs {
    /// Which context collectors to run.
    /// May be repeated or comma-delimited, e.g. `--type=all,oracle-file`.
    #[arg(long = "type", value_enum, value_delimiter = ',')]
    pub(crate) context_types: Vec<ContextRetrievalType>,
    /// Recompute context even if the example already has related files.
    #[arg(long, short = 'f', default_value_t = false)]
    pub(crate) force: bool,
}

impl ContextArgs {
    pub(crate) fn context_types(&self) -> Vec<ContextRetrievalType> {
        if self.context_types.is_empty() {
            vec![ContextRetrievalType::Lsp]
        } else {
            self.context_types.clone()
        }
    }
}

const INPUTS_HELP: &str = r#"
Inputs can be file paths or special specifiers:

  path
      Path to an example(s) file (.md, .json, or .jsonl)

  captured-after:{timestamp}
      Fetch captured examples from Snowflake after the given RFC3339 timestamp.
      These are examples captured via the "Capture Edit Prediction Example" action.

  rejected-after:{timestamp}
      Fetch rejected edit predictions from Snowflake after the given RFC3339 timestamp.
      These are predictions that were shown to users but rejected (useful for DPO training).

  settled-after:{timestamp}
      Fetch settled stream examples from Snowflake after the given RFC3339 timestamp.
      These are examples from the edit prediction settled stream.

  rated-after:{timestamp}
      Fetch user-rated edit predictions from Snowflake after the given RFC3339 timestamp.
      These are predictions that users explicitly rated as positive or negative via the
      rate completions modal. Only zeta2 predictions are included.
      - Positive ratings: output becomes expected_patches
      - Negative ratings: output becomes rejected_patch

  rated-positive-after:{timestamp}
      Same as rated-after, but only fetches positively rated predictions.

  rated-negative-after:{timestamp}
      Same as rated-after, but only fetches negatively rated predictions.

      Required environment variables to connect to Snowflake:
          EP_SNOWFLAKE_API_KEY
          EP_SNOWFLAKE_BASE_URL

      Optional:
          EP_SNOWFLAKE_ROLE

Examples:

  # Read examples from a file
  ep read examples.jsonl -o output.jsonl

  # Read captured examples after a timestamp
  ep read captured-after:2025-01-01T00:00:00Z -o captured.jsonl

  # Read rejected predictions for DPO training
  ep read rejected-after:2025-01-01T00:00:00Z -o rejected.jsonl

  # Read user-rated predictions
  ep read rated-after:2025-01-01T00:00:00Z -o rated.jsonl

  # Read settled stream examples
  ep read settled-after:2025-01-01T00:00:00Z -o settled.jsonl

  # Read only positively rated predictions
  ep read rated-positive-after:2025-01-01T00:00:00Z -o positive.jsonl

  # Read only negatively rated predictions
  ep read rated-negative-after:2025-01-01T00:00:00Z -o negative.jsonl

  # Mix multiple input sources
  ep predict examples.jsonl captured-after:2025-01-01T00:00:00Z
"#;

#[derive(Subcommand, Debug, Clone)]
pub(crate) enum Command {
    /// Read examples from files or fetch from Snowflake, output as .jsonl
    Read(ReadArgs),
    /// Create git worktrees for each example and load file contents
    LoadProject,
    /// Retrieve context for input examples.
    Context(ContextArgs),
    /// Generate a prompt string for a specific model
    FormatPrompt(FormatPromptArgs),
    /// Runs edit prediction
    Predict(PredictArgs),
    /// Parse model outputs (actual_output) into unified diffs (actual_patch).
    /// Requires format-prompt to have been run first. Uses provider from prompt.
    ParseOutput,
    /// Computes a score based on actual and expected patches
    Score(PredictArgs),
    /// Prepares a distillation dataset by copying expected outputs to
    /// predicted outputs and removing actual outputs and prompts.
    Distill,
    /// Print aggregated scores
    Eval(EvalArgs),
    /// Generate eval examples by analyzing git commits from a repository
    Synthesize(SynthesizeArgs),
    /// Remove git repositories and worktrees
    Clean,
    /// Generate an evaluation example by splitting a chronologically-ordered commit
    SplitCommit(SplitCommitArgs),
    /// Truncate expected patch by the given criteria
    TruncatePatch(TruncatePatchArgs),
    /// Split a JSONL dataset into multiple files (stratified by repository_url if present)
    Split(SplitArgs),
    /// Filter a JSONL dataset by programming language (based on cursor_path extension)
    FilterLanguages(FilterLanguagesArgs),
    /// Import Anthropic batch results by batch IDs (useful for recovering after database loss)
    ImportBatch(ImportBatchArgs),
    /// Assess the quality of predictions using LLM-as-a-judge
    Qa(qa::QaArgs),
    /// Repair predictions that received poor QA scores by generating improved predictions
    Repair(repair::RepairArgs),
    /// Print all valid zeta formats (lowercase, one per line)
    PrintZetaFormats,
}

impl Display for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Command::Read(_) => write!(f, "read"),
            Command::LoadProject => write!(f, "load-project"),
            Command::Context(args) => {
                write!(f, "context --type=")?;
                for (index, context_type) in args.context_types().iter().enumerate() {
                    if index > 0 {
                        write!(f, ",")?;
                    }
                    write!(f, "{}", context_type)?;
                }
                if args.force {
                    write!(f, " --force")?;
                }
                Ok(())
            }
            Command::FormatPrompt(args) => {
                write!(f, "format-prompt --provider={}", args.provider)
            }
            Command::Predict(args) => match &args.provider {
                Some(provider) => write!(f, "predict --provider={}", provider),
                None => write!(f, "predict"),
            },
            Command::ParseOutput => write!(f, "parse-output"),
            Command::Score(args) => match &args.provider {
                Some(provider) => write!(f, "score --provider={}", provider),
                None => write!(f, "score"),
            },
            Command::Distill => write!(f, "distill"),
            Command::Eval(args) => {
                write!(f, "eval")?;
                if args.context_only {
                    write!(f, " --context-only")?;
                }
                if !args.context_types.is_empty() {
                    write!(f, " --type=")?;
                    for (index, context_type) in args.context_types.iter().enumerate() {
                        if index > 0 {
                            write!(f, ",")?;
                        }
                        write!(f, "{}", context_type)?;
                    }
                }
                if args.related_context_limit != score::EVAL_RELATED_CONTEXT_TOKENS_LIMIT {
                    write!(f, " --related-context-limit={}", args.related_context_limit)?;
                }
                if let Some(provider) = &args.predict.provider {
                    write!(f, " --provider={}", provider)?;
                }
                Ok(())
            }
            Command::Synthesize(args) => {
                write!(f, "synthesize --repos {}", args.repos.join(" "))
            }
            Command::Clean => write!(f, "clean"),
            Command::SplitCommit(_) => write!(f, "split-commit"),
            Command::TruncatePatch(_) => write!(f, "truncate-patch"),
            Command::Split(_) => write!(f, "split"),
            Command::FilterLanguages(_) => write!(f, "filter-languages"),
            Command::ImportBatch(args) => {
                write!(f, "import-batch --batch-ids {}", args.batch_ids.join(" "))
            }
            Command::Qa(_) => {
                write!(f, "qa")
            }
            Command::Repair(_) => {
                write!(f, "repair")
            }
            Command::PrintZetaFormats => {
                write!(f, "print-zeta-formats")
            }
        }
    }
}

#[derive(Debug, Args, Clone)]
#[command(after_help = INPUTS_HELP)]
pub(crate) struct ReadArgs {}

#[derive(Debug, Args, Clone)]
pub(crate) struct FormatPromptArgs {
    #[clap(long, short('p'), default_value_t = PredictionProvider::default())]
    pub(crate) provider: PredictionProvider,
    /// Token budget for related-file context in teacher-jumps prompts.
    #[clap(long, default_value_t = format_prompt::TeacherJumpsPrompt::DEFAULT_RELATED_FILES_BUDGET)]
    pub(crate) related_files_budget: usize,
}

#[derive(Debug, Args, Clone)]
pub(crate) struct PredictArgs {
    #[clap(long, short('p'))]
    pub(crate) provider: Option<PredictionProvider>,
    #[clap(long, default_value_t = 1)]
    pub(crate) repetitions: usize,
    /// Only use cached responses, don't queue new requests for batching
    #[clap(long)]
    pub(crate) cache_only: bool,
    /// Wait for all batches to complete before exiting (only applies to batched providers like teacher)
    #[clap(long)]
    pub(crate) wait: bool,
}

#[derive(Debug, Args, Clone)]
pub(crate) struct EvalArgs {
    #[clap(flatten)]
    pub(crate) predict: PredictArgs,
    /// Only compute editable context coverage from expected patches and retrieved context.
    #[clap(long)]
    pub(crate) context_only: bool,
    /// Only score persisted related context excerpts from these context types.
    /// May be repeated or comma-delimited, e.g. `--type=current-file,edit-history`.
    #[arg(long = "type", value_enum, value_delimiter = ',')]
    pub(crate) context_types: Vec<ContextRetrievalType>,
    /// Maximum number of retrieved context tokens to include when scoring.
    #[clap(long, default_value_t = score::EVAL_RELATED_CONTEXT_TOKENS_LIMIT)]
    pub(crate) related_context_limit: usize,
    /// Path to write summary scores as JSON
    #[clap(long)]
    pub(crate) summary_json: Option<PathBuf>,
    /// Print all individual example lines (default: up to 20)
    #[clap(long)]
    pub(crate) verbose: bool,
}

impl EvalArgs {
    pub(crate) fn context_source_filter(&self) -> Option<Vec<ContextSource>> {
        if self.context_types.is_empty() {
            None
        } else {
            Some(context_sources_for_types(&self.context_types))
        }
    }
}

#[derive(Clone, Default, Debug, PartialEq, Eq, Hash, Args)]
pub(crate) struct SynthesizeArgs {
    /// Repository URLs (git@github.com:owner/repo or https://...)
    #[clap(long, required = true, num_args = 1..)]
    pub(crate) repos: Vec<String>,

    /// Number of examples to generate per repository
    #[clap(long, default_value_t = 5)]
    pub(crate) count: usize,

    /// Maximum commits to scan per repository before giving up
    #[clap(long, default_value_t = 100)]
    pub(crate) max_commits: usize,

    /// Ignore state file and reprocess all commits
    #[clap(long)]
    pub(crate) fresh: bool,
}

#[derive(Debug, Args, Clone)]
pub(crate) struct ImportBatchArgs {
    /// Batch IDs to import (e.g., msgbatch_xxx for Anthropic, batch_xxx for OpenAI)
    #[clap(long, required = true, num_args = 1..)]
    pub(crate) batch_ids: Vec<String>,
    /// Which provider's batches to import (anthropic or openai)
    #[clap(long, default_value = "anthropic")]
    pub(crate) provider: BatchProvider,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum BatchProvider {
    Anthropic,
    Openai,
}

impl EpArgs {
    pub(crate) fn output_path(&self) -> Option<PathBuf> {
        if self.in_place {
            if self.inputs.len() == 1 {
                self.inputs.first().cloned()
            } else {
                panic!("--in-place requires exactly one input file")
            }
        } else {
            self.output.clone()
        }
    }
}

/// Minimum Mav version required for Snowflake queries.
/// This version introduced the current request schema with predicted edits in the edit
/// history, and open source repos distinguished.
pub(crate) const MIN_CAPTURE_VERSION: pull_examples::MinCaptureVersion =
    pull_examples::MinCaptureVersion {
        major: 0,
        minor: 224,
        patch: 1,
    };
