use super::*;

impl KeymapFile {
    pub(super) fn build_keymap_action(
        action: &KeymapAction,
        cx: &App,
    ) -> std::result::Result<(Box<dyn Action>, Option<String>), String> {
        Self::build_keymap_action_value(&action.0, cx)
    }

    pub(super) fn build_keymap_action_value(
        action: &Value,
        cx: &App,
    ) -> std::result::Result<(Box<dyn Action>, Option<String>), String> {
        let (build_result, action_input_string) = match Self::parse_action_value(action)? {
            Some((name, action_input)) if name.as_str() == ActionSequence::name_for_type() => {
                match action_input {
                    Some(action_input) => (
                        ActionSequence::build_sequence(action_input.clone(), cx),
                        None,
                    ),
                    None => (Err(ActionSequence::expected_array_error()), None),
                }
            }
            Some((name, Some(action_input))) => {
                let action_input_string = action_input.to_string();
                (
                    cx.build_action(name, Some(action_input.clone())),
                    Some(action_input_string),
                )
            }
            Some((name, None)) => (cx.build_action(name, None), None),
            None => (Ok(NoAction.boxed_clone()), None),
        };

        let action = match build_result {
            Ok(action) => action,
            Err(ActionBuildError::NotFound { name }) => {
                return Err(format!(
                    "didn't find an action named {}.",
                    MarkdownInlineCode(&format!("\"{}\"", &name))
                ));
            }
            Err(ActionBuildError::BuildError { name, error }) => match action_input_string {
                Some(action_input_string) => {
                    return Err(format!(
                        "can't build {} action from input value {}: {}",
                        MarkdownInlineCode(&format!("\"{}\"", &name)),
                        MarkdownInlineCode(&action_input_string),
                        MarkdownEscaped(&error.to_string())
                    ));
                }
                None => {
                    return Err(format!(
                        "can't build {} action - it requires input data via [name, input]: {}",
                        MarkdownInlineCode(&format!("\"{}\"", &name)),
                        MarkdownEscaped(&error.to_string())
                    ));
                }
            },
        };

        Ok((action, action_input_string))
    }
}
