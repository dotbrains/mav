use super::*;

pub struct ActionSequence(pub Vec<Box<dyn Action>>);

register_action!(ActionSequence);

impl ActionSequence {
    pub(super) fn build_sequence(
        value: Value,
        cx: &App,
    ) -> std::result::Result<Box<dyn Action>, ActionBuildError> {
        match value {
            Value::Array(values) => {
                let actions = values
                    .into_iter()
                    .enumerate()
                    .map(|(index, action)| {
                        match KeymapFile::build_keymap_action(&KeymapAction(action), cx) {
                            Ok((action, _)) => Ok(action),
                            Err(err) => {
                                return Err(ActionBuildError::BuildError {
                                    name: Self::name_for_type().to_string(),
                                    error: anyhow::anyhow!(
                                        "error at sequence index {index}: {err}"
                                    ),
                                });
                            }
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Box::new(Self(actions)))
            }
            _ => Err(Self::expected_array_error()),
        }
    }

    pub(super) fn expected_array_error() -> ActionBuildError {
        ActionBuildError::BuildError {
            name: Self::name_for_type().to_string(),
            error: anyhow::anyhow!("expected array of actions"),
        }
    }
}

impl Action for ActionSequence {
    fn name(&self) -> &'static str {
        Self::name_for_type()
    }

    fn name_for_type() -> &'static str
    where
        Self: Sized,
    {
        "action::Sequence"
    }

    fn partial_eq(&self, action: &dyn Action) -> bool {
        action
            .as_any()
            .downcast_ref::<Self>()
            .map_or(false, |other| {
                self.0.len() == other.0.len()
                    && self
                        .0
                        .iter()
                        .zip(other.0.iter())
                        .all(|(a, b)| a.partial_eq(b.as_ref()))
            })
    }

    fn boxed_clone(&self) -> Box<dyn Action> {
        Box::new(ActionSequence(
            self.0
                .iter()
                .map(|action| action.boxed_clone())
                .collect::<Vec<_>>(),
        ))
    }

    fn build(_value: Value) -> Result<Box<dyn Action>> {
        Err(anyhow::anyhow!(
            "{} cannot be built directly",
            Self::name_for_type()
        ))
    }

    fn action_json_schema(generator: &mut schemars::SchemaGenerator) -> Option<schemars::Schema> {
        let keymap_action_schema = generator.subschema_for::<KeymapAction>();
        Some(json_schema!({
            "type": "array",
            "items": keymap_action_schema
        }))
    }

    fn deprecated_aliases() -> &'static [&'static str] {
        &[]
    }

    fn deprecation_message() -> Option<&'static str> {
        None
    }

    fn documentation() -> Option<&'static str> {
        Some(
            "Runs a sequence of actions.\n\n\
            NOTE: This does **not** wait for asynchronous actions to complete before running the next action.",
        )
    }
}
