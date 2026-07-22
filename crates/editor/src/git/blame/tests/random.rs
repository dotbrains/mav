use super::*;

#[gpui::test(iterations = 100)]
async fn test_blame_random(mut rng: StdRng, cx: &mut gpui::TestAppContext) {
    let operations = env::var("OPERATIONS")
        .map(|i| i.parse().expect("invalid `OPERATIONS` variable"))
        .unwrap_or(10);
    let max_edits_per_operation = env::var("MAX_EDITS_PER_OPERATION")
        .map(|i| {
            i.parse()
                .expect("invalid `MAX_EDITS_PER_OPERATION` variable")
        })
        .unwrap_or(5);
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let buffer_initial_text_len = rng.random_range(5..15);
    let mut buffer_initial_text = Rope::from(
        RandomCharIter::new(&mut rng)
            .take(buffer_initial_text_len)
            .collect::<String>()
            .as_str(),
    );

    let mut newline_ixs = (0..buffer_initial_text_len).choose_multiple(&mut rng, 5);
    newline_ixs.sort_unstable();
    for newline_ix in newline_ixs.into_iter().rev() {
        let newline_ix = buffer_initial_text.clip_offset(newline_ix, Bias::Right);
        buffer_initial_text.replace(newline_ix..newline_ix, "\n");
    }
    log::info!("initial buffer text: {:?}", buffer_initial_text);

    fs.insert_tree(
        path!("/my-repo"),
        json!({
            ".git": {},
            "file.txt": buffer_initial_text.to_string()
        }),
    )
    .await;

    let blame_entries = gen_blame_entries(buffer_initial_text.max_point().row, &mut rng);
    log::info!("initial blame entries: {:?}", blame_entries);
    fs.set_blame_for_repo(
        Path::new(path!("/my-repo/.git")),
        vec![(
            repo_path("file.txt"),
            Blame {
                entries: blame_entries,
                ..Default::default()
            },
        )],
    );

    let project = Project::test(fs.clone(), [path!("/my-repo").as_ref()], cx).await;
    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/my-repo/file.txt"), cx)
        })
        .await
        .unwrap();
    let mbuffer = cx.new(|cx| MultiBuffer::singleton(buffer.clone(), cx));

    let git_blame = cx.new(|cx| GitBlame::new(mbuffer.clone(), project, false, true, cx));
    cx.executor().run_until_parked();
    git_blame.update(cx, |blame, cx| blame.check_invariants(cx));

    for _ in 0..operations {
        match rng.random_range(0..100) {
            0..=19 => {
                log::info!("quiescing");
                cx.executor().run_until_parked();
            }
            20..=69 => {
                log::info!("editing buffer");
                buffer.update(cx, |buffer, cx| {
                    buffer.randomly_edit(&mut rng, max_edits_per_operation, cx);
                    log::info!("buffer text: {:?}", buffer.text());
                });

                let blame_entries = gen_blame_entries(
                    buffer.read_with(cx, |buffer, _| buffer.max_point().row),
                    &mut rng,
                );
                log::info!("regenerating blame entries: {:?}", blame_entries);

                fs.set_blame_for_repo(
                    Path::new(path!("/my-repo/.git")),
                    vec![(
                        repo_path("file.txt"),
                        Blame {
                            entries: blame_entries,
                            ..Default::default()
                        },
                    )],
                );
            }
            _ => {
                git_blame.update(cx, |blame, cx| blame.check_invariants(cx));
            }
        }
    }

    git_blame.update(cx, |blame, cx| blame.check_invariants(cx));
}
