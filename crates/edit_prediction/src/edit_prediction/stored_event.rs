use super::*;

impl StoredEvent {
    fn can_merge(
        &self,
        next_old_event: &StoredEvent,
        latest_snapshot: &TextBufferSnapshot,
        latest_edit_range: &Range<Anchor>,
    ) -> bool {
        // Events must be for the same buffer and be contiguous across included snapshots to be mergeable.
        if self.old_snapshot.remote_id() != next_old_event.old_snapshot.remote_id() {
            return false;
        }
        if self.old_snapshot.remote_id() != latest_snapshot.remote_id() {
            return false;
        }
        if self.new_snapshot_version != next_old_event.old_snapshot.version {
            return false;
        }
        if !latest_snapshot
            .version
            .observed_all(&next_old_event.new_snapshot_version)
        {
            return false;
        }

        let a_is_predicted = matches!(
            self.event.as_ref(),
            zeta_prompt::Event::BufferChange {
                predicted: true,
                ..
            }
        );
        let b_is_predicted = matches!(
            next_old_event.event.as_ref(),
            zeta_prompt::Event::BufferChange {
                predicted: true,
                ..
            }
        );

        // If events come from the same source (both predicted or both manual) then
        // we would have coalesced them already.
        if a_is_predicted == b_is_predicted {
            return false;
        }

        let left_range = self.total_edit_range.to_point(latest_snapshot);
        let right_range = next_old_event.total_edit_range.to_point(latest_snapshot);
        let latest_range = latest_edit_range.to_point(latest_snapshot);

        // Events near to the latest edit are not merged if their sources differ.
        if lines_between_ranges(&left_range, &latest_range)
            .min(lines_between_ranges(&right_range, &latest_range))
            <= CHANGE_GROUPING_LINE_SPAN
        {
            return false;
        }

        // Events that are distant from each other are not merged.
        if lines_between_ranges(&left_range, &right_range) > CHANGE_GROUPING_LINE_SPAN {
            return false;
        }

        true
    }
}
