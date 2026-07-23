use super::*;

mod special_commands;

pub(crate) fn run() {
    let args = EpArgs::parse();

    if args.printenv {
        ::util::shell_env::print_env();
        return;
    }

    let output = args.output_path();

    if args.markdown && output.is_none() {
        eprintln!("--markdown requires -o to specify the output directory");
        std::process::exit(1);
    }

    let command = match &args.command {
        Some(cmd) => cmd.clone(),
        None => {
            EpArgs::command().print_help().unwrap();
            return;
        }
    };

    if special_commands::handle(&args, output.as_ref(), &command) {
        return;
    }

    let http_client = Arc::new(ReqwestClient::new());
    let app = gpui_platform::headless().with_http_client(http_client);

    app.run(move |cx| {
        let app_state = Arc::new(headless::init(cx));
        EditPredictionStore::global(&app_state.client, &app_state.user_store, cx);

        cx.spawn(async move |cx| {
            let result = async {
                let examples = load_examples(
                    app_state.client.http_client(),
                    &args,
                    output.as_ref(),
                    cx.background_executor().clone(),
                )
                .await?;

                match &command {
                    Command::Predict(args) | Command::Score(args) => {
                        predict::sync_batches(args.provider.as_ref()).await?;
                    }
                    Command::Eval(args) => {
                        if !args.context_only {
                            predict::sync_batches(args.predict.provider.as_ref()).await?;
                        }
                    }
                    Command::Qa(args) => {
                        qa::sync_batches(args).await?;
                    }
                    Command::Repair(args) => {
                        repair::sync_batches(args).await?;
                    }
                    _ => (),
                }

                let failfast_on_single_example = examples.len() == 1;

                // For --markdown mode, create the output directory if it doesn't exist
                if args.markdown {
                    let dir = output.as_ref().expect("--markdown requires -o");
                    if !dir.exists() {
                        std::fs::create_dir_all(dir)
                            .expect("Failed to create markdown output directory");
                    }
                }

                // Set up JSONL output writer (not used in markdown mode)
                let mut output_sender: Option<mpsc::UnboundedSender<String>> = None;
                let mut in_place_temp_path: Option<PathBuf> = None;
                if !args.markdown
                    && let Some(output_path) = output.as_ref()
                {
                    let write_path = if args.in_place {
                        let temp = output_path.with_extension("jsonl.tmp");
                        in_place_temp_path = Some(temp.clone());
                        temp
                    } else {
                        output_path.clone()
                    };

                    let file = OpenOptions::new()
                        .create(true)
                        .write(true)
                        .truncate(args.in_place)
                        .append(!args.in_place)
                        .open(&write_path)
                        .expect("Failed to open output file");

                    let mut writer = BufWriter::new(file);
                    let (sender, mut receiver) = mpsc::unbounded::<String>();
                    cx.background_spawn(async move {
                        while let Some(line) = receiver.next().await {
                            writeln!(writer, "{}", line).expect("Failed to write example");
                            writer.flush().expect("Failed to flush output");
                        }
                    })
                    .detach();
                    output_sender = Some(sender);
                }

                let example_batches = if args.group_by_repo {
                    group_examples_by_repo(examples)
                } else {
                    chunk_examples(examples, args.max_parallelism)
                };
                let example_batches = Mutex::new(example_batches);
                let finished_examples = Mutex::new(Vec::new());

                let mut tasks = Vec::new();
                for _ in 0..args.max_parallelism {
                    tasks.push(async {
                        loop {
                            let Some(mut repo_examples) =
                                example_batches.lock().unwrap().pop_front()
                            else {
                                break;
                            };
                            for example in &mut repo_examples {
                                let example_progress =
                                    Progress::global().start_group(&example.spec.name);

                                let result = async {
                                    match &command {
                                        Command::Read(_) => {}
                                        Command::LoadProject => {
                                            run_load_project(
                                                example,
                                                app_state.clone(),
                                                &example_progress,
                                                cx.clone(),
                                            )
                                            .await?;
                                        }
                                        Command::Context(args) => {
                                            run_context_retrieval(
                                                example,
                                                app_state.clone(),
                                                &example_progress,
                                                args.context_types(),
                                                args.force,
                                                cx.clone(),
                                            )
                                            .await?;
                                        }
                                        Command::FormatPrompt(args) => {
                                            run_format_prompt(
                                                example,
                                                args,
                                                app_state.clone(),
                                                &example_progress,
                                                cx.clone(),
                                            )
                                            .await?;
                                        }
                                        Command::Predict(args) => {
                                            run_prediction(
                                                example,
                                                args,
                                                app_state.clone(),
                                                &example_progress,
                                                cx.clone(),
                                            )
                                            .await?;
                                        }
                                        Command::ParseOutput => {
                                            parse_output::run_parse_output(example)?;
                                        }
                                        Command::Distill => {
                                            run_distill(example).await?;
                                        }
                                        Command::Score(args) => {
                                            run_scoring(
                                                example,
                                                args,
                                                app_state.clone(),
                                                &example_progress,
                                                cx.clone(),
                                                false,
                                                None,
                                                None,
                                            )
                                            .await?;
                                        }
                                        Command::Eval(args) => {
                                            let context_source_filter =
                                                args.context_source_filter();
                                            if args.context_only {
                                                score::run_context_coverage_scoring(
                                                    example,
                                                    &example_progress,
                                                    Some(args.related_context_limit * 3),
                                                    context_source_filter.as_deref(),
                                                )?;
                                            } else {
                                                run_scoring(
                                                    example,
                                                    &args.predict,
                                                    app_state.clone(),
                                                    &example_progress,
                                                    cx.clone(),
                                                    true,
                                                    Some(args.related_context_limit * 3),
                                                    context_source_filter,
                                                )
                                                .await?;
                                            }
                                        }
                                        Command::Qa(args) => {
                                            qa::run_qa(example, args, &example_progress).await?;
                                        }
                                        Command::Repair(args) => {
                                            repair::run_repair(example, args, &example_progress)
                                                .await?;
                                        }
                                        Command::Clean
                                        | Command::Synthesize(_)
                                        | Command::SplitCommit(_)
                                        | Command::Split(_)
                                        | Command::TruncatePatch(_)
                                        | Command::FilterLanguages(_)
                                        | Command::ImportBatch(_)
                                        | Command::PrintZetaFormats => {
                                            unreachable!()
                                        }
                                    }
                                    anyhow::Ok(())
                                }
                                .await;

                                let failed = if let Err(error) = result {
                                    handle_error(
                                        error,
                                        &args,
                                        &command,
                                        &app_state,
                                        failfast_on_single_example,
                                        &example,
                                    )
                                    .await;
                                    true
                                } else {
                                    false
                                };

                                let should_write = !failed || args.failed == FailedHandling::Keep;
                                if should_write {
                                    if args.markdown {
                                        let markdown_dir =
                                            output.as_ref().expect("--markdown requires -o");
                                        let filename = format!("{}.md", example.spec.filename());
                                        let path = markdown_dir.join(&filename);
                                        let markdown = example.spec.to_markdown();
                                        std::fs::write(&path, &markdown)
                                            .expect("Failed to write markdown file");
                                    } else if let Some(ref mut sender) = output_sender.clone() {
                                        let line = serde_json::to_string(&example).unwrap();
                                        sender
                                            .send(line)
                                            .await
                                            .expect("Failed to send to output writer");
                                    } else if args.output.is_none()
                                        && !matches!(command, Command::Eval(_))
                                    {
                                        let line = serde_json::to_string(&example).unwrap();
                                        println!("{}", line);
                                    }
                                }
                            }

                            let project = repo_examples
                                .iter()
                                .find_map(|e| e.state.as_ref().map(|s| s.project.clone()));

                            if let Some(project) = project {
                                let mut cx = cx.clone();

                                let shutdown_task: Task<()> =
                                    project.update(&mut cx, |project, cx| {
                                        let lsp_store = project.lsp_store();
                                        lsp_store.update(cx, |lsp_store, cx| {
                                            lsp_store.shutdown_all_language_servers(cx)
                                        })
                                    });

                                shutdown_task.await;

                                if let Some(ep_store) =
                                    cx.update(|cx| EditPredictionStore::try_global(cx))
                                {
                                    ep_store.update(&mut cx, |store, _| {
                                        store.remove_project(&project);
                                    });
                                }
                            }

                            for example in &mut repo_examples {
                                example.state.take();
                            }
                            finished_examples
                                .lock()
                                .unwrap()
                                .extend_from_slice(&repo_examples);
                        }
                    });
                }
                futures::future::join_all(tasks).await;

                Progress::global().finalize();

                let is_markdown = args.markdown;
                let write_path = in_place_temp_path.as_ref().or(output.as_ref());
                match &command {
                    Command::Predict(args) | Command::Score(args) => {
                        predict::sync_batches(args.provider.as_ref()).await?;
                        if args.wait {
                            predict::wait_for_batches(args.provider.as_ref()).await?;
                            let mut examples =
                                std::mem::take(&mut *finished_examples.lock().unwrap());
                            predict::reprocess_after_batch_wait(&mut examples, args).await?;
                            rewrite_output(&examples, write_path, is_markdown)?;
                            *finished_examples.lock().unwrap() = examples;
                        }
                    }
                    Command::Eval(args) => {
                        if !args.context_only {
                            predict::sync_batches(args.predict.provider.as_ref()).await?;
                            if args.predict.wait {
                                predict::wait_for_batches(args.predict.provider.as_ref()).await?;
                                let mut examples =
                                    std::mem::take(&mut *finished_examples.lock().unwrap());
                                predict::reprocess_after_batch_wait(&mut examples, &args.predict)
                                    .await?;
                                rewrite_output(&examples, write_path, is_markdown)?;
                                *finished_examples.lock().unwrap() = examples;
                            }
                        }
                    }
                    Command::Qa(args) => {
                        qa::sync_batches(args).await?;
                    }
                    Command::Repair(args) => {
                        repair::sync_batches(args).await?;
                        if args.wait {
                            repair::wait_for_batches(args).await?;
                            let mut examples =
                                std::mem::take(&mut *finished_examples.lock().unwrap());
                            repair::reprocess_after_batch_wait(&mut examples, args).await?;
                            rewrite_output(&examples, write_path, is_markdown)?;
                            *finished_examples.lock().unwrap() = examples;
                        }
                    }
                    _ => (),
                }

                match &command {
                    Command::Eval(args) => {
                        let examples = finished_examples.lock().unwrap();
                        let context_source_filter = args.context_source_filter();
                        score::print_report(
                            &examples,
                            args.verbose,
                            args.context_only,
                            Some(args.related_context_limit * 3),
                            context_source_filter.as_deref(),
                        );
                        if let Some(summary_path) = &args.summary_json {
                            score::write_summary_json(
                                &examples,
                                summary_path,
                                Some(args.related_context_limit * 3),
                                context_source_filter.as_deref(),
                            )?;
                        }
                    }
                    Command::Repair(args) => {
                        let examples = finished_examples.lock().unwrap();
                        repair::print_report(&examples, args.confidence_threshold);
                    }
                    _ => (),
                };

                // For --in-place, atomically rename temp file to original
                if let Some(temp_path) = &in_place_temp_path {
                    let final_path = output.as_ref().expect("in_place_temp_path requires output");
                    std::fs::rename(temp_path, final_path)
                        .expect("Failed to rename temp file to final output");
                }

                anyhow::Ok(())
            }
            .await;

            if let Err(e) = result {
                panic!("Fatal error: {:?}", e);
            }

            let _ = cx.update(|cx| cx.quit());
        })
        .detach();
    });
}
