use super::*;

impl StatefulModal for DevContainerModal {
    type State = DevContainerState;
    type Message = DevContainerMessage;

    fn state(&self) -> Self::State {
        self.state.clone()
    }

    fn render_for_state(
        &self,
        state: Self::State,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match state {
            DevContainerState::Initial => self.render_initial(window, cx),
            DevContainerState::QueryingTemplates => self.render_querying_templates(window, cx),
            DevContainerState::TemplateQueryReturned(Ok(_)) => {
                self.render_retrieved_templates(window, cx)
            }
            DevContainerState::UserOptionsSpecifying(template_entry) => {
                self.render_user_options_specifying(template_entry, window, cx)
            }
            DevContainerState::QueryingFeatures(_) => self.render_querying_features(window, cx),
            DevContainerState::FeaturesQueryReturned(_) => {
                self.render_features_query_returned(window, cx)
            }
            DevContainerState::ConfirmingWriteDevContainer(template_entry) => {
                self.render_confirming_write_dev_container(template_entry, window, cx)
            }
            DevContainerState::TemplateWriteFailed(dev_container_error) => self.render_error(
                "Error Creating Dev Container Definition".to_string(),
                dev_container_error,
                window,
                cx,
            ),
            DevContainerState::TemplateQueryReturned(Err(e)) => {
                self.render_error("Error Retrieving Templates".to_string(), e, window, cx)
            }
        }
    }

    fn accept_message(
        &mut self,
        message: Self::Message,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let new_state = match message {
            DevContainerMessage::SearchTemplates => {
                cx.spawn_in(window, async move |this, cx| {
                    let Ok(client) = cx.update(|_, cx| cx.http_client()) else {
                        return;
                    };
                    match get_ghcr_templates(client).await {
                        Ok(templates) => {
                            let message =
                                DevContainerMessage::TemplatesRetrieved(templates.templates);
                            this.update_in(cx, |this, window, cx| {
                                this.accept_message(message, window, cx);
                            })
                            .ok();
                        }
                        Err(e) => {
                            let message = DevContainerMessage::ErrorRetrievingTemplates(e);
                            this.update_in(cx, |this, window, cx| {
                                this.accept_message(message, window, cx);
                            })
                            .ok();
                        }
                    }
                })
                .detach();
                Some(DevContainerState::QueryingTemplates)
            }
            DevContainerMessage::ErrorRetrievingTemplates(message) => {
                Some(DevContainerState::TemplateQueryReturned(Err(message)))
            }
            DevContainerMessage::GoBack => match &self.state {
                DevContainerState::Initial => Some(DevContainerState::Initial),
                DevContainerState::QueryingTemplates => Some(DevContainerState::Initial),
                DevContainerState::UserOptionsSpecifying(template_entry) => {
                    if template_entry.current_option_index <= 1 {
                        self.accept_message(DevContainerMessage::SearchTemplates, window, cx);
                    } else {
                        let mut template_entry = template_entry.clone();
                        template_entry.current_option_index =
                            template_entry.current_option_index.saturating_sub(2);
                        self.accept_message(
                            DevContainerMessage::TemplateOptionsSpecified(template_entry),
                            window,
                            cx,
                        );
                    }
                    None
                }
                _ => Some(DevContainerState::Initial),
            },
            DevContainerMessage::TemplatesRetrieved(items) => {
                let items = items
                    .into_iter()
                    .map(|item| TemplateEntry {
                        template: item,
                        options_selected: HashMap::new(),
                        current_option_index: 0,
                        current_option: None,
                        features_selected: HashSet::new(),
                    })
                    .collect::<Vec<TemplateEntry>>();
                if self.state == DevContainerState::QueryingTemplates {
                    let delegate = TemplatePickerDelegate::new(
                        "Select a template".to_string(),
                        cx.weak_entity(),
                        items.clone(),
                        Box::new(|entry, this, window, cx| {
                            this.accept_message(
                                DevContainerMessage::TemplateSelected(entry),
                                window,
                                cx,
                            );
                        }),
                    );

                    let picker = cx.new(|cx| Picker::uniform_list(delegate, window, cx).embedded());
                    self.picker = Some(picker);
                    Some(DevContainerState::TemplateQueryReturned(Ok(items)))
                } else {
                    None
                }
            }
            DevContainerMessage::TemplateSelected(mut template_entry) => {
                let Some(options) = template_entry.template.clone().options else {
                    return self.accept_message(
                        DevContainerMessage::TemplateOptionsCompleted(template_entry),
                        window,
                        cx,
                    );
                };

                let options = options
                    .iter()
                    .collect::<Vec<(&String, &TemplateOptions)>>()
                    .clone();

                let Some((first_option_name, first_option)) =
                    options.get(template_entry.current_option_index)
                else {
                    return self.accept_message(
                        DevContainerMessage::TemplateOptionsCompleted(template_entry),
                        window,
                        cx,
                    );
                };

                let next_option_entries = first_option
                    .possible_values()
                    .into_iter()
                    .map(|option| (option, NavigableEntry::focusable(cx)))
                    .collect();

                template_entry.current_option_index += 1;
                template_entry.current_option = Some(TemplateOptionSelection {
                    option_name: (*first_option_name).clone(),
                    description: first_option
                        .description
                        .clone()
                        .unwrap_or_else(|| "".to_string()),
                    navigable_options: next_option_entries,
                });

                Some(DevContainerState::UserOptionsSpecifying(template_entry))
            }
            DevContainerMessage::TemplateOptionsSpecified(mut template_entry) => {
                let Some(options) = template_entry.template.clone().options else {
                    return self.accept_message(
                        DevContainerMessage::TemplateOptionsCompleted(template_entry),
                        window,
                        cx,
                    );
                };

                let options = options
                    .iter()
                    .collect::<Vec<(&String, &TemplateOptions)>>()
                    .clone();

                let Some((next_option_name, next_option)) =
                    options.get(template_entry.current_option_index)
                else {
                    return self.accept_message(
                        DevContainerMessage::TemplateOptionsCompleted(template_entry),
                        window,
                        cx,
                    );
                };

                let next_option_entries = next_option
                    .possible_values()
                    .into_iter()
                    .map(|option| (option, NavigableEntry::focusable(cx)))
                    .collect();

                template_entry.current_option_index += 1;
                template_entry.current_option = Some(TemplateOptionSelection {
                    option_name: (*next_option_name).clone(),
                    description: next_option
                        .description
                        .clone()
                        .unwrap_or_else(|| "".to_string()),
                    navigable_options: next_option_entries,
                });

                Some(DevContainerState::UserOptionsSpecifying(template_entry))
            }
            DevContainerMessage::TemplateOptionsCompleted(template_entry) => {
                cx.spawn_in(window, async move |this, cx| {
                    let Ok(client) = cx.update(|_, cx| cx.http_client()) else {
                        return;
                    };
                    let Some(features) = get_ghcr_features(client).await.log_err() else {
                        return;
                    };
                    let message = DevContainerMessage::FeaturesRetrieved(features.features);
                    this.update_in(cx, |this, window, cx| {
                        this.accept_message(message, window, cx);
                    })
                    .ok();
                })
                .detach();
                Some(DevContainerState::QueryingFeatures(template_entry))
            }
            DevContainerMessage::FeaturesRetrieved(features) => {
                if let DevContainerState::QueryingFeatures(template_entry) = self.state.clone() {
                    let features = features
                        .iter()
                        .map(|feature| FeatureEntry {
                            feature: feature.clone(),
                            toggle_state: ToggleState::Unselected,
                        })
                        .collect::<Vec<FeatureEntry>>();
                    let delegate = FeaturePickerDelegate::new(
                        "Select features to add".to_string(),
                        cx.weak_entity(),
                        features,
                        template_entry.clone(),
                        Box::new(|entry, this, window, cx| {
                            this.accept_message(
                                DevContainerMessage::FeaturesSelected(entry),
                                window,
                                cx,
                            );
                        }),
                    );

                    let picker = cx.new(|cx| Picker::uniform_list(delegate, window, cx).embedded());
                    self.features_picker = Some(picker);
                    Some(DevContainerState::FeaturesQueryReturned(template_entry))
                } else {
                    None
                }
            }
            DevContainerMessage::FeaturesSelected(template_entry) => {
                if let Some(workspace) = self.workspace.upgrade() {
                    dispatch_apply_templates(template_entry, workspace, window, true, cx);
                }

                None
            }
            DevContainerMessage::NeedConfirmWriteDevContainer(template_entry) => Some(
                DevContainerState::ConfirmingWriteDevContainer(template_entry),
            ),
            DevContainerMessage::ConfirmWriteDevContainer(template_entry) => {
                if let Some(workspace) = self.workspace.upgrade() {
                    dispatch_apply_templates(template_entry, workspace, window, false, cx);
                }
                None
            }
            DevContainerMessage::FailedToWriteTemplate(error) => {
                Some(DevContainerState::TemplateWriteFailed(error))
            }
        };
        if let Some(state) = new_state {
            self.state = state;
            self.focus_handle.focus(window, cx);
        }
        cx.notify();
    }
}
