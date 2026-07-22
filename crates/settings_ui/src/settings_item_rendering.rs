use crate::*;

/// Shared layout for both JSON-backed and non-JSON-backed setting items.
///
/// Renders title + description on the left, control on the right, with
/// optional reset button and copy-link icon.
pub(crate) fn render_settings_item_layout(
    settings_window: &SettingsWindow,
    title: &'static str,
    description: &'static str,
    control: AnyElement,
    reset_fn: Option<Box<dyn Fn(&mut Window, &mut App)>>,
    modified_in: Option<String>,
    json_path: Option<&'static str>,
    sub_field: bool,
    cx: &mut Context<'_, SettingsWindow>,
) -> Stateful<Div> {
    h_flex()
        .id(title)
        .role(Role::Group)
        .aria_label(SharedString::new_static(title))
        .min_w_0()
        .justify_between()
        .child(
            v_flex()
                .relative()
                .w_full()
                .max_w_2_3()
                .min_w_0()
                .child(
                    h_flex()
                        .w_full()
                        .gap_1()
                        .child(Label::new(SharedString::new_static(title)))
                        .when_some(reset_fn, |this, reset_to_default| {
                            this.child(
                                IconButton::new("reset-to-default-btn", IconName::Undo)
                                    .icon_color(Color::Muted)
                                    .icon_size(IconSize::Small)
                                    .aria_label("Reset to Default")
                                    .tooltip(Tooltip::text("Reset to Default"))
                                    .on_click(move |_, window, cx| {
                                        reset_to_default(window, cx);
                                    }),
                            )
                        })
                        .when_some(modified_in, |this, modified_in| {
                            this.child(
                                Label::new(format!("\u{2014}  Modified in {modified_in}"))
                                    .color(Color::Muted)
                                    .size(LabelSize::Small),
                            )
                        }),
                )
                .child(
                    Label::new(SharedString::new_static(description))
                        .size(LabelSize::Small)
                        .color(Color::Muted)
                        .render_code_spans(),
                ),
        )
        .child(control)
        .when(settings_window.sub_page_stack.is_empty(), |this| {
            this.child(render_settings_item_link(
                description,
                json_path,
                sub_field,
                settings_window,
                cx,
            ))
        })
}

pub(crate) fn render_settings_item(
    settings_window: &SettingsWindow,
    setting_item: &SettingItem,
    file: SettingsUiFile,
    control: AnyElement,
    sub_field: bool,
    cx: &mut Context<'_, SettingsWindow>,
) -> Stateful<Div> {
    let (found_in_file, _) = setting_item.field.file_set_in(file.clone(), cx);
    let file_set_in = SettingsUiFile::from_settings(found_in_file.clone());

    let reset_fn = if sub_field {
        None
    } else {
        setting_item
            .field
            .reset_to_default_fn(&file, &found_in_file, cx)
    };

    let modified_in = file_set_in
        .filter(|f| f != &file)
        .and_then(|f| settings_window.display_name(&f));

    let control = if setting_item.field.is_overridden_by_organization(cx) {
        h_flex()
            .gap_2()
            .child(
                div()
                    .id(format!(
                        "{}-organization-configuration-warning",
                        setting_item.title
                    ))
                    .child(
                        Icon::new(IconName::Warning)
                            .size(IconSize::Small)
                            .color(Color::Warning),
                    )
                    .tooltip(|_, cx| {
                        Tooltip::with_meta(
                            "Overridden by Organization",
                            None,
                            "Contact your organization admins to adjust this setting.",
                            cx,
                        )
                    }),
            )
            .child(control)
            .into_any_element()
    } else {
        control
    };

    render_settings_item_layout(
        settings_window,
        setting_item.title,
        setting_item.description,
        control,
        reset_fn,
        modified_in,
        setting_item.field.json_path(),
        sub_field,
        cx,
    )
}

pub(crate) fn render_settings_item_link(
    id: impl Into<ElementId>,
    json_path: Option<&'static str>,
    sub_field: bool,
    settings_window: &SettingsWindow,
    cx: &mut Context<'_, SettingsWindow>,
) -> impl IntoElement {
    let copied_link_matches =
        json_path.is_some() && json_path == settings_window.last_copied_link_path;

    let (link_icon, link_icon_color) = if copied_link_matches {
        (IconName::Check, Color::Success)
    } else {
        (IconName::Link, Color::Muted)
    };

    div()
        .absolute()
        .top(rems_from_px(18.))
        .map(|this| {
            if sub_field {
                this.visible_on_hover("setting-sub-item")
                    .left(rems_from_px(-8.5))
            } else {
                this.visible_on_hover("setting-item")
                    .left(rems_from_px(-22.))
            }
        })
        .child(
            IconButton::new((id.into(), "copy-link-btn"), link_icon)
                .icon_color(link_icon_color)
                .icon_size(IconSize::Small)
                .shape(IconButtonShape::Square)
                .aria_label("Copy Link")
                .tooltip(Tooltip::text("Copy Link"))
                .when_some(json_path, |this, path| {
                    this.on_click(cx.listener(move |this, _, _, cx| {
                        let link = format!("mav://settings/{}", path);
                        cx.write_to_clipboard(ClipboardItem::new_string(link));
                        this.last_copied_link_path = Some(path);
                        cx.notify();
                    }))
                }),
        )
}
