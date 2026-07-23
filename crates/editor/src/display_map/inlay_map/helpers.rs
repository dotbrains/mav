use super::*;

pub(crate) fn push_isomorphic(sum_tree: &mut SumTree<Transform>, summary: MBTextSummary) {
    if summary.len == MultiBufferOffset(0) {
        return;
    }

    let mut summary = Some(summary);
    sum_tree.update_last(
        |transform| {
            if let Transform::Isomorphic(transform) = transform {
                *transform += summary.take().unwrap();
            }
        },
        (),
    );

    if let Some(summary) = summary {
        sum_tree.push(Transform::Isomorphic(summary), ());
    }
}
