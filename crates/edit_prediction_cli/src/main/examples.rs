use super::*;

pub(crate) fn deduplicate_examples(examples: &mut Vec<Example>, max_per_cluster: usize) {
    let total_before_exact = examples.len();
    let mut seen_positions = HashSet::default();
    examples.retain(|example| seen_positions.insert(example.spec.cursor_position.clone()));
    log::info!(
        "exact duplicate filter: {total_before_exact} examples → {} examples ({} removed)",
        examples.len(),
        total_before_exact - examples.len(),
    );

    const JACCARD_THRESHOLD: f64 = 0.5;
    const NUM_HASHES: usize = 128;
    const TOKEN_NGRAM_SIZE: usize = 5;

    let (num_bands, band_width) = calculate_minhash_params(JACCARD_THRESHOLD, NUM_HASHES);
    let num_hashes = num_bands * band_width;
    let minhasher = MinHasher32::new(num_hashes);
    let mut index: MinHashIndex<u32, usize> =
        MinHashIndex::new(num_bands, band_width, JACCARD_THRESHOLD);

    let signatures: Vec<Vec<u32>> = examples
        .iter()
        .map(|example| {
            let shingles = code_token_ngrams(&example.spec.cursor_position, TOKEN_NGRAM_SIZE);
            minhasher.create_signature(shingles.iter())
        })
        .collect();

    for (id, signature) in signatures.iter().enumerate() {
        index.insert(id, signature.clone());
    }

    // Build clusters via union-find on LSH candidate pairs.
    let mut parent: Vec<usize> = (0..examples.len()).collect();

    fn find(parent: &mut Vec<usize>, mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }

    for (id, signature) in signatures.iter().enumerate() {
        for candidate in index.query_owned(signature) {
            let (a, b) = (find(&mut parent, id), find(&mut parent, candidate));
            if a != b {
                parent[a] = b;
            }
        }
    }

    let mut clusters: HashMap<usize, Vec<usize>> = HashMap::default();
    for id in 0..examples.len() {
        clusters.entry(find(&mut parent, id)).or_default().push(id);
    }

    let mut keep: HashSet<usize> = HashSet::default();
    for members in clusters.values() {
        let selected = greedy_max_min_diverse(members, &signatures, max_per_cluster);
        keep.extend(selected);
    }

    let total = examples.len();
    let mut kept_indices: Vec<usize> = keep.into_iter().collect();
    kept_indices.sort();

    let mut retained = Vec::with_capacity(kept_indices.len());
    for index in kept_indices.into_iter().rev() {
        retained.push(examples.swap_remove(index));
    }
    retained.reverse();

    *examples = retained;
    log::info!(
        "near-duplicate filter: {total} examples → {} examples ({} removed)",
        examples.len(),
        total - examples.len(),
    );
}

fn greedy_max_min_diverse(members: &[usize], signatures: &[Vec<u32>], k: usize) -> Vec<usize> {
    if members.len() <= k {
        return members.to_vec();
    }

    let mut selected = vec![members[0]];
    let mut min_dist: HashMap<usize, f64> = HashMap::default();
    for &member in &members[1..] {
        let dist = 1.0 - compute_minhash_similarity(&signatures[selected[0]], &signatures[member]);
        min_dist.insert(member, dist);
    }

    while selected.len() < k {
        let &best = min_dist
            .iter()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(id, _)| id)
            .expect("min_dist should not be empty when selected.len() < k");
        selected.push(best);
        min_dist.remove(&best);

        let best_sig = &signatures[best];
        for (member, current_min) in min_dist.iter_mut() {
            let dist = 1.0 - compute_minhash_similarity(best_sig, &signatures[*member]);
            if dist < *current_min {
                *current_min = dist;
            }
        }
    }

    selected
}

fn code_token_ngrams(code: &str, ngram_size: usize) -> Vec<String> {
    let tokens: Vec<&str> = word_diff::tokenize(code)
        .into_iter()
        .filter(|t| !t.trim().is_empty())
        .collect();

    if tokens.len() < ngram_size {
        return vec![tokens.join("\0")];
    }

    tokens
        .windows(ngram_size)
        .map(|window| window.join("\0"))
        .collect()
}

pub(crate) async fn load_examples(
    http_client: Arc<dyn http_client::HttpClient>,
    args: &EpArgs,
    output_path: Option<&PathBuf>,
    background_executor: BackgroundExecutor,
) -> anyhow::Result<Vec<Example>> {
    let mut captured_after_timestamps = Vec::new();
    let mut rejected_after_timestamps = Vec::new();
    let mut requested_after_timestamps = Vec::new();
    let mut settled_after_timestamps = Vec::new();
    let mut rated_after_inputs: Vec<(String, Option<telemetry_events::EditPredictionRating>)> =
        Vec::new();
    let mut accepted_after_timestamps = Vec::new();
    let mut file_inputs = Vec::new();

    for input in &args.inputs {
        let input_string = input.to_string_lossy();
        if let Some(timestamp) = pull_examples::parse_captured_after_input(input_string.as_ref()) {
            captured_after_timestamps.push(timestamp.to_string());
        } else if let Some((explicit, timestamp)) =
            pull_examples::parse_rejected_after_input(input_string.as_ref())
        {
            rejected_after_timestamps.push((explicit, timestamp.to_string()));
        } else if let Some(timestamp) =
            pull_examples::parse_accepted_after_input(input_string.as_ref())
        {
            accepted_after_timestamps.push(timestamp.to_string());
        } else if let Some(timestamp) =
            pull_examples::parse_requested_after_input(input_string.as_ref())
        {
            requested_after_timestamps.push(timestamp.to_string());
        } else if let Some(timestamp) = parse_settled_after_input(input_string.as_ref()) {
            settled_after_timestamps.push(timestamp.to_string());
        } else if let Some((timestamp, rating_filter)) =
            pull_examples::parse_rated_after_input(input_string.as_ref())
        {
            rated_after_inputs.push((timestamp.to_string(), rating_filter));
        } else {
            file_inputs.push(input.clone());
        }
    }

    let mut examples = read_example_files(&file_inputs);

    // Apply offset to file examples first, then pass remaining offset to Snowflake.
    let file_example_count = examples.len();
    let remaining_offset = if let Some(offset) = args.offset {
        if offset >= file_example_count {
            examples.clear();
            offset - file_example_count
        } else {
            examples.splice(0..offset, []);
            0
        }
    } else {
        0
    };

    Progress::global().set_total_examples(examples.len());

    let remaining_limit_for_snowflake =
        args.limit.map(|limit| limit.saturating_sub(examples.len()));

    if let Some(0) = remaining_limit_for_snowflake {
        log::info!(
            "skipping Snowflake inputs because --limit is already satisfied by example files"
        );
    } else {
        let max_rows_per_timestamp = remaining_limit_for_snowflake;

        if !rejected_after_timestamps.is_empty() {
            rejected_after_timestamps.sort();

            let mut rejected_examples = pull_examples::fetch_rejected_examples_after(
                http_client.clone(),
                &rejected_after_timestamps,
                max_rows_per_timestamp,
                remaining_offset,
                background_executor.clone(),
                Some(MIN_CAPTURE_VERSION),
            )
            .await?;
            examples.append(&mut rejected_examples);
        }

        if !accepted_after_timestamps.is_empty() {
            accepted_after_timestamps.sort();

            let mut accepted_examples = pull_examples::fetch_accepted_examples_after(
                http_client.clone(),
                &accepted_after_timestamps,
                max_rows_per_timestamp,
                remaining_offset,
                background_executor.clone(),
                Some(MIN_CAPTURE_VERSION),
            )
            .await?;
            examples.append(&mut accepted_examples);
        }

        if !requested_after_timestamps.is_empty() {
            requested_after_timestamps.sort();

            let mut requested_examples = pull_examples::fetch_requested_examples_after(
                http_client.clone(),
                &requested_after_timestamps,
                max_rows_per_timestamp,
                remaining_offset,
                background_executor.clone(),
                Some(MIN_CAPTURE_VERSION),
            )
            .await?;
            examples.append(&mut requested_examples);
        }

        if !captured_after_timestamps.is_empty() {
            captured_after_timestamps.sort();

            let mut captured_examples = pull_examples::fetch_captured_examples_after(
                http_client.clone(),
                &captured_after_timestamps,
                max_rows_per_timestamp,
                remaining_offset,
                background_executor.clone(),
                Some(MIN_CAPTURE_VERSION),
            )
            .await?;
            examples.append(&mut captured_examples);
        }

        if !settled_after_timestamps.is_empty() {
            settled_after_timestamps.sort();

            let mut settled_examples = fetch_settled_examples_after(
                http_client.clone(),
                &settled_after_timestamps,
                max_rows_per_timestamp,
                remaining_offset,
                background_executor.clone(),
                Some(MIN_CAPTURE_VERSION),
            )
            .await?;
            examples.append(&mut settled_examples);
        }

        if !rated_after_inputs.is_empty() {
            rated_after_inputs.sort();

            let mut rated_examples = pull_examples::fetch_rated_examples_after(
                http_client,
                &rated_after_inputs,
                max_rows_per_timestamp,
                remaining_offset,
                background_executor,
                Some(MIN_CAPTURE_VERSION),
            )
            .await?;
            examples.append(&mut rated_examples);
        }
    }

    crate::example::sort_examples_by_repo_and_rev(&mut examples);

    if let Some(name_filter) = &args.name {
        examples.retain(|example| example.spec.name.contains(name_filter));
    }
    if let Some(repo_filter) = &args.repo {
        examples.retain(|example| example.spec.repository_url.contains(repo_filter));
    }

    // Skip resume logic for --in-place since input and output are the same file,
    // which would incorrectly treat all input examples as already processed.
    if !args.in_place {
        if let Some(path) = output_path
            && let Some(command) = &args.command
        {
            resume_from_output(path, &mut examples, command);
        }
    }

    if let Some(max_duplicates) = args.max_duplicates {
        deduplicate_examples(&mut examples, max_duplicates);
    }

    if let Some(limit) = args.limit {
        examples.truncate(limit);
    }

    let progress = Progress::global();
    progress.set_total_examples(examples.len());
    progress.set_max_example_name_len(examples.iter().map(|e| &e.spec.name));

    Ok(examples)
}

fn spec_hash(spec: &edit_prediction::example_spec::ExampleSpec) -> u64 {
    let mut hasher = collections::FxHasher::default();
    spec.hash(&mut hasher);
    hasher.finish()
}

pub(crate) fn chunk_examples(
    examples: Vec<Example>,
    max_parallelism: usize,
) -> VecDeque<Vec<Example>> {
    if examples.is_empty() || max_parallelism == 0 {
        return VecDeque::new();
    }

    let chunk_size = examples.len().div_ceil(max_parallelism);
    examples
        .chunks(chunk_size)
        .map(|chunk| chunk.to_vec())
        .collect()
}

fn resume_from_output(path: &PathBuf, examples: &mut Vec<Example>, command: &Command) {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return,
    };

    let input_hashes: HashSet<u64> = examples.iter().map(|e| spec_hash(&e.spec)).collect();

    let reader = BufReader::new(file);
    let mut kept_lines = Vec::new();
    let mut kept_hashes = HashSet::default();

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };

        if let Ok(output_example) = serde_json::from_str::<Example>(&line) {
            let hash = spec_hash(&output_example.spec);
            if input_hashes.contains(&hash) && !kept_hashes.contains(&hash) {
                let is_complete = match command {
                    Command::Qa(_) => output_example
                        .qa
                        .first()
                        .and_then(|q| q.as_ref())
                        .and_then(|q| q.confidence)
                        .is_some(),
                    Command::Repair(_) => output_example.predictions.iter().any(|p| {
                        p.provider == PredictionProvider::Repair && p.actual_patch.is_some()
                    }),
                    _ => true,
                };
                if is_complete {
                    kept_hashes.insert(hash);
                    kept_lines.push(line);
                }
            }
        }
    }

    let total = examples.len();
    let already_processed = kept_hashes.len();

    eprintln!(
        "Resuming: {}/{} examples already processed",
        already_processed, total
    );

    let file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(path)
        .expect("Failed to open output file for rewriting");
    let mut writer = BufWriter::new(file);
    for line in &kept_lines {
        writeln!(writer, "{}", line).expect("Failed to write to output file");
    }
    writer.flush().expect("Failed to flush output file");

    examples.retain(|e| !kept_hashes.contains(&spec_hash(&e.spec)));
}
