use super::*;

pub(super) fn make_cli_open_request(
    paths: Vec<String>,
    open_behavior: cli::OpenBehavior,
) -> CliRequest {
    CliRequest::Open {
        paths,
        urls: vec![],
        diff_paths: vec![],
        diff_all: false,
        wsl: None,
        wait: false,
        open_behavior,
        env: None,
        user_data_dir: None,
        dev_container: false,
        cwd: None,
    }
}

pub(super) fn make_cli_url_open_request(
    urls: Vec<String>,
    open_behavior: cli::OpenBehavior,
) -> CliRequest {
    CliRequest::Open {
        paths: vec![],
        urls,
        diff_paths: vec![],
        diff_all: false,
        wsl: None,
        wait: false,
        open_behavior,
        env: None,
        user_data_dir: None,
        dev_container: false,
        cwd: None,
    }
}

/// Runs the real [`cli::run_cli_response_loop`] on an OS thread against
/// the Mav-side `handle_cli_connection` on the GPUI foreground executor,
/// using `allow_parking` so the test scheduler tolerates cross-thread
/// wakeups.
///
/// Returns `(exit_status, prompt_was_shown)`.
pub(super) fn run_cli_with_mav_handler(
    cx: &mut TestAppContext,
    app_state: Arc<AppState>,
    open_request: CliRequest,
    prompt_response: Option<cli::CliBehaviorSetting>,
) -> (i32, bool) {
    cx.executor().allow_parking();

    let (request_tx, request_rx) = mpsc::unbounded::<CliRequest>();
    let (response_tx, response_rx) = std::sync::mpsc::channel::<CliResponse>();
    let response_sink: Box<dyn CliResponseSink> = Box::new(SyncResponseSender(response_tx));

    cx.spawn(|mut cx| async move {
        handle_cli_connection((request_rx, response_sink), app_state, &mut cx).await;
    })
    .detach();

    let prompt_called = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let prompt_called_for_thread = prompt_called.clone();

    let cli_thread = std::thread::spawn(move || -> anyhow::Result<i32> {
        request_tx
            .unbounded_send(open_request)
            .map_err(|error| anyhow::anyhow!("{error}"))?;

        while let Ok(response) = response_rx.recv() {
            match response {
                CliResponse::Ping => {}
                CliResponse::Stdout { .. } | CliResponse::Stderr { .. } => {}
                CliResponse::Exit { status } => return Ok(status),
                CliResponse::PromptOpenBehavior => {
                    prompt_called_for_thread.store(true, std::sync::atomic::Ordering::SeqCst);
                    let behavior =
                        prompt_response.unwrap_or(cli::CliBehaviorSetting::ExistingWindow);
                    request_tx
                        .unbounded_send(CliRequest::SetOpenBehavior { behavior })
                        .map_err(|error| anyhow::anyhow!("{error}"))?;
                }
            }
        }

        anyhow::bail!("CLI response channel closed without Exit")
    });

    while !cli_thread.is_finished() {
        cx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    let exit_status = cli_thread.join().unwrap().expect("CLI loop failed");
    let prompt_shown = prompt_called.load(std::sync::atomic::Ordering::SeqCst);

    // Flush any remaining async work (e.g. settings file writes).
    cx.run_until_parked();

    (exit_status, prompt_shown)
}
