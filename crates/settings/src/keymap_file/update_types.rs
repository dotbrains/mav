use super::*;

pub enum KeybindUpdateOperation<'a> {
    Replace {
        /// Describes the keybind to create
        source: KeybindUpdateTarget<'a>,
        /// Describes the keybind to remove
        target: KeybindUpdateTarget<'a>,
        target_keybind_source: KeybindSource,
    },
    Add {
        source: KeybindUpdateTarget<'a>,
        from: Option<KeybindUpdateTarget<'a>>,
    },
    Remove {
        target: KeybindUpdateTarget<'a>,
        target_keybind_source: KeybindSource,
    },
}

impl KeybindUpdateOperation<'_> {
    pub fn generate_telemetry(
        &self,
    ) -> (
        // The keybind that is created
        String,
        // The keybinding that was removed
        String,
        // The source of the keybinding
        String,
    ) {
        let (new_binding, removed_binding, source) = match &self {
            KeybindUpdateOperation::Replace {
                source,
                target,
                target_keybind_source,
            } => (Some(source), Some(target), Some(*target_keybind_source)),
            KeybindUpdateOperation::Add { source, .. } => (Some(source), None, None),
            KeybindUpdateOperation::Remove {
                target,
                target_keybind_source,
            } => (None, Some(target), Some(*target_keybind_source)),
        };

        let new_binding = new_binding
            .map(KeybindUpdateTarget::telemetry_string)
            .unwrap_or("null".to_owned());
        let removed_binding = removed_binding
            .map(KeybindUpdateTarget::telemetry_string)
            .unwrap_or("null".to_owned());

        let source = source
            .as_ref()
            .map(KeybindSource::name)
            .map(ToOwned::to_owned)
            .unwrap_or("null".to_owned());

        (new_binding, removed_binding, source)
    }
}

impl<'a> KeybindUpdateOperation<'a> {
    pub fn add(source: KeybindUpdateTarget<'a>) -> Self {
        Self::Add { source, from: None }
    }
}

#[derive(Debug, Clone)]
pub struct KeybindUpdateTarget<'a> {
    pub context: Option<&'a str>,
    pub keystrokes: &'a [KeybindingKeystroke],
    pub action_name: &'a str,
    pub action_arguments: Option<&'a str>,
}

impl<'a> KeybindUpdateTarget<'a> {
    pub(super) fn action_value(&self) -> Result<Value> {
        if self.action_name == gpui::NoAction.name() {
            return Ok(Value::Null);
        }
        let action_name: Value = self.action_name.into();
        let value = match self.action_arguments {
            Some(args) if !args.is_empty() => {
                let args = serde_json::from_str::<Value>(args)
                    .context("Failed to parse action arguments as JSON")?;
                serde_json::json!([action_name, args])
            }
            _ => action_name,
        };
        Ok(value)
    }

    pub(super) fn keystrokes_unparsed(&self) -> String {
        let mut keystrokes = String::with_capacity(self.keystrokes.len() * 8);
        for keystroke in self.keystrokes {
            // The reason use `keystroke.unparse()` instead of `keystroke.inner.unparse()`
            // here is that, we want the user to use `ctrl-shift-4` instead of `ctrl-$`
            // by default on Windows.
            keystrokes.push_str(&keystroke.unparse());
            keystrokes.push(' ');
        }
        keystrokes.pop();
        keystrokes
    }

    fn telemetry_string(&self) -> String {
        format!(
            "action_name: {}, context: {}, action_arguments: {}, keystrokes: {}",
            self.action_name,
            self.context.unwrap_or("global"),
            self.action_arguments.unwrap_or("none"),
            self.keystrokes_unparsed()
        )
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum KeybindSource {
    User,
    Vim,
    Base,
    #[default]
    Default,
    Unknown,
}

impl KeybindSource {
    const BASE: KeyBindingMetaIndex = KeyBindingMetaIndex(KeybindSource::Base as u32);
    const DEFAULT: KeyBindingMetaIndex = KeyBindingMetaIndex(KeybindSource::Default as u32);
    const VIM: KeyBindingMetaIndex = KeyBindingMetaIndex(KeybindSource::Vim as u32);
    const USER: KeyBindingMetaIndex = KeyBindingMetaIndex(KeybindSource::User as u32);

    pub fn name(&self) -> &'static str {
        match self {
            KeybindSource::User => "User",
            KeybindSource::Default => "Default",
            KeybindSource::Base => "Base",
            KeybindSource::Vim => "Vim",
            KeybindSource::Unknown => "Unknown",
        }
    }

    pub fn meta(&self) -> KeyBindingMetaIndex {
        match self {
            KeybindSource::User => Self::USER,
            KeybindSource::Default => Self::DEFAULT,
            KeybindSource::Base => Self::BASE,
            KeybindSource::Vim => Self::VIM,
            KeybindSource::Unknown => KeyBindingMetaIndex(*self as u32),
        }
    }

    pub fn from_meta(index: KeyBindingMetaIndex) -> Self {
        match index {
            Self::USER => KeybindSource::User,
            Self::BASE => KeybindSource::Base,
            Self::DEFAULT => KeybindSource::Default,
            Self::VIM => KeybindSource::Vim,
            _ => KeybindSource::Unknown,
        }
    }
}

impl From<KeyBindingMetaIndex> for KeybindSource {
    fn from(index: KeyBindingMetaIndex) -> Self {
        Self::from_meta(index)
    }
}

impl From<KeybindSource> for KeyBindingMetaIndex {
    fn from(source: KeybindSource) -> Self {
        source.meta()
    }
}
