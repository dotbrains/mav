//! `ep split` implementation.
//!
//! This command splits a JSONL dataset into multiple files based on size specifications,
//! with optional stratification by a JSON field.
//!
//! # Usage
//!
//! ```text
//! ep split [--stratify=<field>] [input.jsonl] <out1>=<size1> <out2>=<size2> ...
//! ```
//!
//! If `input.jsonl` is not provided or is `-`, reads from stdin.
//!
//! # Size specifications
//!
//! - `80%` - percentage of total examples (lines)
//! - `100` - approximate absolute count of examples (lines)
//! - `rest` - all remaining items (only one split can use this)
//!
//! # Stratification
//!
//! The `--stratify` flag controls how examples are grouped before splitting:
//!
//! - `cursor-path` (default): group by the `cursor_path` JSON field
//! - `project`: group by the first component of the `cursor_path` JSON field
//! - `repo`: group by the `repository_url` JSON field
//! - `none`: no grouping, split individual examples
//!
//! When stratifying, the split ensures each output file contains examples from
//! non-overlapping groups. Size specifications always apply to the number of
//! examples (lines), with whole groups assigned greedily to meet the target.
//! Examples missing the stratification field are treated as individual groups.

use anyhow::{Context as _, Result, bail};
use clap::Args;
use rand::SeedableRng;
use rand::seq::SliceRandom;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

/// `ep split` CLI args.
#[derive(Debug, Args, Clone)]
#[command(
    about = "Split a JSONL dataset into multiple files with optional stratification",
    after_help = r#"SIZE SPECIFICATIONS:
  <percentage>%    Percentage of total (e.g., 80%)
  <count>          Absolute number (e.g., 100)
  rest             All remaining items (only one output can use this)

  Sizes always apply to examples (lines). When stratifying, whole groups
  are assigned greedily to approximate the target count.

EXAMPLES:
  # Split 80% train, 20% validation (default: stratify by cursor_path)
  ep split input.jsonl train.jsonl=80% valid.jsonl=rest

  # Split into train/valid/test
  ep split input.jsonl train.jsonl=80% valid.jsonl=10% test.jsonl=rest

  # Stratify by repository_url instead of cursor_path
  ep split --stratify=repo input.jsonl train.jsonl=80% valid.jsonl=rest

  # No stratification (split by individual examples)
  ep split --stratify=none input.jsonl train.jsonl=80% valid.jsonl=rest

  # Read from stdin
  cat input.jsonl | ep split train.jsonl=80% valid.jsonl=rest

  # Reproducible split with seed
  ep split --seed 42 input.jsonl train.jsonl=80% valid.jsonl=rest

STRATIFICATION:
  Controls how examples are grouped before splitting:
    cursor-path  Group by "cursor_path" field (default)
    project      Group by the first component of the "cursor_path" field
    repo         Group by "repository_url" field
    none         No grouping, split individual examples

  When stratifying, the split ensures each output file contains examples
  from non-overlapping groups. This prevents data leakage between
  train/test splits.
"#
)]
pub struct SplitArgs {
    /// Random seed for reproducibility
    #[arg(long)]
    pub seed: Option<u64>,

    /// Stratification field for splitting the dataset
    #[arg(long, default_value = "cursor-path")]
    pub stratify: Stratify,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum, strum::Display)]
pub enum Stratify {
    #[strum(serialize = "cursor_path")]
    CursorPath,
    #[strum(serialize = "project")]
    Project,
    #[strum(serialize = "repo")]
    Repo,
    #[strum(serialize = "none")]
    None,
}

#[derive(Debug, Clone)]
pub enum SplitSize {
    Percentage(f64),
    Absolute(usize),
    Rest,
}

#[derive(Debug, Clone)]
pub struct SplitSpec {
    pub path: PathBuf,
    pub size: SplitSize,
}

fn parse_split_spec(spec: &str) -> Result<SplitSpec> {
    let (path, size_str) = spec
        .rsplit_once('=')
        .with_context(|| format!("invalid split spec '{}': expected <path>=<size>", spec))?;

    let size = if size_str == "rest" {
        SplitSize::Rest
    } else if size_str.ends_with('%') {
        let pct_str = size_str.trim_end_matches('%');
        let pct: f64 = pct_str
            .parse()
            .with_context(|| format!("invalid percentage '{}' in '{}'", pct_str, spec))?;
        if !(0.0..=100.0).contains(&pct) {
            bail!("percentage must be between 0 and 100, got {}", pct);
        }
        SplitSize::Percentage(pct / 100.0)
    } else {
        let count: usize = size_str
            .parse()
            .with_context(|| format!("invalid count '{}' in '{}'", size_str, spec))?;
        SplitSize::Absolute(count)
    };

    Ok(SplitSpec {
        path: PathBuf::from(path),
        size,
    })
}

fn read_lines_from_input(input: Option<&Path>) -> Result<Vec<String>> {
    let reader: Box<dyn BufRead> = match input {
        Some(path) => {
            let file =
                File::open(path).with_context(|| format!("failed to open '{}'", path.display()))?;
            Box::new(BufReader::new(file))
        }
        None => Box::new(BufReader::new(io::stdin())),
    };

    let lines: Vec<String> = reader
        .lines()
        .collect::<io::Result<Vec<_>>>()
        .context("failed to read input lines")?;

    Ok(lines)
}

fn compute_split_counts(specs: &[SplitSpec], total: usize) -> Result<Vec<usize>> {
    let mut counts = vec![0usize; specs.len()];
    let mut remaining = total;
    let mut rest_index: Option<usize> = None;

    for (i, spec) in specs.iter().enumerate() {
        match &spec.size {
            SplitSize::Percentage(pct) => {
                let count = (total as f64 * pct).round() as usize;
                counts[i] = count.min(remaining);
                remaining = remaining.saturating_sub(counts[i]);
            }
            SplitSize::Absolute(count) => {
                counts[i] = (*count).min(remaining);
                remaining = remaining.saturating_sub(counts[i]);
            }
            SplitSize::Rest => {
                if rest_index.is_some() {
                    bail!("only one split can use 'rest'");
                }
                rest_index = Some(i);
            }
        }
    }

    if let Some(idx) = rest_index {
        counts[idx] = remaining;
    }

    Ok(counts)
}

fn write_lines_to_file(path: &Path, lines: &[String]) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory '{}'", parent.display()))?;
        }
    }

    let file =
        File::create(path).with_context(|| format!("failed to create '{}'", path.display()))?;
    let mut writer = BufWriter::new(file);

    for line in lines {
        writeln!(writer, "{}", line)
            .with_context(|| format!("failed to write to '{}'", path.display()))?;
    }

    writer
        .flush()
        .with_context(|| format!("failed to flush '{}'", path.display()))?;

    Ok(())
}

pub fn run_split(args: &SplitArgs, inputs: &[PathBuf]) -> Result<()> {
    if inputs.is_empty() {
        bail!("usage: ep split [input.jsonl] train.jsonl=80% valid.jsonl=rest");
    }

    let (input_path, split_specs_raw): (Option<&Path>, &[PathBuf]) =
        if inputs.first().is_some_and(|p| {
            let s = p.to_string_lossy();
            !s.contains('=')
        }) {
            let first = inputs.first().map(|p| p.as_path());
            let first = if first == Some(Path::new("-")) {
                None
            } else {
                first
            };
            (first, &inputs[1..])
        } else {
            (None, inputs)
        };

    if split_specs_raw.is_empty() {
        bail!("no split specifications provided");
    }

    let specs: Vec<SplitSpec> = split_specs_raw
        .iter()
        .map(|p| parse_split_spec(&p.to_string_lossy()))
        .collect::<Result<Vec<_>>>()?;

    let lines = read_lines_from_input(input_path)?;
    let total_lines = lines.len();

    if total_lines == 0 {
        for spec in &specs {
            write_lines_to_file(&spec.path, &[])?;
        }
        return Ok(());
    }

    let mut grouped_lines = group_lines(&lines, args.stratify);

    if args.stratify != Stratify::None {
        eprintln!(
            "Stratifying by {} ({} unique groups, {} examples)",
            args.stratify,
            grouped_lines.len(),
            total_lines
        );
    } else {
        eprintln!(
            "No stratification, splitting {} examples by line",
            total_lines
        );
    }

    let mut rng = match args.seed {
        Some(seed) => rand::rngs::StdRng::seed_from_u64(seed),
        None => rand::rngs::StdRng::from_os_rng(),
    };

    grouped_lines.shuffle(&mut rng);

    let line_targets = compute_split_counts(&specs, total_lines)?;
    let rest_index = specs.iter().position(|s| matches!(s.size, SplitSize::Rest));
    let mut split_outputs: Vec<Vec<String>> = vec![Vec::new(); specs.len()];
    let mut group_iter = grouped_lines.into_iter();

    for (split_idx, &target) in line_targets.iter().enumerate() {
        if Some(split_idx) == rest_index {
            continue;
        }
        let mut accumulated = 0;
        while accumulated < target {
            if let Some(group) = group_iter.next() {
                accumulated += group.len();
                split_outputs[split_idx].extend(group);
            } else {
                break;
            }
        }
    }

    if let Some(idx) = rest_index {
        for group in group_iter {
            split_outputs[idx].extend(group);
        }
    }

    for (spec, output_lines) in specs.iter().zip(split_outputs.iter()) {
        write_lines_to_file(&spec.path, output_lines)?;
        eprintln!("{}: {} examples", spec.path.display(), output_lines.len());
    }

    Ok(())
}

/// Groups lines by the specified stratification field.
///
/// When `stratify` is `None`, each line becomes its own group.
/// When a line is missing the stratification field, it is also placed in its own group.
fn group_lines(lines: &[String], stratify: Stratify) -> Vec<Vec<String>> {
    if stratify == Stratify::None {
        return lines.iter().map(|line| vec![line.clone()]).collect();
    }

    let get_key = |line: &str| {
        let json: Value = serde_json::from_str(line).unwrap_or_default();
        match stratify {
            Stratify::Repo => json
                .get("repository_url")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            Stratify::CursorPath => json
                .get("cursor_path")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            Stratify::Project => json
                .get("cursor_path")
                .and_then(|v| v.as_str())
                .and_then(|s| s.split(['/', '\\']).next())
                .map(|s| s.to_string()),
            Stratify::None => unreachable!(),
        }
    };

    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut ungrouped: Vec<Vec<String>> = Vec::new();

    for line in lines {
        let key = get_key(line);
        match key {
            Some(key) => groups.entry(key).or_default().push(line.clone()),
            None => ungrouped.push(vec![line.clone()]),
        }
    }

    let mut result: Vec<Vec<String>> = groups.into_values().collect();
    result.extend(ungrouped);
    result
}

#[cfg(test)]
#[path = "split_dataset/tests.rs"]
mod tests;
