use super::*;

pub(super) fn handle(args: &EpArgs, output: Option<&PathBuf>, command: &Command) -> bool {
    match command {
        Command::ImportBatch(import_args) => {
            gpui::block_on(async {
                match import_args.provider {
                    BatchProvider::Anthropic => {
                        let client = anthropic_client::AnthropicClient::batch(&paths::LLM_CACHE_DB)
                            .expect("Failed to create Anthropic client");
                        if let Err(e) = client.import_batches(&import_args.batch_ids).await {
                            eprintln!("Error importing Anthropic batches: {:?}", e);
                            std::process::exit(1);
                        }
                    }
                    BatchProvider::Openai => {
                        let client = openai_client::OpenAiClient::batch(&paths::LLM_CACHE_DB)
                            .expect("Failed to create OpenAI client");
                        if let Err(e) = client.import_batches(&import_args.batch_ids).await {
                            eprintln!("Error importing OpenAI batches: {:?}", e);
                            std::process::exit(1);
                        }
                    }
                }
                println!(
                    "Successfully imported {} batch(es)",
                    import_args.batch_ids.len()
                );
            });
            true
        }
        Command::Clean => {
            std::fs::remove_dir_all(&*paths::DATA_DIR).unwrap();
            true
        }
        Command::PrintZetaFormats => {
            use strum::IntoEnumIterator as _;
            for format in ZetaFormat::iter() {
                println!("{}", format.to_string().to_lowercase());
            }
            true
        }
        Command::Synthesize(synth_args) => {
            let output_dir = if let Some(output_dir) = args.output.clone() {
                output_dir
            } else {
                let default_output_dir = env::current_dir()
                    .unwrap()
                    .join("crates/edit_prediction_cli/evals-generated");
                if default_output_dir.parent().unwrap().exists() {
                    std::fs::create_dir(&default_output_dir).ok();
                    default_output_dir
                } else {
                    panic!("output dir is required");
                }
            };
            let config = SynthesizeConfig {
                repo_urls: synth_args.repos.clone(),
                count: synth_args.count,
                max_commits: synth_args.max_commits,
                output_dir,
                fresh: synth_args.fresh,
            };
            gpui::block_on(async {
                if let Err(e) = run_synthesize(config).await {
                    eprintln!("Error: {:?}", e);
                    std::process::exit(1);
                }
            });
            true
        }
        Command::SplitCommit(split_commit_args) => {
            if let Err(error) =
                split_commit::run_split_commit(split_commit_args, &args.inputs, output, args.failed)
            {
                eprintln!("{error:#}");
                std::process::exit(1);
            }
            true
        }
        Command::TruncatePatch(truncate_args) => {
            if let Err(error) =
                truncate_expected_patch::run_truncate_expected_patch(truncate_args, &args.inputs)
            {
                eprintln!("{error:#}");
                std::process::exit(1);
            }
            true
        }
        Command::Split(split_args) => {
            if let Err(error) = split_dataset::run_split(split_args, &args.inputs) {
                eprintln!("{error:#}");
                std::process::exit(1);
            }
            true
        }
        Command::FilterLanguages(filter_args) => {
            if let Err(error) =
                run_filter_languages(filter_args, &args.inputs, args.output.as_ref())
            {
                eprintln!("{error:#}");
                std::process::exit(1);
            }
            true
        }
        _ => false,
    }
}
