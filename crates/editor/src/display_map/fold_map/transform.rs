use super::*;

#[derive(Clone, Debug, Default)]
pub(crate) struct Transform {
    pub(crate) summary: TransformSummary,
    pub(crate) placeholder: Option<TransformPlaceholder>,
}

#[derive(Clone, Debug)]
pub(crate) struct TransformPlaceholder {
    pub(crate) text: SharedString,
    pub(crate) chars: u128,
    pub(crate) renderer: ChunkRenderer,
}

impl Transform {
    pub(crate) fn is_fold(&self) -> bool {
        self.placeholder.is_some()
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct TransformSummary {
    pub(crate) output: MBTextSummary,
    pub(crate) input: MBTextSummary,
}

impl sum_tree::Item for Transform {
    type Summary = TransformSummary;

    fn summary(&self, _cx: ()) -> Self::Summary {
        self.summary.clone()
    }
}

impl sum_tree::ContextLessSummary for TransformSummary {
    fn zero() -> Self {
        Default::default()
    }

    fn add_summary(&mut self, other: &Self) {
        self.input += other.input;
        self.output += other.output;
    }
}
