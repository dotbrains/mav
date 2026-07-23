use super::*;

/// Create a directory symlink (`link` -> `target`) in a cross-platform way.
///
/// Returns `Err` when the platform cannot create symlinks (e.g. Windows without
/// the create-symlink privilege), so callers can skip a scenario gracefully
fn make_dir_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_dir(target, link)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (target, link);
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "symlinks are not supported on this platform",
        ))
    }
}

/// Waits up to `timeout` for `events` to deliver something that covers the
/// written file: either a path event whose path satisfies `path_matches`, or a
/// `Rescan` (which tells the consumer to re-scan this watcher's whole tree, so
/// it would discover the file anyway). Returns `false` if nothing relevant
/// arrives before the timeout.
async fn watcher_delivered_event(
    events: &mut (impl futures::Stream<Item = Vec<PathEvent>> + Unpin),
    executor: &BackgroundExecutor,
    timeout: Duration,
    path_matches: &(dyn Fn(&Path) -> bool + Send + Sync),
) -> bool {
    let timeout = executor.timer(timeout).fuse();
    futures::pin_mut!(timeout);
    loop {
        futures::select_biased! {
            batch = events.next().fuse() => {
                let Some(batch) = batch else { return false };
                let covered = batch.iter().any(|event| {
                    path_matches(&event.path) || event.kind == Some(PathEventKind::Rescan)
                });
                if covered {
                    return true;
                }
            }
            _ = timeout => return false,
        }
    }
}

/// Exercises a spread of real watchers whose registered watch path is spelled
/// differently from the path the OS reports events under. Each scenario watches
/// some directory and then mutates the on-disk file; a correct watcher must
/// deliver an event (or a rescan) for every scenario.
///
/// This asserts the residual path-aliasing bugs that real-casing the watch root
/// at add-time does NOT fix. The headline failure is `symlink_ancestor`:
/// watching a path that traverses a symlinked ancestor. On macOS FSEvents
/// reports events under the resolved real path, which no longer has the
/// symlinked prefix the watch root was registered with, so the events are
/// filtered out and never delivered.
///
/// Platform notes (the test runs everywhere but scenarios self-skip when they
/// cannot apply):
/// - Case scenarios require a case-insensitive filesystem (macOS/Windows
///   default; case-sensitive Linux/APFS skip them).
/// - Symlink scenarios require symlink creation (skipped on Windows without the
///   privilege).
/// - `symlink_ancestor` fails on macOS (FSEvents canonicalizes) but is expected
///   to pass on Linux (notify reconstructs paths from the watch path you pass),
///   which is itself a useful demonstration that this is an FSEvents-specific
///   bug.
#[gpui::test]
async fn test_realfs_watch_aliased_watch_paths_deliver_events(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();

    let fs = RealFs::new(None, executor.clone());
    let temp_dir = TempDir::new().expect("create temp dir");
    let root = temp_dir.path().to_path_buf();
    let latency = Duration::from_millis(10);

    // Probe the real filesystem for case sensitivity rather than guessing from
    // the platform.
    std::fs::create_dir_all(root.join("CaseProbe")).expect("create case probe dir");
    let case_insensitive = root.join("caseprobe").exists();
    eprintln!("filesystem is case-insensitive: {case_insensitive}");

    struct Scenario {
        name: &'static str,
        events: Pin<Box<dyn Send + futures::Stream<Item = Vec<PathEvent>>>>,
        _watcher: Arc<dyn Watcher>,
        path_matches: Box<dyn Fn(&Path) -> bool + Send + Sync>,
        action: Option<Box<dyn FnOnce() + Send>>,
    }

    let mut scenarios: Vec<Scenario> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();

    // --- Headline residual bug: watch path traverses a symlinked ancestor. ---
    {
        let real = root.join("ancestor_real");
        let inner = real.join("inner");
        std::fs::create_dir_all(&inner).expect("create symlinked-ancestor target");
        let link = root.join("ancestor_link");
        match make_dir_symlink(&real, &link) {
            Ok(()) => {
                let (events, watcher) = fs.watch(&link.join("inner"), latency).await;
                let file = inner.join("symlink_ancestor.txt");
                scenarios.push(Scenario {
                    name: "symlink_ancestor",
                    events,
                    _watcher: watcher,
                    path_matches: Box::new(|path| {
                        path.ends_with(Path::new("inner/symlink_ancestor.txt"))
                    }),
                    action: Some(Box::new(move || {
                        std::fs::write(&file, b"x").expect("write symlink-ancestor file");
                    })),
                });
            }
            Err(error) => skipped.push(format!("symlink_ancestor (cannot symlink: {error})")),
        }
    }

    // --- Control: watching a symlinked root IS handled (RealFs::watch follows
    //     the root symlink and also watches the target). ---
    {
        let real = root.join("root_real");
        std::fs::create_dir_all(&real).expect("create symlinked-root target");
        let link = root.join("root_link");
        match make_dir_symlink(&real, &link) {
            Ok(()) => {
                let (events, watcher) = fs.watch(&link, latency).await;
                let file = real.join("symlink_root.txt");
                scenarios.push(Scenario {
                    name: "symlink_root",
                    events,
                    _watcher: watcher,
                    path_matches: Box::new(|path| path.ends_with(Path::new("symlink_root.txt"))),
                    action: Some(Box::new(move || {
                        std::fs::write(&file, b"x").expect("write symlink-root file");
                    })),
                });
            }
            Err(error) => skipped.push(format!("symlink_root (cannot symlink: {error})")),
        }
    }

    // --- Control: wrong-case watch root (the originally-reported bug, which the
    //     real-casing fix already addresses). ---
    if case_insensitive {
        let real = root.join("CaseAlpha");
        std::fs::create_dir_all(&real).expect("create wrong-case root");
        let lower = PathBuf::from(real.to_string_lossy().to_lowercase());
        let (events, watcher) = fs.watch(&lower, latency).await;
        let file = real.join("alpha.txt");
        scenarios.push(Scenario {
            name: "wrong_case_root",
            events,
            _watcher: watcher,
            path_matches: Box::new(|path| path.ends_with(Path::new("alpha.txt"))),
            action: Some(Box::new(move || {
                std::fs::write(&file, b"x").expect("write wrong-case-root file");
            })),
        });
    } else {
        skipped.push("wrong_case_root (case-sensitive fs)".to_owned());
    }

    // --- Control: wrong-case nested watch path. ---
    if case_insensitive {
        let real = root.join("CaseBravo").join("Inner");
        std::fs::create_dir_all(&real).expect("create wrong-case nested dir");
        let lower = PathBuf::from(real.to_string_lossy().to_lowercase());
        let (events, watcher) = fs.watch(&lower, latency).await;
        let file = real.join("bravo.txt");
        scenarios.push(Scenario {
            name: "nested_wrong_case",
            events,
            _watcher: watcher,
            path_matches: Box::new(|path| path.ends_with(Path::new("bravo.txt"))),
            action: Some(Box::new(move || {
                std::fs::write(&file, b"x").expect("write nested-wrong-case file");
            })),
        });
    } else {
        skipped.push("nested_wrong_case (case-sensitive fs)".to_owned());
    }

    // --- Residual bug: the watched root is renamed to a different casing after
    //     the watch is established, so later events arrive under a spelling the
    //     registered (old-case) root no longer matches. ---
    if case_insensitive {
        let real = root.join("CaseEcho");
        std::fs::create_dir_all(&real).expect("create case-rename dir");
        let (events, watcher) = fs.watch(&real, latency).await;
        let renamed = root.join("CASEECHO");
        let file = renamed.join("echo.txt");
        scenarios.push(Scenario {
            name: "case_rename_root",
            events,
            _watcher: watcher,
            path_matches: Box::new(|path| path.ends_with(Path::new("echo.txt"))),
            action: Some(Box::new(move || {
                std::fs::rename(&real, &renamed).expect("case-only rename of watched root");
                std::fs::write(&file, b"x").expect("write case-rename file");
            })),
        });
    } else {
        skipped.push("case_rename_root (case-sensitive fs)".to_owned());
    }

    // Let every watch settle before mutating, then perform all mutations.
    executor.timer(Duration::from_millis(250)).await;
    for scenario in &mut scenarios {
        if let Some(action) = scenario.action.take() {
            action();
        }
    }

    let mut failures = Vec::new();
    for scenario in &mut scenarios {
        let delivered = watcher_delivered_event(
            &mut scenario.events,
            &executor,
            Duration::from_secs(3),
            scenario.path_matches.as_ref(),
        )
        .await;
        eprintln!("scenario {}: delivered={delivered}", scenario.name);
        if !delivered {
            failures.push(scenario.name);
        }
    }

    for name in &skipped {
        eprintln!("scenario skipped: {name}");
    }

    assert!(
        failures.is_empty(),
        "watchers failed to deliver events for {failures:?} (skipped: {skipped:?})"
    );
}

#[gpui::test]
#[ignore = "stress test; run explicitly when needed"]
async fn test_realfs_watch_stress_reports_missed_paths(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    const FILE_COUNT: usize = 32000;
    cx.executor().allow_parking();

    let fs = RealFs::new(None, executor.clone());
    let temp_dir = TempDir::new().expect("create temp dir");
    let root = temp_dir.path();

    let mut file_paths = Vec::with_capacity(FILE_COUNT);
    let mut expected_paths = BTreeSet::new();

    for index in 0..FILE_COUNT {
        let dir_path = root.join(format!("dir-{index:04}"));
        let file_path = dir_path.join("file.txt");
        fs.create_dir(&dir_path).await.expect("create watched dir");
        fs.write(&file_path, b"before")
            .await
            .expect("create initial file");
        expected_paths.insert(file_path.clone());
        file_paths.push(file_path);
    }

    let (mut events, watcher) = fs.watch(root, Duration::from_millis(10)).await;
    let _watcher = watcher;

    for file_path in &expected_paths {
        _watcher
            .add(file_path.parent().expect("file has parent"))
            .expect("add explicit directory watch");
    }

    for (index, file_path) in file_paths.iter().enumerate() {
        let content = format!("after-{index}");
        fs.write(file_path, content.as_bytes())
            .await
            .expect("modify watched file");
    }

    let mut changed_paths = BTreeSet::new();
    let mut rescan_count: u32 = 0;
    let timeout = executor.timer(Duration::from_secs(10)).fuse();

    futures::pin_mut!(timeout);

    let mut ticks = 0;
    while ticks < 1000 {
        if let Some(batch) = events.next().fuse().now_or_never().flatten() {
            for event in batch {
                if event.kind == Some(PathEventKind::Rescan) {
                    rescan_count += 1;
                }
                if expected_paths.contains(&event.path) {
                    changed_paths.insert(event.path);
                }
            }
            if changed_paths.len() == expected_paths.len() {
                break;
            }
            ticks = 0;
        } else {
            ticks += 1;
            executor.timer(Duration::from_millis(10)).await;
        }
    }

    let missed_paths: BTreeSet<_> = expected_paths.difference(&changed_paths).cloned().collect();

    eprintln!(
        "realfs watch stress: expected={}, observed={}, missed={}, rescan={}",
        expected_paths.len(),
        changed_paths.len(),
        missed_paths.len(),
        rescan_count
    );

    assert!(
        missed_paths.is_empty() || rescan_count > 0,
        "missed {} paths without rescan being reported",
        missed_paths.len()
    );
}
