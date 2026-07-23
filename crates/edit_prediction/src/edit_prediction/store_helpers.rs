use super::*;

pub(crate) fn collaborator_edit_overlaps_locality_region(
    project_state: &ProjectState,
    project: &Entity<Project>,
    buffer: &Entity<Buffer>,
    snapshot: &BufferSnapshot,
    edit_range: &Range<Anchor>,
    cx: &App,
) -> bool {
    let Some((active_buffer, Some(position))) = project_state.active_buffer(project, cx) else {
        return false;
    };

    if active_buffer.entity_id() != buffer.entity_id() {
        return false;
    }

    let locality_point_range = expand_context_syntactically_then_linewise(
        snapshot,
        (position..position).to_point(snapshot),
        COLLABORATOR_EDIT_LOCALITY_CONTEXT_TOKENS,
    );
    let locality_anchor_range = snapshot.anchor_range_inside(locality_point_range);

    edit_range.overlaps(&locality_anchor_range, snapshot)
}

pub(crate) fn merge_trailing_events_if_needed(
    events: &mut VecDeque<StoredEvent>,
    end_snapshot: &TextBufferSnapshot,
    latest_snapshot: &TextBufferSnapshot,
    latest_edit_range: &Range<Anchor>,
) {
    if let Some(last_event) = events.back() {
        if last_event.old_snapshot.remote_id() != latest_snapshot.remote_id() {
            return;
        }
        if !latest_snapshot
            .version
            .observed_all(&last_event.new_snapshot_version)
        {
            return;
        }
    }

    let mut next_old_event = None;
    let mut mergeable_count = 0;
    for old_event in events.iter().rev() {
        if let Some(next_old_event) = next_old_event
            && !old_event.can_merge(next_old_event, latest_snapshot, latest_edit_range)
        {
            break;
        }
        mergeable_count += 1;
        next_old_event = Some(old_event);
    }

    if mergeable_count <= 1 {
        return;
    }

    let merge_start = events.len() - mergeable_count;
    let oldest_event = &events[merge_start];
    let oldest_snapshot = oldest_event.old_snapshot.clone();
    let newest_snapshot = end_snapshot;
    let mut merged_edit_range = oldest_event.total_edit_range.clone();

    for event in events.range(events.len() - mergeable_count + 1..) {
        merged_edit_range =
            merge_anchor_ranges(&merged_edit_range, &event.total_edit_range, latest_snapshot);
    }

    if let Some((diff, old_range, new_range)) = compute_diff_between_snapshots_in_range(
        &oldest_snapshot,
        newest_snapshot,
        &merged_edit_range,
    ) {
        let merged_event = match oldest_event.event.as_ref() {
            zeta_prompt::Event::BufferChange {
                old_path,
                path,
                in_open_source_repo,
                ..
            } => StoredEvent {
                event: Arc::new(zeta_prompt::Event::BufferChange {
                    old_path: old_path.clone(),
                    path: path.clone(),
                    diff,
                    old_range,
                    new_range: new_range.clone(),
                    in_open_source_repo: *in_open_source_repo,
                    predicted: events.range(merge_start..).all(|event| {
                        matches!(
                            event.event.as_ref(),
                            zeta_prompt::Event::BufferChange {
                                predicted: true,
                                ..
                            }
                        )
                    }),
                }),
                old_snapshot: oldest_snapshot.clone(),
                new_snapshot_version: newest_snapshot.version.clone(),
                total_edit_range: newest_snapshot.anchor_before(new_range.start)
                    ..newest_snapshot.anchor_before(new_range.end),
                file_context: oldest_event.file_context.clone(),
            },
        };
        events.truncate(events.len() - mergeable_count);
        events.push_back(merged_event);
    }
}

#[derive(Error, Debug)]
#[error(
    "You must update to Mav version {minimum_version} or higher to continue using edit predictions."
)]
pub struct MavUpdateRequiredError {
    pub(crate) minimum_version: Version,
}

#[derive(Error, Debug)]
#[error("Cloud request timed out")]
pub(crate) struct CloudRequestTimeoutError;

pub(crate) struct MavPredictUpsell;

pub(crate) fn is_upsell_dismissed(cx: &App) -> bool {
    // To make this backwards compatible with older versions of Mav, we
    // check if the user has seen the previous Edit Prediction Onboarding
    // before, by checking the data collection choice which was written to
    // the database once the user clicked on "Accept and Enable"
    let kvp = KeyValueStore::global(cx);
    if kvp
        .read_kvp(MAV_PREDICT_DATA_COLLECTION_CHOICE)
        .log_err()
        .is_some_and(|s| s.is_some())
    {
        return true;
    }

    kvp.read_kvp(MavPredictUpsell::KEY)
        .log_err()
        .is_some_and(|s| s.is_some())
}

impl Dismissable for MavPredictUpsell {
    const KEY: &'static str = "dismissed-edit-predict-upsell";

    fn dismissed(cx: &App) -> bool {
        is_upsell_dismissed(cx)
    }
}

pub fn should_show_upsell_modal(cx: &App) -> bool {
    !is_upsell_dismissed(cx)
}

pub fn init(cx: &mut App) {
    cx.observe_new(move |workspace: &mut Workspace, _, _cx| {
        workspace.register_action(
            move |workspace, _: &mav_actions::OpenMavPredictOnboarding, window, cx| {
                MavPredictModal::toggle(
                    workspace,
                    workspace.user_store().clone(),
                    workspace.client().clone(),
                    window,
                    cx,
                )
            },
        );

        workspace.register_action(|workspace, _: &ResetOnboarding, _window, cx| {
            update_settings_file(workspace.app_state().fs.clone(), cx, move |settings, _| {
                settings
                    .project
                    .all_languages
                    .edit_predictions
                    .get_or_insert_default()
                    .provider = Some(EditPredictionProvider::None)
            });
        });
        pub(crate) fn copilot_for_project(
            project: &Entity<Project>,
            cx: &mut App,
        ) -> Option<Entity<Copilot>> {
            EditPredictionStore::try_global(cx).and_then(|store| {
                store.update(cx, |this, cx| this.start_copilot_for_project(project, cx))
            })
        }

        workspace.register_action(|workspace, _: &SignIn, window, cx| {
            if let Some(copilot) = copilot_for_project(workspace.project(), cx) {
                copilot_ui::initiate_sign_in(copilot, window, cx);
            }
        });
        workspace.register_action(|workspace, _: &Reinstall, window, cx| {
            if let Some(copilot) = copilot_for_project(workspace.project(), cx) {
                copilot_ui::reinstall_and_sign_in(copilot, window, cx);
            }
        });
        workspace.register_action(|workspace, _: &SignOut, window, cx| {
            if let Some(copilot) = copilot_for_project(workspace.project(), cx) {
                copilot_ui::initiate_sign_out(copilot, window, cx);
            }
        });
    })
    .detach();
}

pub(crate) fn is_mav_industries_repo(url: &str) -> bool {
    url.strip_prefix("https://github.com/mav-industries/")
        .or_else(|| url.strip_prefix("http://github.com/mav-industries/"))
        .or_else(|| url.strip_prefix("git@github.com:mav-industries/"))
        .or_else(|| url.strip_prefix("ssh://git@github.com/mav-industries/"))
        .is_some_and(|repo| !repo.is_empty())
}
