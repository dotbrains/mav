use super::*;

pub(super) type PersistedFold = (usize, usize, Option<String>, Option<String>);
pub(super) type MigratedFold = (usize, usize, String, String);

pub(super) fn resolve_persisted_folds(
    folds: Vec<PersistedFold>,
    snapshot: &MultiBufferSnapshot,
    collect_migration_data: bool,
) -> (Vec<Range<MultiBufferOffset>>, Vec<MigratedFold>) {
    let snapshot_len = snapshot.len().0;
    let mut search_start = 0usize;
    let mut migrated_folds = Vec::new();

    let valid_folds = folds
        .into_iter()
        .filter_map(|(stored_start, stored_end, start_fp, end_fp)| {
            let sfp = start_fp?;
            let efp = end_fp?;
            let efp_len = efp.len();

            let start_matches = stored_start < snapshot_len
                && snapshot.contains_str_at(MultiBufferOffset(stored_start), &sfp);
            let efp_check_pos = stored_end.saturating_sub(efp_len);
            let end_matches = efp_check_pos >= stored_start
                && stored_end <= snapshot_len
                && snapshot.contains_str_at(MultiBufferOffset(efp_check_pos), &efp);

            let (new_start, new_end) = if start_matches && end_matches {
                (stored_start, stored_end)
            } else if sfp == efp {
                let new_start = find_fingerprint(snapshot, &sfp, search_start)?;
                let fold_len = stored_end - stored_start;
                let new_end = new_start + fold_len;
                (new_start, new_end)
            } else {
                let new_start = find_fingerprint(snapshot, &sfp, search_start)?;
                let efp_pos = find_fingerprint(snapshot, &efp, new_start + sfp.len())?;
                let new_end = efp_pos + efp_len;
                (new_start, new_end)
            };

            search_start = new_end;

            if new_end <= new_start {
                return None;
            }

            if collect_migration_data {
                migrated_folds.push((new_start, new_end, sfp, efp));
            }

            Some(
                snapshot.clip_offset(MultiBufferOffset(new_start), Bias::Left)
                    ..snapshot.clip_offset(MultiBufferOffset(new_end), Bias::Right),
            )
        })
        .collect();

    (valid_folds, migrated_folds)
}

fn find_fingerprint(
    snapshot: &MultiBufferSnapshot,
    fingerprint: &str,
    search_start: usize,
) -> Option<usize> {
    let search_start = snapshot
        .clip_offset(MultiBufferOffset(search_start), Bias::Left)
        .0;
    let search_end = snapshot.len().0.saturating_sub(fingerprint.len());

    let mut byte_offset = search_start;
    for ch in snapshot.chars_at(MultiBufferOffset(search_start)) {
        if byte_offset > search_end {
            break;
        }
        if snapshot.contains_str_at(MultiBufferOffset(byte_offset), fingerprint) {
            return Some(byte_offset);
        }
        byte_offset += ch.len_utf8();
    }
    None
}
