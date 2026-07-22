use super::*;

struct HumanizedActionNameCache {
    cache: HashMap<&'static str, SharedString>,
}

impl HumanizedActionNameCache {
    fn new(cx: &App) -> Self {
        let cache = HashMap::from_iter(cx.all_action_names().iter().map(|&action_name| {
            (
                action_name,
                command_palette::humanize_action_name(action_name).into(),
            )
        }));
        Self { cache }
    }

    fn get(&self, action_name: &'static str) -> SharedString {
        match self.cache.get(action_name) {
            Some(name) => name.clone(),
            None => action_name.into(),
        }
    }
}

#[derive(Clone)]
struct KeyBinding {
    keystrokes: Rc<[KeybindingKeystroke]>,
    source: KeybindSource,
}

impl KeyBinding {
    fn new(binding: &gpui::KeyBinding, source: KeybindSource) -> Self {
        Self {
            keystrokes: Rc::from(binding.keystrokes()),
            source,
        }
    }
}

#[derive(Clone)]
struct KeybindInformation {
    keystroke_text: SharedString,
    binding: KeyBinding,
    context: KeybindContextString,
    source: KeybindSource,
    is_no_action: bool,
    is_unbound_by_unbind: bool,
}

impl KeybindInformation {
    fn get_action_mapping(&self) -> ActionMapping {
        ActionMapping {
            keystrokes: self.binding.keystrokes.clone(),
            context: self.context.local().cloned(),
        }
    }
}

#[derive(Clone)]
struct ActionInformation {
    name: &'static str,
    humanized_name: SharedString,
    arguments: Option<SyntaxHighlightedText>,
    documentation: Option<&'static str>,
    has_schema: bool,
}

impl ActionInformation {
    fn new(
        action_name: &'static str,
        action_arguments: Option<SyntaxHighlightedText>,
        actions_with_schemas: &HashSet<&'static str>,
        action_documentation: &HashMap<&'static str, &'static str>,
        action_name_cache: &HumanizedActionNameCache,
    ) -> Self {
        Self {
            humanized_name: action_name_cache.get(action_name),
            has_schema: actions_with_schemas.contains(action_name),
            arguments: action_arguments,
            documentation: action_documentation.get(action_name).copied(),
            name: action_name,
        }
    }
}

#[derive(Clone)]
enum ProcessedBinding {
    Mapped(KeybindInformation, ActionInformation),
    Unmapped(ActionInformation),
}

impl ProcessedBinding {
    fn new_mapped(
        keystroke_text: impl Into<SharedString>,
        binding: KeyBinding,
        context: KeybindContextString,
        source: KeybindSource,
        is_no_action: bool,
        is_unbound_by_unbind: bool,
        action_information: ActionInformation,
    ) -> Self {
        Self::Mapped(
            KeybindInformation {
                keystroke_text: keystroke_text.into(),
                binding,
                context,
                source,
                is_no_action,
                is_unbound_by_unbind,
            },
            action_information,
        )
    }

    fn is_unbound(&self) -> bool {
        matches!(self, Self::Unmapped(_))
    }

    fn get_action_mapping(&self) -> Option<ActionMapping> {
        self.keybind_information()
            .map(|keybind| keybind.get_action_mapping())
    }

    fn keystrokes(&self) -> Option<&[KeybindingKeystroke]> {
        self.key_binding()
            .map(|binding| binding.keystrokes.as_ref())
    }

    fn keybind_information(&self) -> Option<&KeybindInformation> {
        match self {
            Self::Mapped(keybind_information, _) => Some(keybind_information),
            Self::Unmapped(_) => None,
        }
    }

    fn keybind_source(&self) -> Option<KeybindSource> {
        self.keybind_information().map(|keybind| keybind.source)
    }

    fn context(&self) -> Option<&KeybindContextString> {
        self.keybind_information().map(|keybind| &keybind.context)
    }

    fn key_binding(&self) -> Option<&KeyBinding> {
        self.keybind_information().map(|keybind| &keybind.binding)
    }

    fn is_no_action(&self) -> bool {
        self.keybind_information()
            .is_some_and(|keybind| keybind.is_no_action)
    }

    fn is_unbound_by_unbind(&self) -> bool {
        self.keybind_information()
            .is_some_and(|keybind| keybind.is_unbound_by_unbind)
    }

    fn keystroke_text(&self) -> Option<&SharedString> {
        self.keybind_information()
            .map(|binding| &binding.keystroke_text)
    }

    fn action(&self) -> &ActionInformation {
        match self {
            Self::Mapped(_, action) | Self::Unmapped(action) => action,
        }
    }

    fn cmp(&self, other: &Self) -> cmp::Ordering {
        match (self, other) {
            (Self::Mapped(keybind1, action1), Self::Mapped(keybind2, action2)) => {
                match keybind1.source.cmp(&keybind2.source) {
                    cmp::Ordering::Equal => action1.humanized_name.cmp(&action2.humanized_name),
                    ordering => ordering,
                }
            }
            (Self::Mapped(_, _), Self::Unmapped(_)) => cmp::Ordering::Less,
            (Self::Unmapped(_), Self::Mapped(_, _)) => cmp::Ordering::Greater,
            (Self::Unmapped(action1), Self::Unmapped(action2)) => {
                action1.humanized_name.cmp(&action2.humanized_name)
            }
        }
    }
}

#[derive(Clone, Debug, IntoElement, PartialEq, Eq, Hash)]
enum KeybindContextString {
    Global,
    Local(SharedString, Arc<Language>),
}

impl KeybindContextString {
    const GLOBAL: SharedString = SharedString::new_static("<global>");

    pub fn local(&self) -> Option<&SharedString> {
        match self {
            KeybindContextString::Global => None,
            KeybindContextString::Local(name, _) => Some(name),
        }
    }

    pub fn local_str(&self) -> Option<&str> {
        match self {
            KeybindContextString::Global => None,
            KeybindContextString::Local(name, _) => Some(name),
        }
    }
}

impl RenderOnce for KeybindContextString {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        match self {
            KeybindContextString::Global => {
                muted_styled_text(KeybindContextString::GLOBAL, cx).into_any_element()
            }
            KeybindContextString::Local(name, language) => {
                SyntaxHighlightedText::new(name, language).into_any_element()
            }
        }
    }
}

fn muted_styled_text(text: SharedString, cx: &App) -> StyledText {
    let len = text.len();
    StyledText::new(text).with_highlights([(
        0..len,
        gpui::HighlightStyle::color(cx.theme().colors().text_muted),
    )])
}
