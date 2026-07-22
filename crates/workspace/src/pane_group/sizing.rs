use super::*;

pub(super) fn normalize_flexes(member_count: usize, flexes: Vec<f32>) -> Vec<f32> {
    if member_count == 0 {
        return Vec::new();
    }

    let total_flex = flexes.iter().copied().sum::<f32>();
    if flexes.len() != member_count
        || !total_flex.is_finite()
        || total_flex <= f32::EPSILON
        || flexes.iter().any(|flex| !flex.is_finite() || *flex <= 0.)
    {
        return vec![1.; member_count];
    }

    let scale = member_count as f32 / total_flex;
    flexes.into_iter().map(|flex| flex * scale).collect()
}

pub(super) fn split_flexes_for_inserted_size(
    available_size: Pixels,
    inserted_size: Pixels,
    insert_after: bool,
) -> Option<Vec<f32>> {
    let (old_ratio, inserted_ratio) =
        split_ratios_for_inserted_size(available_size, inserted_size)?;
    let old_flex = old_ratio * 2.;
    let inserted_flex = inserted_ratio * 2.;
    Some(if insert_after {
        vec![old_flex, inserted_flex]
    } else {
        vec![inserted_flex, old_flex]
    })
}

pub(super) fn split_ratios_for_inserted_size(
    available_size: Pixels,
    inserted_size: Pixels,
) -> Option<(f32, f32)> {
    let available_size = available_size.as_f32();
    if !available_size.is_finite() || available_size <= 0. {
        return None;
    }

    let min_size = HORIZONTAL_MIN_SIZE;
    let max_inserted_size = available_size - min_size;
    if max_inserted_size < min_size {
        return None;
    }

    let inserted_size = inserted_size.as_f32().clamp(min_size, max_inserted_size);
    let old_size = available_size - inserted_size;

    Some((old_size / available_size, inserted_size / available_size))
}

pub(super) fn resize_adjacent_visible_pair(
    flexes: &mut [f32],
    visible_indices: &[usize],
    current_ix: usize,
    next_ix: usize,
    pixel_delta: Pixels,
    available_size: Pixels,
    min_size: Pixels,
) -> bool {
    let requested_delta = pixel_delta.as_f32();
    if requested_delta.abs() <= f32::EPSILON || available_size <= px(0.) {
        return false;
    }

    let Some(current_visible_ix) = visible_indices
        .iter()
        .position(|visible_ix| *visible_ix == current_ix)
    else {
        return false;
    };
    let Some(next_visible_ix) = visible_indices
        .iter()
        .position(|visible_ix| *visible_ix == next_ix)
    else {
        return false;
    };
    if next_visible_ix != current_visible_ix + 1 {
        return false;
    }

    let visible_total_flex = visible_indices.iter().map(|ix| flexes[*ix]).sum::<f32>();
    if !visible_total_flex.is_finite() || visible_total_flex <= f32::EPSILON {
        return false;
    }

    let current_size = available_size.as_f32() * flexes[current_ix] / visible_total_flex;
    let next_size = available_size.as_f32() * flexes[next_ix] / visible_total_flex;
    let min_size = min_size.as_f32();
    let min_delta = min_size - current_size;
    let max_delta = next_size - min_size;

    if min_delta > max_delta {
        return false;
    }

    let actual_delta = requested_delta.clamp(min_delta, max_delta);
    if actual_delta.abs() <= f32::EPSILON {
        return false;
    }

    let flex_delta = actual_delta * visible_total_flex / available_size.as_f32();
    flexes[current_ix] += flex_delta;
    flexes[next_ix] -= flex_delta;
    true
}
