use super::*;

#[derive(Debug, Default, PartialEq, Eq, Clone, Hash)]
pub(super) struct ActionMapping {
    pub(super) keystrokes: Rc<[KeybindingKeystroke]>,
    pub(super) context: Option<SharedString>,
}

#[derive(Debug)]
pub(super) struct KeybindConflict {
    pub(super) first_conflict_index: usize,
    pub(super) remaining_conflict_amount: usize,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct ConflictOrigin {
    pub(super) override_source: KeybindSource,
    pub(super) overridden_source: Option<KeybindSource>,
    pub(super) index: usize,
}

impl ConflictOrigin {
    pub(super) fn new(source: KeybindSource, index: usize) -> Self {
        Self {
            override_source: source,
            index,
            overridden_source: None,
        }
    }

    fn with_overridden_source(self, source: KeybindSource) -> Self {
        Self {
            overridden_source: Some(source),
            ..self
        }
    }

    fn get_conflict_with(&self, other: &Self) -> Option<Self> {
        if self.override_source == KeybindSource::User
            && other.override_source == KeybindSource::User
        {
            Some(
                Self::new(KeybindSource::User, other.index)
                    .with_overridden_source(self.override_source),
            )
        } else if self.override_source > other.override_source {
            Some(other.with_overridden_source(self.override_source))
        } else {
            None
        }
    }

    fn is_user_keybind_conflict(&self) -> bool {
        self.override_source == KeybindSource::User
            && self.overridden_source == Some(KeybindSource::User)
    }
}

#[derive(Default)]
pub(super) struct ConflictState {
    pub(super) conflicts: Vec<Option<ConflictOrigin>>,
    pub(super) keybind_mapping: ConflictKeybindMapping,
    pub(super) has_user_conflicts: bool,
}

type ConflictKeybindMapping = HashMap<
    Rc<[KeybindingKeystroke]>,
    Vec<(
        Option<gpui::KeyBindingContextPredicate>,
        Vec<ConflictOrigin>,
    )>,
>;

impl ConflictState {
    pub(super) fn new(key_bindings: &[ProcessedBinding]) -> Self {
        let mut action_keybind_mapping = ConflictKeybindMapping::default();

        let mut largest_index = 0;
        for (index, binding) in key_bindings
            .iter()
            .enumerate()
            .flat_map(|(index, binding)| Some(index).zip(binding.keybind_information()))
        {
            let mapping = binding.get_action_mapping();
            let predicate = mapping
                .context
                .and_then(|ctx| gpui::KeyBindingContextPredicate::parse(&ctx).ok());
            let entry = action_keybind_mapping
                .entry(mapping.keystrokes.clone())
                .or_default();
            let origin = ConflictOrigin::new(binding.source, index);
            if let Some((_, origins)) =
                entry
                    .iter_mut()
                    .find(|(other_predicate, _)| match (&predicate, other_predicate) {
                        (None, None) => true,
                        (Some(a), Some(b)) => normalized_ctx_eq(a, b),
                        _ => false,
                    })
            {
                origins.push(origin);
            } else {
                entry.push((predicate, vec![origin]));
            }
            largest_index = index;
        }

        let mut conflicts = vec![None; largest_index + 1];
        let mut has_user_conflicts = false;

        for entries in action_keybind_mapping.values_mut() {
            for (_, indices) in entries.iter_mut() {
                indices.sort_unstable_by_key(|origin| origin.override_source);
                let Some((fst, snd)) = indices.get(0).zip(indices.get(1)) else {
                    continue;
                };

                for origin in indices.iter() {
                    conflicts[origin.index] =
                        origin.get_conflict_with(if origin == fst { snd } else { fst })
                }

                has_user_conflicts |= fst.override_source == KeybindSource::User
                    && snd.override_source == KeybindSource::User;
            }
        }

        Self {
            conflicts,
            keybind_mapping: action_keybind_mapping,
            has_user_conflicts,
        }
    }

    pub(super) fn conflicting_indices_for_mapping(
        &self,
        action_mapping: &ActionMapping,
        keybind_idx: Option<usize>,
    ) -> Option<KeybindConflict> {
        let ActionMapping {
            keystrokes,
            context,
        } = action_mapping;
        let predicate = context
            .as_deref()
            .and_then(|ctx| gpui::KeyBindingContextPredicate::parse(&ctx).ok());
        self.keybind_mapping.get(keystrokes).and_then(|entries| {
            entries
                .iter()
                .find_map(|(other_predicate, indices)| {
                    match (&predicate, other_predicate) {
                        (None, None) => true,
                        (Some(pred), Some(other)) => normalized_ctx_eq(pred, other),
                        _ => false,
                    }
                    .then_some(indices)
                })
                .and_then(|indices| {
                    let mut indices = indices
                        .iter()
                        .filter(|&conflict| Some(conflict.index) != keybind_idx);
                    indices.next().map(|origin| KeybindConflict {
                        first_conflict_index: origin.index,
                        remaining_conflict_amount: indices.count(),
                    })
                })
        })
    }

    pub(super) fn conflict_for_idx(&self, idx: usize) -> Option<ConflictOrigin> {
        self.conflicts.get(idx).copied().flatten()
    }

    pub(super) fn has_user_conflict(&self, candidate_idx: usize) -> bool {
        self.conflict_for_idx(candidate_idx)
            .is_some_and(|conflict| conflict.is_user_keybind_conflict())
    }

    pub(super) fn any_user_binding_conflicts(&self) -> bool {
        self.has_user_conflicts
    }
}
