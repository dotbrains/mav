use super::patch_split::generate_evaluation_example_from_ordered_commit;
use super::types::parse_split_point;
use super::*;

pub fn run_split_commit(
    args: &SplitCommitArgs,
    inputs: &[PathBuf],
    output_path: Option<&PathBuf>,
    failed: FailedHandling,
) -> Result<()> {
    use std::collections::HashSet;
    use std::io::BufRead;

    let stdin_path = PathBuf::from("-");
    let inputs = if inputs.is_empty() {
        std::slice::from_ref(&stdin_path)
    } else {
        inputs
    };

    let split_point = args
        .split_point
        .as_deref()
        .map(parse_split_point)
        .transpose()?;
    let mut output_lines = Vec::new();
    let mut processed_commits = 0usize;

    for input_path in inputs {
        let input: Box<dyn BufRead> = if input_path.as_os_str() == "-" {
            Box::new(io::BufReader::new(io::stdin()))
        } else {
            let file = fs::File::open(input_path)
                .with_context(|| format!("failed to open input file {}", input_path.display()))?;
            Box::new(io::BufReader::new(file))
        };

        for (line_num, line_result) in input.lines().enumerate() {
            let line =
                line_result.with_context(|| format!("failed to read line {}", line_num + 1))?;

            if line.trim().is_empty() {
                continue;
            }

            let annotated: AnnotatedCommit = serde_json::from_str(&line)
                .with_context(|| format!("failed to parse JSON at line {}", line_num + 1))?;

            // Generate multiple samples if num_samples is set
            if let Some(num_samples) = args.num_samples {
                let mut seen_samples: HashSet<String> = HashSet::new();
                let base_seed = args.seed.unwrap_or_else(|| rand::random());

                for sample_idx in 0..num_samples {
                    let sample_seed = base_seed.wrapping_add(sample_idx as u64);

                    let case = match generate_evaluation_example_from_ordered_commit(
                        &annotated.reordered_commit,
                        &annotated.repo_url,
                        &annotated.commit_sha,
                        split_point.clone(),
                        Some(sample_seed),
                        Some(sample_idx),
                    ) {
                        Ok(case) => case,
                        Err(e) => {
                            let err_msg = format!(
                                "failed to generate evaluation example for commit {} at line {} (sample {}): {}",
                                annotated.commit_sha,
                                line_num + 1,
                                sample_idx,
                                e
                            );
                            if e.is::<NoMatchingSplitPointError>() {
                                eprintln!("skipping: {}", err_msg);
                                continue;
                            }
                            match failed {
                                FailedHandling::Skip | FailedHandling::SkipNoFiles => {
                                    eprintln!("{}", err_msg);
                                    continue;
                                }
                                FailedHandling::Keep => {
                                    anyhow::bail!(err_msg);
                                }
                            }
                        }
                    };

                    let json = if args.pretty {
                        serde_json::to_string_pretty(&case)
                    } else {
                        serde_json::to_string(&case)
                    }
                    .context("failed to serialize evaluation case as JSON")?;

                    // Only add unique samples (different split points may produce same result)
                    if seen_samples.insert(json.clone()) {
                        output_lines.push(json);
                    }
                }
            } else {
                let case = match generate_evaluation_example_from_ordered_commit(
                    &annotated.reordered_commit,
                    &annotated.repo_url,
                    &annotated.commit_sha,
                    split_point.clone(),
                    args.seed,
                    None,
                ) {
                    Ok(case) => case,
                    Err(e) => {
                        let err_msg = format!(
                            "failed to generate evaluation example for commit {} at line {}: {}",
                            annotated.commit_sha,
                            line_num + 1,
                            e
                        );
                        if e.is::<NoMatchingSplitPointError>() {
                            eprintln!("skipping: {}", err_msg);
                            continue;
                        }
                        match failed {
                            FailedHandling::Skip | FailedHandling::SkipNoFiles => {
                                eprintln!("{}", err_msg);
                                continue;
                            }
                            FailedHandling::Keep => {
                                anyhow::bail!(err_msg);
                            }
                        }
                    }
                };

                let json = if args.pretty {
                    serde_json::to_string_pretty(&case)
                } else {
                    serde_json::to_string(&case)
                }
                .context("failed to serialize evaluation case as JSON")?;

                output_lines.push(json);
            }

            processed_commits += 1;
            eprint!(
                "\rsplit-commit: processed {} commits, generated {} examples",
                processed_commits,
                output_lines.len()
            );
            io::stderr()
                .flush()
                .context("failed to flush progress to stderr")?;
        }
    }

    if processed_commits > 0 {
        eprintln!();
    }

    let output_content = output_lines.join("\n") + if output_lines.is_empty() { "" } else { "\n" };

    if let Some(path) = output_path {
        fs::write(path, &output_content)
            .with_context(|| format!("failed to write output to {}", path.display()))?;
    } else {
        io::stdout()
            .write_all(output_content.as_bytes())
            .context("failed to write to stdout")?;
    }

    Ok(())
}
