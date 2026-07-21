use super::*;

pub(crate) struct SettingField<T: 'static> {
    pub(crate) pick: fn(&SettingsContent) -> Option<&T>,
    pub(crate) write: fn(&mut SettingsContent, Option<T>, &App),
    /// Tells us whether the setting is overridden by the currently selected
    /// organization's settings. Takes the organization configuration and the
    /// resolved settings value, and returns `Some(...)` if the organization
    /// overrides the setting, otherwise `None`.
    pub(crate) organization_override: Option<fn(&OrganizationConfiguration) -> Option<&T>>,

    /// A json-path-like string that gives a unique-ish string that identifies
    /// where in the JSON the setting is defined.
    ///
    /// The syntax is `jq`-like, but modified slightly to be URL-safe (and
    /// without the leading dot), e.g. `foo.bar`.
    ///
    /// They are URL-safe (this is important since links are the main use-case
    /// for these paths).
    ///
    /// There are a couple of special cases:
    /// - discrimminants are represented with a trailing `$`, for example
    /// `terminal.working_directory$`. This is to distinguish the discrimminant
    /// setting (i.e. the setting that changes whether the value is a string or
    /// an object) from the setting in the case that it is a string.
    /// - language-specific settings begin `languages.$(language)`. Links
    /// targeting these settings should take the form `languages/Rust/...`, for
    /// example, but are not currently supported.
    pub(crate) json_path: Option<&'static str>,
}

impl<T: 'static> Clone for SettingField<T> {
    fn clone(&self) -> Self {
        *self
    }
}

// manual impl because derive puts a Copy bound on T, which is inaccurate in our case
impl<T: 'static> Copy for SettingField<T> {}

/// Helper for unimplemented settings, used in combination with `SettingField::unimplemented`
/// to keep the setting around in the UI with valid pick and write implementations, but don't actually try to render it.
/// TODO(settings_ui): In non-dev builds (`#[cfg(not(debug_assertions))]`) make this render as edit-in-json
#[derive(Clone, Copy)]
pub(crate) struct UnimplementedSettingField;

impl PartialEq for UnimplementedSettingField {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl<T: 'static> SettingField<T> {
    /// Helper for settings with types that are not yet implemented.
    #[allow(unused)]
    pub(crate) fn unimplemented(self) -> SettingField<UnimplementedSettingField> {
        SettingField {
            pick: |_| Some(&UnimplementedSettingField),
            write: |_, _, _| unreachable!(),
            organization_override: None,
            json_path: self.json_path,
        }
    }
}

pub(crate) trait AnySettingField {
    fn as_any(&self) -> &dyn Any;
    fn type_name(&self) -> &'static str;
    fn type_id(&self) -> TypeId;
    // Returns the file this value was set in and true, or File::Default and false to indicate it was not found in any file (missing default)
    fn file_set_in(&self, file: SettingsUiFile, cx: &App) -> (settings::SettingsFile, bool);
    fn reset_to_default_fn(
        &self,
        current_file: &SettingsUiFile,
        file_set_in: &settings::SettingsFile,
        cx: &App,
    ) -> Option<Box<dyn Fn(&mut Window, &mut App)>>;

    fn json_path(&self) -> Option<&'static str>;

    fn is_overridden_by_organization(&self, cx: &App) -> bool;
}

impl<T: PartialEq + Clone + Send + Sync + 'static> AnySettingField for SettingField<T> {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn type_name(&self) -> &'static str {
        type_name::<T>()
    }

    fn type_id(&self) -> TypeId {
        TypeId::of::<T>()
    }

    fn file_set_in(&self, file: SettingsUiFile, cx: &App) -> (settings::SettingsFile, bool) {
        let (file, value) = cx
            .global::<SettingsStore>()
            .get_value_from_file(file.to_settings(), self.pick);
        return (file, value.is_some());
    }

    fn reset_to_default_fn(
        &self,
        current_file: &SettingsUiFile,
        file_set_in: &settings::SettingsFile,
        cx: &App,
    ) -> Option<Box<dyn Fn(&mut Window, &mut App)>> {
        if file_set_in == &settings::SettingsFile::Default {
            return None;
        }
        if file_set_in != &current_file.to_settings() {
            return None;
        }
        let this = *self;
        let store = SettingsStore::global(cx);
        let default_value = (this.pick)(store.raw_default_settings());
        let is_default = store
            .get_content_for_file(file_set_in.clone())
            .map_or(None, this.pick)
            == default_value;
        if is_default {
            return None;
        }
        let current_file = current_file.clone();

        return Some(Box::new(move |window, cx| {
            let store = SettingsStore::global(cx);
            let default_value = (this.pick)(store.raw_default_settings());
            let is_set_somewhere_other_than_default = store
                .get_value_up_to_file(current_file.to_settings(), this.pick)
                .0
                != settings::SettingsFile::Default;
            let value_to_set = if is_set_somewhere_other_than_default {
                default_value.cloned()
            } else {
                None
            };
            update_settings_file(
                current_file.clone(),
                None,
                window,
                cx,
                move |settings, app| {
                    (this.write)(settings, value_to_set, app);
                },
            )
            // todo(settings_ui): Don't log err
            .log_err();
        }));
    }

    fn json_path(&self) -> Option<&'static str> {
        self.json_path
    }

    fn is_overridden_by_organization(&self, cx: &App) -> bool {
        let Some(org_override) = self.organization_override else {
            return false;
        };

        let user_store = AppState::global(cx).user_store.read(cx);
        let Some(org_config) = user_store.current_organization_configuration() else {
            return false;
        };

        (org_override)(&org_config).is_some()
    }
}

#[derive(Default, Clone)]
pub(crate) struct SettingFieldRenderer {
    pub(crate) renderers: Rc<
        RefCell<
            HashMap<
                TypeId,
                Box<
                    dyn Fn(
                        &SettingsWindow,
                        &SettingItem,
                        SettingsUiFile,
                        Option<&SettingsFieldMetadata>,
                        bool,
                        &mut Window,
                        &mut Context<SettingsWindow>,
                    ) -> Stateful<Div>,
                >,
            >,
        >,
    >,
}

impl Global for SettingFieldRenderer {}

impl SettingFieldRenderer {
    pub(crate) fn add_basic_renderer<T: 'static>(
        &mut self,
        render_control: impl Fn(
            SettingField<T>,
            SettingsUiFile,
            Option<&SettingsFieldMetadata>,
            &mut Window,
            &mut App,
        ) -> AnyElement
        + 'static,
    ) -> &mut Self {
        self.add_renderer(
            move |settings_window: &SettingsWindow,
                  item: &SettingItem,
                  field: SettingField<T>,
                  settings_file: SettingsUiFile,
                  metadata: Option<&SettingsFieldMetadata>,
                  sub_field: bool,
                  window: &mut Window,
                  cx: &mut Context<SettingsWindow>| {
                render_settings_item(
                    settings_window,
                    item,
                    settings_file.clone(),
                    render_control(field, settings_file, metadata, window, cx),
                    sub_field,
                    cx,
                )
            },
        )
    }

    pub(crate) fn add_renderer<T: 'static>(
        &mut self,
        renderer: impl Fn(
            &SettingsWindow,
            &SettingItem,
            SettingField<T>,
            SettingsUiFile,
            Option<&SettingsFieldMetadata>,
            bool,
            &mut Window,
            &mut Context<SettingsWindow>,
        ) -> Stateful<Div>
        + 'static,
    ) -> &mut Self {
        let key = TypeId::of::<T>();
        let renderer = Box::new(
            move |settings_window: &SettingsWindow,
                  item: &SettingItem,
                  settings_file: SettingsUiFile,
                  metadata: Option<&SettingsFieldMetadata>,
                  sub_field: bool,
                  window: &mut Window,
                  cx: &mut Context<SettingsWindow>| {
                let field = *item
                    .field
                    .as_ref()
                    .as_any()
                    .downcast_ref::<SettingField<T>>()
                    .unwrap();
                renderer(
                    settings_window,
                    item,
                    field,
                    settings_file,
                    metadata,
                    sub_field,
                    window,
                    cx,
                )
            },
        );
        self.renderers.borrow_mut().insert(key, renderer);
        self
    }
}

pub(crate) struct NonFocusableHandle {
    pub(crate) handle: FocusHandle,
    _subscription: Subscription,
}

impl NonFocusableHandle {
    pub(crate) fn new(
        tab_index: isize,
        tab_stop: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        let handle = cx.focus_handle().tab_index(tab_index).tab_stop(tab_stop);
        Self::from_handle(handle, window, cx)
    }

    pub(crate) fn from_handle(
        handle: FocusHandle,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| {
            let _subscription = cx.on_focus(&handle, window, {
                move |_, window, cx| {
                    window.focus_next(cx);
                }
            });
            Self {
                handle,
                _subscription,
            }
        })
    }
}

impl Focusable for NonFocusableHandle {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.handle.clone()
    }
}

#[derive(Default)]
pub(crate) struct SettingsFieldMetadata {
    pub(crate) placeholder: Option<&'static str>,
    pub(crate) should_do_titlecase: Option<bool>,
    pub(crate) display_confirm_button: bool,
    pub(crate) display_clear_button: bool,
    pub(crate) confirm_on_focus_out: bool,
    pub(crate) treat_missing_text_as_empty: bool,
}
