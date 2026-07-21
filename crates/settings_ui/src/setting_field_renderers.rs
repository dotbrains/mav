use super::*;

/// Derives a human-readable label for assistive technology from a setting's
/// JSON path, e.g. `"buffer_font_size"` becomes `"Buffer Font Size"`.
fn a11y_label_for_json_path(json_path: Option<&'static str>) -> Option<SharedString> {
    json_path.map(|path| SharedString::from(path.to_title_case()))
}

struct CurrentSettingsValue<'a, T> {
    value: &'a T,
    disabled: bool,
}

fn get_current_value<'a, T>(
    settings_store: &'a SettingsStore,
    file: &SettingsUiFile,
    field: &'a SettingField<T>,
    cx: &'a App,
) -> Option<CurrentSettingsValue<'a, T>> {
    let user_store = AppState::global(cx).user_store.read(cx);
    let org_config = user_store.current_organization_configuration();

    let (_file, value) = settings_store.get_value_from_file(file.to_settings(), field.pick);
    let value = value?;

    let org_value = org_config
        .zip(field.organization_override)
        .and_then(|(org_config, org_override)| (org_override)(org_config));

    Some(CurrentSettingsValue {
        disabled: org_value.is_some(),
        value: org_value.unwrap_or(&value),
    })
}

pub(super) fn render_text_field<T: From<String> + Into<String> + AsRef<str> + Clone>(
    field: SettingField<T>,
    file: SettingsUiFile,
    metadata: Option<&SettingsFieldMetadata>,
    _window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let (_, initial_text) =
        SettingsStore::global(cx).get_value_from_file(file.to_settings(), field.pick);
    let initial_text = if metadata.is_some_and(|metadata| metadata.treat_missing_text_as_empty) {
        Some(
            initial_text
                .map(|text| text.as_ref().to_string())
                .unwrap_or_default(),
        )
    } else {
        initial_text
            .filter(|text| !text.as_ref().is_empty())
            .map(|text| text.as_ref().to_string())
    };

    // The JSON path uniquely identifies the setting this field edits, making
    // it a stable, collision-free element ID within the page.
    SettingsInputField::new(field.json_path.unwrap_or("settings-text-field"))
        .tab_index(0)
        .when_some(
            a11y_label_for_json_path(field.json_path),
            |editor, label| editor.aria_label(label),
        )
        .when_some(initial_text, |editor, text| editor.with_initial_text(text))
        .when_some(
            metadata.and_then(|metadata| metadata.placeholder),
            |editor, placeholder| editor.with_placeholder(placeholder),
        )
        .when(
            metadata.is_some_and(|metadata| metadata.display_confirm_button),
            |editor| editor.display_confirm_button(),
        )
        .when(
            metadata.is_some_and(|metadata| metadata.display_clear_button),
            |editor| editor.display_clear_button(),
        )
        .when(
            metadata.is_some_and(|metadata| metadata.confirm_on_focus_out),
            |editor| editor.confirm_on_focus_out(),
        )
        .on_confirm({
            move |new_text, window, cx| {
                update_settings_file(
                    file.clone(),
                    field.json_path,
                    window,
                    cx,
                    move |settings, app| {
                        (field.write)(settings, new_text.map(Into::into), app);
                    },
                )
                .log_err(); // todo(settings_ui) don't log err
            }
        })
        .into_any_element()
}

pub(super) fn render_toggle_button<B: Into<bool> + From<bool> + Copy>(
    field: SettingField<B>,
    file: SettingsUiFile,
    _metadata: Option<&SettingsFieldMetadata>,
    _window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let value = get_current_value(&SettingsStore::global(cx), &file, &field, cx);
    let (value, disabled) = value
        .map(|current_value| (*current_value.value, current_value.disabled))
        .unwrap_or((false.into(), false));

    let toggle_state = if value.into() {
        ToggleState::Selected
    } else {
        ToggleState::Unselected
    };

    Switch::new("toggle_button", toggle_state)
        .tab_index(0_isize)
        .when_some(a11y_label_for_json_path(field.json_path), |this, label| {
            this.aria_label(label)
        })
        .disabled(disabled)
        .on_click({
            move |state, window, cx| {
                telemetry::event!("Settings Change", setting = field.json_path, type = file.setting_type());

                let state = *state == ui::ToggleState::Selected;
                update_settings_file(file.clone(), field.json_path, window, cx, move |settings, app| {
                    (field.write)(settings, Some(state.into()), app);
                })
                .log_err(); // todo(settings_ui) don't log err
            }
        })
        .into_any_element()
}

pub(super) fn render_editable_number_field<T: NumberFieldType + Send + Sync>(
    field: SettingField<T>,
    file: SettingsUiFile,
    _metadata: Option<&SettingsFieldMetadata>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let (_, value) = SettingsStore::global(cx).get_value_from_file(file.to_settings(), field.pick);
    let value = value.copied().unwrap_or_else(T::min_value);

    let id = field
        .json_path
        .map(|p| format!("numeric_stepper_{}", p))
        .unwrap_or_else(|| "numeric_stepper".to_string());

    NumberField::new(id, value, window, cx)
        .mode(NumberFieldMode::Edit, cx)
        .tab_index(0_isize)
        .when_some(a11y_label_for_json_path(field.json_path), |this, label| {
            this.aria_label(label)
        })
        .on_change({
            move |value, window, cx| {
                let value = *value;
                update_settings_file(
                    file.clone(),
                    field.json_path,
                    window,
                    cx,
                    move |settings, app| {
                        (field.write)(settings, Some(value), app);
                    },
                )
                .log_err(); // todo(settings_ui) don't log err
            }
        })
        .into_any_element()
}

pub(super) fn render_dropdown<T>(
    field: SettingField<T>,
    file: SettingsUiFile,
    metadata: Option<&SettingsFieldMetadata>,
    _window: &mut Window,
    cx: &mut App,
) -> AnyElement
where
    T: strum::VariantArray + strum::VariantNames + Copy + PartialEq + Send + Sync + 'static,
{
    let variants = || -> &'static [T] { <T as strum::VariantArray>::VARIANTS };
    let labels = || -> &'static [&'static str] { <T as strum::VariantNames>::VARIANTS };
    let should_do_titlecase = metadata
        .and_then(|metadata| metadata.should_do_titlecase)
        .unwrap_or(true);

    let current_value = get_current_value(&SettingsStore::global(cx), &file, &field, cx);
    let (current_value, disabled) = current_value
        .map(|current_value| (*current_value.value, current_value.disabled))
        .unwrap_or((variants()[0], false));

    EnumVariantDropdown::new("dropdown", current_value, variants(), labels(), {
        move |value, window, cx| {
            if value == current_value {
                return;
            }
            update_settings_file(
                file.clone(),
                field.json_path,
                window,
                cx,
                move |settings, app| {
                    (field.write)(settings, Some(value), app);
                },
            )
            .log_err(); // todo(settings_ui) don't log err
        }
    })
    .when_some(a11y_label_for_json_path(field.json_path), |this, label| {
        this.aria_label(label)
    })
    .disabled(disabled)
    .tab_index(0)
    .title_case(should_do_titlecase)
    .into_any_element()
}

pub(super) fn render_picker_trigger_button(id: SharedString, label: SharedString) -> Button {
    Button::new(id, label)
        .aria_role(Role::ComboBox)
        .tab_index(0_isize)
        .style(ButtonStyle::Outlined)
        .size(ButtonSize::Medium)
        .end_icon(
            Icon::new(IconName::ChevronUpDown)
                .size(IconSize::Small)
                .color(Color::Muted),
        )
}

/// Wires the Expand/Collapse accessibility actions on a picker trigger button to
/// the popover handle, so assistive technology can open and close the picker
/// (used by UIA on Windows and AX on macOS; Linux/AT-SPI uses the click action).
fn wire_picker_trigger_a11y<M: gpui::ManagedView>(
    button: Button,
    handle: ui::PopoverMenuHandle<M>,
) -> Button {
    let show_handle = handle.clone();
    let hide_handle = handle;
    button
        .on_a11y_action(gpui::accesskit::Action::Expand, move |_, window, cx| {
            show_handle.show(window, cx);
        })
        .on_a11y_action(gpui::accesskit::Action::Collapse, move |_, _window, cx| {
            hide_handle.hide(cx);
        })
}

pub(super) fn render_font_picker(
    field: SettingField<settings::FontFamilyName>,
    file: SettingsUiFile,
    _metadata: Option<&SettingsFieldMetadata>,
    _window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let current_value = SettingsStore::global(cx)
        .get_value_from_file(file.to_settings(), field.pick)
        .1
        .cloned()
        .map_or_else(|| SharedString::default(), |value| value.into_gpui());

    let handle = ui::PopoverMenuHandle::default();
    PopoverMenu::new("font-picker")
        .trigger(wire_picker_trigger_a11y(
            render_picker_trigger_button(
                "font_family_picker_trigger".into(),
                current_value.clone(),
            )
            .when_some(a11y_label_for_json_path(field.json_path), |this, label| {
                this.aria_label(format!("{}: {}", label, current_value.clone()))
            }),
            handle.clone(),
        ))
        .menu(move |window, cx| {
            let file = file.clone();
            let current_value = current_value.clone();

            Some(cx.new(move |cx| {
                font_picker(
                    current_value,
                    move |font_name, window, cx| {
                        update_settings_file(
                            file.clone(),
                            field.json_path,
                            window,
                            cx,
                            move |settings, app| {
                                (field.write)(settings, Some(font_name.to_string().into()), app);
                            },
                        )
                        .log_err(); // todo(settings_ui) don't log err
                    },
                    window,
                    cx,
                )
            }))
        })
        .anchor(gpui::Anchor::TopLeft)
        .offset(gpui::Point {
            x: px(0.0),
            y: px(2.0),
        })
        .with_handle(handle)
        .into_any_element()
}

pub(super) fn render_theme_picker(
    field: SettingField<settings::ThemeName>,
    file: SettingsUiFile,
    _metadata: Option<&SettingsFieldMetadata>,
    _window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let (_, value) = SettingsStore::global(cx).get_value_from_file(file.to_settings(), field.pick);
    let current_value = value
        .cloned()
        .map(|theme_name| theme_name.0.into())
        .unwrap_or_else(|| cx.theme().name.clone());

    let handle = ui::PopoverMenuHandle::default();
    PopoverMenu::new("theme-picker")
        .trigger(wire_picker_trigger_a11y(
            render_picker_trigger_button("theme_picker_trigger".into(), current_value.clone())
                .when_some(a11y_label_for_json_path(field.json_path), |this, label| {
                    this.aria_label(format!("{}: {}", label, current_value.clone()))
                }),
            handle.clone(),
        ))
        .menu(move |window, cx| {
            Some(cx.new(|cx| {
                let file = file.clone();
                let current_value = current_value.clone();
                theme_picker(
                    current_value,
                    move |theme_name, window, cx| {
                        update_settings_file(
                            file.clone(),
                            field.json_path,
                            window,
                            cx,
                            move |settings, app| {
                                (field.write)(
                                    settings,
                                    Some(settings::ThemeName(theme_name.into())),
                                    app,
                                );
                            },
                        )
                        .log_err(); // todo(settings_ui) don't log err
                    },
                    window,
                    cx,
                )
            }))
        })
        .anchor(gpui::Anchor::TopLeft)
        .offset(gpui::Point {
            x: px(0.0),
            y: px(2.0),
        })
        .with_handle(handle)
        .into_any_element()
}

pub(super) fn render_icon_theme_picker(
    field: SettingField<settings::IconThemeName>,
    file: SettingsUiFile,
    _metadata: Option<&SettingsFieldMetadata>,
    _window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let (_, value) = SettingsStore::global(cx).get_value_from_file(file.to_settings(), field.pick);
    let current_value = value
        .cloned()
        .map(|theme_name| theme_name.0.into())
        .unwrap_or_else(|| cx.theme().name.clone());

    let handle = ui::PopoverMenuHandle::default();
    PopoverMenu::new("icon-theme-picker")
        .trigger(wire_picker_trigger_a11y(
            render_picker_trigger_button("icon_theme_picker_trigger".into(), current_value.clone())
                .when_some(a11y_label_for_json_path(field.json_path), |this, label| {
                    this.aria_label(format!("{}: {}", label, current_value.clone()))
                }),
            handle.clone(),
        ))
        .menu(move |window, cx| {
            Some(cx.new(|cx| {
                let file = file.clone();
                let current_value = current_value.clone();
                icon_theme_picker(
                    current_value,
                    move |theme_name, window, cx| {
                        update_settings_file(
                            file.clone(),
                            field.json_path,
                            window,
                            cx,
                            move |settings, app| {
                                (field.write)(
                                    settings,
                                    Some(settings::IconThemeName(theme_name.into())),
                                    app,
                                );
                            },
                        )
                        .log_err(); // todo(settings_ui) don't log err
                    },
                    window,
                    cx,
                )
            }))
        })
        .anchor(gpui::Anchor::TopLeft)
        .offset(gpui::Point {
            x: px(0.0),
            y: px(2.0),
        })
        .with_handle(handle)
        .into_any_element()
}
