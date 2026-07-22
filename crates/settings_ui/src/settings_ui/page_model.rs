use super::*;

pub(crate) struct SearchDocument {
    id: usize,
    words: Vec<String>,
}

pub(crate) struct SearchIndex {
    documents: Vec<SearchDocument>,
    fuzzy_match_candidates: Vec<StringMatchCandidate>,
    key_lut: Vec<SearchKeyLUTEntry>,
}

pub(crate) struct SearchKeyLUTEntry {
    page_index: usize,
    header_index: usize,
    item_index: usize,
    json_path: Option<&'static str>,
}

pub(crate) struct SubPage {
    link: SubPageLink,
    section_header: SharedString,
    scroll_handle: ScrollHandle,
}

impl SubPage {
    fn new(link: SubPageLink, section_header: SharedString) -> Self {
        if link.r#type == SubPageType::Language
            && let Some(mut active_language_global) = active_language_mut()
        {
            active_language_global.replace(link.title.clone());
        }

        SubPage {
            link,
            section_header,
            scroll_handle: ScrollHandle::new(),
        }
    }
}

impl Drop for SubPage {
    fn drop(&mut self) {
        if self.link.r#type == SubPageType::Language
            && let Some(mut active_language_global) = active_language_mut()
            && active_language_global
                .as_ref()
                .is_some_and(|language_name| language_name == &self.link.title)
        {
            active_language_global.take();
        }
    }
}

#[derive(Debug)]
pub(crate) struct NavBarEntry {
    title: &'static str,
    is_root: bool,
    expanded: bool,
    page_index: usize,
    item_index: Option<usize>,
    focus_handle: FocusHandle,
}

pub(crate) struct SettingsPage {
    title: &'static str,
    items: Box<[SettingsPageItem]>,
}

#[derive(PartialEq)]
pub(crate) enum SettingsPageItem {
    SectionHeader(&'static str),
    SettingItem(SettingItem),
    SubPageLink(SubPageLink),
    DynamicItem(DynamicItem),
    ActionLink(ActionLink),
}

impl std::fmt::Debug for SettingsPageItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SettingsPageItem::SectionHeader(header) => write!(f, "SectionHeader({})", header),
            SettingsPageItem::SettingItem(setting_item) => {
                write!(f, "SettingItem({})", setting_item.title)
            }
            SettingsPageItem::SubPageLink(sub_page_link) => {
                write!(f, "SubPageLink({})", sub_page_link.title)
            }
            SettingsPageItem::DynamicItem(dynamic_item) => {
                write!(f, "DynamicItem({})", dynamic_item.discriminant.title)
            }
            SettingsPageItem::ActionLink(action_link) => {
                write!(f, "ActionLink({})", action_link.title)
            }
        }
    }
}

impl SettingsPageItem {
    fn header_text(&self) -> Option<&'static str> {
        match self {
            SettingsPageItem::SectionHeader(header) => Some(header),
            _ => None,
        }
    }

    fn render(
        &self,
        settings_window: &SettingsWindow,
        item_index: usize,
        bottom_border: bool,
        extra_bottom_padding: bool,
        window: &mut Window,
        cx: &mut Context<SettingsWindow>,
    ) -> AnyElement {
        let file = settings_window.current_file.clone();

        let apply_padding = |element: Stateful<Div>| -> Stateful<Div> {
            let element = element.pt_4();
            if extra_bottom_padding {
                element.pb_10()
            } else {
                element.pb_4()
            }
        };

        let mut render_setting_item_inner =
            |setting_item: &SettingItem,
             padding: bool,
             sub_field: bool,
             cx: &mut Context<SettingsWindow>| {
                let renderer = cx.default_global::<SettingFieldRenderer>().clone();
                let (_, found) = setting_item.field.file_set_in(file.clone(), cx);

                let renderers = renderer.renderers.borrow();

                let field_renderer =
                    renderers.get(&AnySettingField::type_id(setting_item.field.as_ref()));
                let field_renderer_or_warning =
                    field_renderer.ok_or("NO RENDERER").and_then(|renderer| {
                        if cfg!(debug_assertions) && !found {
                            Err("NO DEFAULT")
                        } else {
                            Ok(renderer)
                        }
                    });

                let field = match field_renderer_or_warning {
                    Ok(field_renderer) => window.with_id(item_index, |window| {
                        field_renderer(
                            settings_window,
                            setting_item,
                            file.clone(),
                            setting_item.metadata.as_deref(),
                            sub_field,
                            window,
                            cx,
                        )
                    }),
                    Err(warning) => render_settings_item(
                        settings_window,
                        setting_item,
                        file.clone(),
                        Button::new("error-warning", warning)
                            .style(ButtonStyle::Outlined)
                            .size(ButtonSize::Medium)
                            .start_icon(Icon::new(IconName::Debug).color(Color::Error))
                            .tab_index(0_isize)
                            .tooltip(Tooltip::text(setting_item.field.type_name()))
                            .into_any_element(),
                        sub_field,
                        cx,
                    ),
                };

                let field = if padding {
                    field.map(apply_padding)
                } else {
                    field
                };

                (field, field_renderer_or_warning.is_ok())
            };

        match self {
            SettingsPageItem::SectionHeader(header) => {
                SettingsSectionHeader::new(SharedString::new_static(header)).into_any_element()
            }
            SettingsPageItem::SettingItem(setting_item) => {
                let (field_with_padding, _) =
                    render_setting_item_inner(setting_item, true, false, cx);

                v_flex()
                    .group("setting-item")
                    .px_8()
                    .child(field_with_padding)
                    .when(bottom_border, |this| this.child(Divider::horizontal()))
                    .into_any_element()
            }
            SettingsPageItem::SubPageLink(sub_page_link) => v_flex()
                .group("setting-item")
                .px_8()
                .child(
                    h_flex()
                        .id(sub_page_link.title.clone())
                        .w_full()
                        .min_w_0()
                        .justify_between()
                        .map(apply_padding)
                        .child(
                            v_flex()
                                .relative()
                                .w_full()
                                .max_w_1_2()
                                .child(Label::new(sub_page_link.title.clone()))
                                .when_some(
                                    sub_page_link.description.as_ref(),
                                    |this, description| {
                                        this.child(
                                            Label::new(description.clone())
                                                .size(LabelSize::Small)
                                                .color(Color::Muted),
                                        )
                                    },
                                ),
                        )
                        .child(
                            Button::new(
                                ("sub-page".into(), sub_page_link.title.clone()),
                                "Configure",
                            )
                            .aria_label(format!("Configure {}", sub_page_link.title))
                            .tab_index(0_isize)
                            .end_icon(
                                Icon::new(IconName::ChevronRight)
                                    .size(IconSize::Small)
                                    .color(Color::Muted),
                            )
                            .style(ButtonStyle::OutlinedGhost)
                            .size(ButtonSize::Medium)
                            .on_click({
                                let sub_page_link = sub_page_link.clone();
                                cx.listener(move |this, _, window, cx| {
                                    let header_text = this
                                        .sub_page_stack
                                        .last()
                                        .map(|sub_page| sub_page.link.title.clone())
                                        .or_else(|| {
                                            this.current_page()
                                                .items
                                                .iter()
                                                .take(item_index)
                                                .rev()
                                                .find_map(|item| {
                                                    item.header_text().map(SharedString::new_static)
                                                })
                                        });

                                    let Some(header) = header_text else {
                                        unreachable!(
                                            "All items always have a section header above them"
                                        )
                                    };

                                    this.push_sub_page(sub_page_link.clone(), header, window, cx)
                                })
                            }),
                        )
                        .child(render_settings_item_link(
                            sub_page_link.title.clone(),
                            sub_page_link.json_path,
                            false,
                            settings_window,
                            cx,
                        )),
                )
                .when(bottom_border, |this| this.child(Divider::horizontal()))
                .into_any_element(),
            SettingsPageItem::DynamicItem(DynamicItem {
                discriminant: discriminant_setting_item,
                pick_discriminant,
                fields,
            }) => {
                let file = file.to_settings();
                let discriminant = SettingsStore::global(cx)
                    .get_value_from_file(file, *pick_discriminant)
                    .1;

                let (discriminant_element, rendered_ok) =
                    render_setting_item_inner(discriminant_setting_item, true, false, cx);

                let has_sub_fields =
                    rendered_ok && discriminant.is_some_and(|d| !fields[d].is_empty());

                let mut content = v_flex()
                    .id("dynamic-item")
                    .child(
                        div()
                            .group("setting-item")
                            .px_8()
                            .child(discriminant_element.when(has_sub_fields, |this| this.pb_4())),
                    )
                    .when(!has_sub_fields && bottom_border, |this| {
                        this.child(h_flex().px_8().child(Divider::horizontal()))
                    });

                if rendered_ok {
                    let discriminant =
                        discriminant.expect("This should be Some if rendered_ok is true");
                    let sub_fields = &fields[discriminant];
                    let sub_field_count = sub_fields.len();

                    for (index, field) in sub_fields.iter().enumerate() {
                        let is_last_sub_field = index == sub_field_count - 1;
                        let (raw_field, _) = render_setting_item_inner(field, false, true, cx);

                        content = content.child(
                            raw_field
                                .group("setting-sub-item")
                                .mx_8()
                                .p_4()
                                .border_t_1()
                                .when(is_last_sub_field, |this| this.border_b_1())
                                .when(is_last_sub_field && extra_bottom_padding, |this| {
                                    this.mb_8()
                                })
                                .border_dashed()
                                .border_color(cx.theme().colors().border_variant)
                                .bg(cx.theme().colors().element_background.opacity(0.2)),
                        );
                    }
                }

                return content.into_any_element();
            }
            SettingsPageItem::ActionLink(action_link) => v_flex()
                .group("setting-item")
                .px_8()
                .child(
                    h_flex()
                        .id(action_link.title.clone())
                        .w_full()
                        .min_w_0()
                        .justify_between()
                        .map(apply_padding)
                        .child(
                            v_flex()
                                .relative()
                                .w_full()
                                .max_w_1_2()
                                .child(Label::new(action_link.title.clone()))
                                .when_some(
                                    action_link.description.as_ref(),
                                    |this, description| {
                                        this.child(
                                            Label::new(description.clone())
                                                .size(LabelSize::Small)
                                                .color(Color::Muted),
                                        )
                                    },
                                ),
                        )
                        .child(
                            Button::new(
                                ("action-link".into(), action_link.title.clone()),
                                action_link.button_text.clone(),
                            )
                            .tab_index(0_isize)
                            .end_icon(
                                Icon::new(IconName::ArrowUpRight)
                                    .size(IconSize::Small)
                                    .color(Color::Muted),
                            )
                            .style(ButtonStyle::OutlinedGhost)
                            .size(ButtonSize::Medium)
                            .on_click({
                                let on_click = action_link.on_click.clone();
                                cx.listener(move |this, _, window, cx| {
                                    on_click(this, window, cx);
                                })
                            }),
                        ),
                )
                .when(bottom_border, |this| this.child(Divider::horizontal()))
                .into_any_element(),
        }
    }
}

pub(crate) struct SettingItem {
    title: &'static str,
    description: &'static str,
    field: Box<dyn AnySettingField>,
    metadata: Option<Box<SettingsFieldMetadata>>,
    files: FileMask,
}

pub(crate) struct DynamicItem {
    discriminant: SettingItem,
    pick_discriminant: fn(&SettingsContent) -> Option<usize>,
    fields: Vec<Vec<SettingItem>>,
}

impl PartialEq for DynamicItem {
    fn eq(&self, other: &Self) -> bool {
        self.discriminant == other.discriminant && self.fields == other.fields
    }
}

impl PartialEq for SettingItem {
    fn eq(&self, other: &Self) -> bool {
        self.title == other.title
            && self.description == other.description
            && (match (&self.metadata, &other.metadata) {
                (None, None) => true,
                (Some(m1), Some(m2)) => m1.placeholder == m2.placeholder,
                _ => false,
            })
    }
}

#[derive(Clone, PartialEq, Default)]
pub(crate) enum SubPageType {
    Language,
    SkillCreator,
    #[default]
    Other,
}

#[derive(Clone)]
pub(crate) struct SubPageLink {
    title: SharedString,
    r#type: SubPageType,
    description: Option<SharedString>,
    /// See [`SettingField.json_path`]
    json_path: Option<&'static str>,
    /// Whether or not the settings in this sub page are configurable in settings.json
    /// Removes the "Edit in settings.json" button from the page.
    in_json: bool,
    files: FileMask,
    render:
        fn(&SettingsWindow, &ScrollHandle, &mut Window, &mut Context<SettingsWindow>) -> AnyElement,
}

impl PartialEq for SubPageLink {
    fn eq(&self, other: &Self) -> bool {
        self.title == other.title
    }
}

#[derive(Clone)]
pub(crate) struct ActionLink {
    title: SharedString,
    description: Option<SharedString>,
    button_text: SharedString,
    on_click: Arc<dyn Fn(&mut SettingsWindow, &mut Window, &mut App) + Send + Sync>,
    files: FileMask,
}

impl PartialEq for ActionLink {
    fn eq(&self, other: &Self) -> bool {
        self.title == other.title
    }
}

pub(crate) fn all_language_names(cx: &App) -> Vec<SharedString> {
    let state = workspace::AppState::global(cx);
    state
        .languages
        .language_names()
        .into_iter()
        .filter(|name| name.as_ref() != "Mav Keybind Context")
        .map(Into::into)
        .collect()
}
