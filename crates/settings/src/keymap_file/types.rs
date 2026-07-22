use super::*;

pub trait KeyBindingValidator: Send + Sync {
    fn action_type_id(&self) -> TypeId;
    fn validate(&self, binding: &KeyBinding) -> Result<(), MarkdownString>;
}

pub struct KeyBindingValidatorRegistration(pub fn() -> Box<dyn KeyBindingValidator>);

inventory::collect!(KeyBindingValidatorRegistration);

pub(crate) static KEY_BINDING_VALIDATORS: LazyLock<BTreeMap<TypeId, Box<dyn KeyBindingValidator>>> =
    LazyLock::new(|| {
        let mut validators = BTreeMap::new();
        for validator_registration in inventory::iter::<KeyBindingValidatorRegistration> {
            let validator = validator_registration.0();
            validators.insert(validator.action_type_id(), validator);
        }
        validators
    });

// Note that the doc comments on these are shown by json-language-server when editing the keymap, so
// they should be considered user-facing documentation. Documentation is not handled well with
// schemars-0.8 - when there are newlines, it is rendered as plaintext (see
// https://github.com/GREsau/schemars/issues/38#issuecomment-2282883519). So for now these docs
// avoid newlines.
//
// TODO: Update to schemars-1.0 once it's released, and add more docs as newlines would be
// supported. Tracking issue is https://github.com/GREsau/schemars/issues/112.

/// Keymap configuration consisting of sections. Each section may have a context predicate which
/// determines whether its bindings are used.
#[derive(Debug, Deserialize, Default, Clone, JsonSchema)]
#[serde(transparent)]
pub struct KeymapFile(pub(super) Vec<KeymapSection>);

/// Keymap section which binds keystrokes to actions.
#[derive(Debug, Deserialize, Default, Clone, JsonSchema)]
pub struct KeymapSection {
    /// Determines when these bindings are active. When just a name is provided, like `Editor` or
    /// `Workspace`, the bindings will be active in that context. Boolean expressions like `X && Y`,
    /// `X || Y`, `!X` are also supported. Some more complex logic including checking OS and the
    /// current file extension are also supported - see [the
    /// documentation](https://mav.dev/docs/key-bindings#contexts) for more details.
    #[serde(default)]
    pub context: String,
    /// This option enables specifying keys based on their position on a QWERTY keyboard, by using
    /// position-equivalent mappings for some non-QWERTY keyboards. This is currently only supported
    /// on macOS. See the documentation for more details.
    #[serde(default)]
    pub(super) use_key_equivalents: bool,
    /// This keymap section's unbindings, as a JSON object mapping keystrokes to actions. These are
    /// parsed before `bindings`, so bindings later in the same section can still take precedence.
    #[serde(default)]
    pub(super) unbind: Option<IndexMap<String, UnbindTargetAction>>,
    /// This keymap section's bindings, as a JSON object mapping keystrokes to actions. The
    /// keystrokes key is a string representing a sequence of keystrokes to type, where the
    /// keystrokes are separated by whitespace. Each keystroke is a sequence of modifiers (`ctrl`,
    /// `alt`, `shift`, `fn`, `cmd`, `super`, or `win`) followed by a key, separated by `-`. The
    /// order of bindings does matter. When the same keystrokes are bound at the same context depth,
    /// the binding that occurs later in the file is preferred. For displaying keystrokes in the UI,
    /// the later binding for the same action is preferred.
    #[serde(default)]
    pub(super) bindings: Option<IndexMap<String, KeymapAction>>,
    #[serde(flatten)]
    pub(super) unrecognized_fields: IndexMap<String, Value>,
    // This struct intentionally uses permissive types for its fields, rather than validating during
    // deserialization. The purpose of this is to allow loading the portion of the keymap that doesn't
    // have errors. The downside of this is that the errors are not reported with line+column info.
    // Unfortunately the implementations of the `Spanned` types for preserving this information are
    // highly inconvenient (`serde_spanned`) and in some cases don't work at all here
    // (`json_spanned_>value`). Serde should really have builtin support for this.
}

impl KeymapSection {
    pub fn bindings(&self) -> impl DoubleEndedIterator<Item = (&String, &KeymapAction)> {
        self.bindings.iter().flatten()
    }
}

/// Keymap action as a JSON value, since it can either be null for no action, or the name of the
/// action, or an array of the name of the action and the action input.
///
/// Unlike the other json types involved in keymaps (including actions), this doc-comment will not
/// be included in the generated JSON schema, as it manually defines its `JsonSchema` impl. The
/// actual schema used for it is automatically generated in `KeymapFile::generate_json_schema`.
#[derive(Debug, Deserialize, Default, Clone)]
#[serde(transparent)]
pub struct KeymapAction(pub(super) Value);

impl std::fmt::Display for KeymapAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Value::String(s) => write!(f, "{}", s),
            Value::Array(arr) => {
                let strings: Vec<String> = arr.iter().map(|v| v.to_string()).collect();
                write!(f, "{}", strings.join(", "))
            }
            _ => write!(f, "{}", self.0),
        }
    }
}

impl JsonSchema for KeymapAction {
    /// This is used when generating the JSON schema for the `KeymapAction` type, so that it can
    /// reference the keymap action schema.
    fn schema_name() -> Cow<'static, str> {
        "KeymapAction".into()
    }

    /// This schema will be replaced with the full action schema in
    /// `KeymapFile::generate_json_schema`.
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        json_schema!(true)
    }
}

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(transparent)]
pub struct UnbindTargetAction(pub(super) Value);

impl JsonSchema for UnbindTargetAction {
    fn schema_name() -> Cow<'static, str> {
        "UnbindTargetAction".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        json_schema!(true)
    }
}

#[derive(Debug)]
#[must_use]
pub enum KeymapFileLoadResult {
    Success {
        key_bindings: Vec<KeyBinding>,
    },
    SomeFailedToLoad {
        key_bindings: Vec<KeyBinding>,
        error_message: MarkdownString,
    },
    JsonParseFailure {
        error: anyhow::Error,
    },
}
