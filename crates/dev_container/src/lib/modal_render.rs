use super::*;

impl DevContainerModal {
    pub(super) fn new(
        workspace: WeakEntity<Workspace>,
        _window: &mut Window,
        cx: &mut App,
    ) -> Self {
        DevContainerModal {
            workspace,
            picker: None,
            features_picker: None,
            state: DevContainerState::Initial,
            focus_handle: cx.focus_handle(),
            confirm_entry: NavigableEntry::focusable(cx),
            back_entry: NavigableEntry::focusable(cx),
        }
    }

    pub(super) fn render_initial(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let mut view = Navigable::new(
            div()
                .p_1()
                .child(
                    div().track_focus(&self.focus_handle).child(
                        ModalHeader::new().child(
                            Headline::new("Create Dev Container").size(HeadlineSize::XSmall),
                        ),
                    ),
                )
                .child(ListSeparator)
                .child(
                    div()
                        .track_focus(&self.confirm_entry.focus_handle)
                        .on_action(cx.listener(|this, _: &menu::Confirm, window, cx| {
                            this.accept_message(DevContainerMessage::SearchTemplates, window, cx);
                        }))
                        .child(
                            ListItem::new("li-search-containers")
                                .inset(true)
                                .spacing(ui::ListItemSpacing::Sparse)
                                .start_slot(
                                    Icon::new(IconName::MagnifyingGlass).color(Color::Muted),
                                )
                                .toggle_state(
                                    self.confirm_entry.focus_handle.contains_focused(window, cx),
                                )
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.accept_message(
                                        DevContainerMessage::SearchTemplates,
                                        window,
                                        cx,
                                    );
                                    cx.notify();
                                }))
                                .child(Label::new("Search for Dev Container Templates")),
                        ),
                )
                .into_any_element(),
        );
        view = view.entry(self.confirm_entry.clone());
        view.render(window, cx).into_any_element()
    }

    pub(super) fn render_error(
        &self,
        error_title: String,
        error: impl Display,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        v_flex()
            .p_1()
            .child(div().track_focus(&self.focus_handle).child(
                ModalHeader::new().child(Headline::new(error_title).size(HeadlineSize::XSmall)),
            ))
            .child(ListSeparator)
            .child(
                v_flex()
                    .child(Label::new(format!("{}", error)))
                    .whitespace_normal(),
            )
            .into_any_element()
    }

    pub(super) fn render_retrieved_templates(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(picker) = &self.picker {
            let picker_element = div()
                .track_focus(&self.focus_handle(cx))
                .child(picker.clone().into_any_element())
                .into_any_element();
            picker.focus_handle(cx).focus(window, cx);
            picker_element
        } else {
            div().into_any_element()
        }
    }

    pub(super) fn render_user_options_specifying(
        &self,
        template_entry: TemplateEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(next_option_entries) = &template_entry.current_option else {
            return div().into_any_element();
        };
        let mut view = Navigable::new(
            div()
                .child(
                    div()
                        .id("title")
                        .tooltip(Tooltip::text(next_option_entries.description.clone()))
                        .track_focus(&self.focus_handle)
                        .child(
                            ModalHeader::new()
                                .child(
                                    Headline::new("Template Option: ").size(HeadlineSize::XSmall),
                                )
                                .child(
                                    Headline::new(&next_option_entries.option_name)
                                        .size(HeadlineSize::XSmall),
                                ),
                        ),
                )
                .child(ListSeparator)
                .children(
                    next_option_entries
                        .navigable_options
                        .iter()
                        .map(|(option, entry)| {
                            div()
                                .id(format!("li-parent-{}", option))
                                .track_focus(&entry.focus_handle)
                                .on_action({
                                    let mut template = template_entry.clone();
                                    template.options_selected.insert(
                                        next_option_entries.option_name.clone(),
                                        option.clone(),
                                    );
                                    cx.listener(move |this, _: &menu::Confirm, window, cx| {
                                        this.accept_message(
                                            DevContainerMessage::TemplateOptionsSpecified(
                                                template.clone(),
                                            ),
                                            window,
                                            cx,
                                        );
                                    })
                                })
                                .child(
                                    ListItem::new(format!("li-option-{}", option))
                                        .inset(true)
                                        .spacing(ui::ListItemSpacing::Sparse)
                                        .toggle_state(
                                            entry.focus_handle.contains_focused(window, cx),
                                        )
                                        .on_click({
                                            let mut template = template_entry.clone();
                                            template.options_selected.insert(
                                                next_option_entries.option_name.clone(),
                                                option.clone(),
                                            );
                                            cx.listener(move |this, _, window, cx| {
                                                this.accept_message(
                                                    DevContainerMessage::TemplateOptionsSpecified(
                                                        template.clone(),
                                                    ),
                                                    window,
                                                    cx,
                                                );
                                                cx.notify();
                                            })
                                        })
                                        .child(Label::new(option)),
                                )
                        }),
                )
                .child(ListSeparator)
                .child(
                    div()
                        .track_focus(&self.back_entry.focus_handle)
                        .on_action(cx.listener(|this, _: &menu::Confirm, window, cx| {
                            this.accept_message(DevContainerMessage::GoBack, window, cx);
                        }))
                        .child(
                            ListItem::new("li-goback")
                                .inset(true)
                                .spacing(ui::ListItemSpacing::Sparse)
                                .start_slot(Icon::new(IconName::Return).color(Color::Muted))
                                .toggle_state(
                                    self.back_entry.focus_handle.contains_focused(window, cx),
                                )
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.accept_message(DevContainerMessage::GoBack, window, cx);
                                    cx.notify();
                                }))
                                .child(Label::new("Go Back")),
                        ),
                )
                .into_any_element(),
        );
        for (_, entry) in &next_option_entries.navigable_options {
            view = view.entry(entry.clone());
        }
        view = view.entry(self.back_entry.clone());
        view.render(window, cx).into_any_element()
    }

    pub(super) fn render_features_query_returned(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(picker) = &self.features_picker {
            let picker_element = div()
                .track_focus(&self.focus_handle(cx))
                .child(picker.clone().into_any_element())
                .into_any_element();
            picker.focus_handle(cx).focus(window, cx);
            picker_element
        } else {
            div().into_any_element()
        }
    }

    pub(super) fn render_confirming_write_dev_container(
        &self,
        template_entry: TemplateEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        Navigable::new(
            div()
                .child(
                    div().track_focus(&self.focus_handle).child(
                        ModalHeader::new()
                            .icon(Icon::new(IconName::Warning).color(Color::Warning))
                            .child(
                                Headline::new("Overwrite Existing Configuration?")
                                    .size(HeadlineSize::XSmall),
                            ),
                    ),
                )
                .child(
                    div()
                        .track_focus(&self.confirm_entry.focus_handle)
                        .on_action({
                            let template = template_entry.clone();
                            cx.listener(move |this, _: &menu::Confirm, window, cx| {
                                this.accept_message(
                                    DevContainerMessage::ConfirmWriteDevContainer(template.clone()),
                                    window,
                                    cx,
                                );
                            })
                        })
                        .child(
                            ListItem::new("li-search-containers")
                                .inset(true)
                                .spacing(ui::ListItemSpacing::Sparse)
                                .start_slot(Icon::new(IconName::Check).color(Color::Muted))
                                .toggle_state(
                                    self.confirm_entry.focus_handle.contains_focused(window, cx),
                                )
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.accept_message(
                                        DevContainerMessage::ConfirmWriteDevContainer(
                                            template_entry.clone(),
                                        ),
                                        window,
                                        cx,
                                    );
                                    cx.notify();
                                }))
                                .child(Label::new("Overwrite")),
                        ),
                )
                .child(
                    div()
                        .track_focus(&self.back_entry.focus_handle)
                        .on_action(cx.listener(|this, _: &menu::Confirm, window, cx| {
                            this.dismiss(&menu::Cancel, window, cx);
                        }))
                        .child(
                            ListItem::new("li-goback")
                                .inset(true)
                                .spacing(ui::ListItemSpacing::Sparse)
                                .start_slot(Icon::new(IconName::XCircle).color(Color::Muted))
                                .toggle_state(
                                    self.back_entry.focus_handle.contains_focused(window, cx),
                                )
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.dismiss(&menu::Cancel, window, cx);
                                    cx.notify();
                                }))
                                .child(Label::new("Cancel")),
                        ),
                )
                .into_any_element(),
        )
        .entry(self.confirm_entry.clone())
        .entry(self.back_entry.clone())
        .render(window, cx)
        .into_any_element()
    }

    pub(super) fn render_querying_templates(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        Navigable::new(
            div()
                .child(
                    div().track_focus(&self.focus_handle).child(
                        ModalHeader::new().child(
                            Headline::new("Create Dev Container").size(HeadlineSize::XSmall),
                        ),
                    ),
                )
                .child(ListSeparator)
                .child(
                    div().child(
                        ListItem::new("li-querying")
                            .inset(true)
                            .spacing(ui::ListItemSpacing::Sparse)
                            .start_slot(
                                Icon::new(IconName::ArrowCircle)
                                    .color(Color::Muted)
                                    .with_rotate_animation(2),
                            )
                            .child(Label::new("Querying template registry...")),
                    ),
                )
                .child(ListSeparator)
                .child(
                    div()
                        .track_focus(&self.back_entry.focus_handle)
                        .on_action(cx.listener(|this, _: &menu::Confirm, window, cx| {
                            this.accept_message(DevContainerMessage::GoBack, window, cx);
                        }))
                        .child(
                            ListItem::new("li-goback")
                                .inset(true)
                                .spacing(ui::ListItemSpacing::Sparse)
                                .start_slot(Icon::new(IconName::Pencil).color(Color::Muted))
                                .toggle_state(
                                    self.back_entry.focus_handle.contains_focused(window, cx),
                                )
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.accept_message(DevContainerMessage::GoBack, window, cx);
                                    cx.notify();
                                }))
                                .child(Label::new("Go Back")),
                        ),
                )
                .into_any_element(),
        )
        .entry(self.back_entry.clone())
        .render(window, cx)
        .into_any_element()
    }
    pub(super) fn render_querying_features(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        Navigable::new(
            div()
                .child(
                    div().track_focus(&self.focus_handle).child(
                        ModalHeader::new().child(
                            Headline::new("Create Dev Container").size(HeadlineSize::XSmall),
                        ),
                    ),
                )
                .child(ListSeparator)
                .child(
                    div().child(
                        ListItem::new("li-querying")
                            .inset(true)
                            .spacing(ui::ListItemSpacing::Sparse)
                            .start_slot(
                                Icon::new(IconName::ArrowCircle)
                                    .color(Color::Muted)
                                    .with_rotate_animation(2),
                            )
                            .child(Label::new("Querying features...")),
                    ),
                )
                .child(ListSeparator)
                .child(
                    div()
                        .track_focus(&self.back_entry.focus_handle)
                        .on_action(cx.listener(|this, _: &menu::Confirm, window, cx| {
                            this.accept_message(DevContainerMessage::GoBack, window, cx);
                        }))
                        .child(
                            ListItem::new("li-goback")
                                .inset(true)
                                .spacing(ui::ListItemSpacing::Sparse)
                                .start_slot(Icon::new(IconName::Pencil).color(Color::Muted))
                                .toggle_state(
                                    self.back_entry.focus_handle.contains_focused(window, cx),
                                )
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.accept_message(DevContainerMessage::GoBack, window, cx);
                                    cx.notify();
                                }))
                                .child(Label::new("Go Back")),
                        ),
                )
                .into_any_element(),
        )
        .entry(self.back_entry.clone())
        .render(window, cx)
        .into_any_element()
    }
}
