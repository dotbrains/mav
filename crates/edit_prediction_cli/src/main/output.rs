use super::*;

pub(crate) fn rewrite_output(
    examples: &[Example],
    output_path: Option<&PathBuf>,
    markdown: bool,
) -> anyhow::Result<()> {
    if markdown {
        let dir = output_path.context("--markdown requires -o")?;
        for example in examples {
            let filename = format!("{}.md", example.spec.filename());
            let path = dir.join(&filename);
            let markdown = example.spec.to_markdown();
            std::fs::write(&path, &markdown).context("Failed to write markdown file")?;
        }
    } else if let Some(path) = output_path {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)
            .context("Failed to open output file for rewriting")?;
        let mut writer = BufWriter::new(file);
        for example in examples {
            let line = serde_json::to_string(example)?;
            writeln!(writer, "{}", line)?;
        }
        writer.flush()?;
    } else {
        for example in examples {
            let line = serde_json::to_string(example)?;
            println!("{}", line);
        }
    }
    Ok(())
}

pub(crate) async fn handle_error(
    error: anyhow::Error,
    args: &EpArgs,
    command: &Command,
    app_state: &Arc<headless::EpAppState>,
    failfast_on_single_example: bool,
    example: &Example,
) {
    Progress::global().increment_failed();

    let msg;
    if !matches!(args.failed, FailedHandling::SkipNoFiles) {
        let example_name = example.spec.filename();

        let failed_example_path = FAILED_EXAMPLES_DIR.join(format!("{}.json", example_name));
        app_state
            .fs
            .write(
                &failed_example_path,
                &serde_json::to_vec_pretty(&example).unwrap(),
            )
            .await
            .unwrap();
        let err_path = FAILED_EXAMPLES_DIR.join(format!("{}_err.txt", example_name));
        app_state
            .fs
            .write(&err_path, format!("{error:?}").as_bytes())
            .await
            .unwrap();

        let failed_jsonl_path = RUN_DIR.join("failed.jsonl");
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&failed_jsonl_path)
            .expect("Failed to open failed.jsonl");
        writeln!(file, "{}", serde_json::to_string(example).unwrap())
            .expect("Failed to write to failed.jsonl");

        let cursor_path = match example.repo_name() {
            Ok(repo_name) => repo_name.worktree_path().join(&example.spec.cursor_path),
            Err(_) => example.spec.cursor_path.as_ref().to_path_buf(),
        };
        msg = format!(
            indoc::indoc! {"
                While processing \"{}\":

                \x1b[31m{:?}\x1b[0m

                Example:        \x1b[36m{}\x1b[0m
                Error file:     \x1b[36m{}\x1b[0m
                Cursor file:    \x1b[36m{}\x1b[0m
                Re-run:         cargo run -p edit_prediction_cli -- {} \x1b[36m{}\x1b[0m
            "},
            example.spec.name,
            error,
            failed_example_path.display(),
            err_path.display(),
            cursor_path.display(),
            command,
            failed_example_path.display(),
        );
    } else {
        msg = format!(
            indoc::indoc! {"
            While processing \"{}\":

                \x1b[31m{:?}\x1b[0m
            "},
            example.spec.name, error
        );
    }

    if args.failfast || failfast_on_single_example {
        Progress::global().finalize();
        panic!("{}", msg);
    } else {
        log::error!("{}", msg);
    }
}
