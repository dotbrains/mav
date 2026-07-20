use super::*;
use pretty_assertions::assert_eq;

#[gpui::test(iterations = 25)]
async fn test_staging_random_hunks(
    mut rng: StdRng,
    _executor: BackgroundExecutor,
    cx: &mut gpui::TestAppContext,
) {
    let operations = env::var("OPERATIONS")
        .map(|i| i.parse().expect("invalid `OPERATIONS` variable"))
        .unwrap_or(20);

    use DiffHunkSecondaryStatus::*;
    init_test(cx);

    let committed_text = (0..30).map(|i| format!("line {i}\n")).collect::<String>();
    let index_text = committed_text.clone();
    let buffer_text = (0..30)
        .map(|i| match i % 5 {
            0 => format!("line {i} (modified)\n"),
            _ => format!("line {i}\n"),
        })
        .collect::<String>();

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/dir"),
        json!({
            ".git": {},
            "file.txt": buffer_text.clone()
        }),
    )
    .await;
    fs.set_head_for_repo(
        path!("/dir/.git").as_ref(),
        &[("file.txt", committed_text.clone())],
        "deadbeef",
    );
    fs.set_index_for_repo(
        path!("/dir/.git").as_ref(),
        &[("file.txt", index_text.clone())],
    );
    let repo = fs
        .open_repo(path!("/dir/.git").as_ref(), Some("git".as_ref()))
        .unwrap();

    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/dir/file.txt"), cx)
        })
        .await
        .unwrap();
    let snapshot = buffer.read_with(cx, |buffer, _| buffer.snapshot());
    let uncommitted_diff = project
        .update(cx, |project, cx| {
            project.open_uncommitted_diff(buffer.clone(), cx)
        })
        .await
        .unwrap();

    let mut hunks = uncommitted_diff.update(cx, |diff, cx| {
        diff.snapshot(cx).hunks(&snapshot).collect::<Vec<_>>()
    });
    assert_eq!(hunks.len(), 6);

    for _i in 0..operations {
        let hunk_ix = rng.random_range(0..hunks.len());
        let hunk = &mut hunks[hunk_ix];
        let row = hunk.range.start.row;

        if hunk.status().has_secondary_hunk() {
            log::info!("staging hunk at {row}");
            uncommitted_diff.update(cx, |diff, cx| {
                diff.stage_or_unstage_hunks(true, std::slice::from_ref(hunk), &snapshot, true, cx);
            });
            hunk.secondary_status = SecondaryHunkRemovalPending;
        } else {
            log::info!("unstaging hunk at {row}");
            uncommitted_diff.update(cx, |diff, cx| {
                diff.stage_or_unstage_hunks(false, std::slice::from_ref(hunk), &snapshot, true, cx);
            });
            hunk.secondary_status = SecondaryHunkAdditionPending;
        }

        for _ in 0..rng.random_range(0..10) {
            log::info!("yielding");
            cx.executor().simulate_random_delay().await;
        }
    }

    cx.executor().run_until_parked();

    for hunk in &mut hunks {
        if hunk.secondary_status == SecondaryHunkRemovalPending {
            hunk.secondary_status = NoSecondaryHunk;
        } else if hunk.secondary_status == SecondaryHunkAdditionPending {
            hunk.secondary_status = HasSecondaryHunk;
        }
    }

    log::info!(
        "index text:\n{}",
        repo.load_index_text(RepoPath::from_rel_path(rel_path("file.txt")))
            .await
            .unwrap()
    );

    uncommitted_diff.update(cx, |diff, cx| {
        let expected_hunks = hunks
            .iter()
            .map(|hunk| (hunk.range.start.row, hunk.secondary_status))
            .collect::<Vec<_>>();
        let actual_hunks = diff
            .snapshot(cx)
            .hunks(&snapshot)
            .map(|hunk| (hunk.range.start.row, hunk.secondary_status))
            .collect::<Vec<_>>();
        assert_eq!(actual_hunks, expected_hunks);
    });
}
