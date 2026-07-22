use super::*;

impl ContextMenu {
    pub fn context(mut self, focus: FocusHandle) -> Self {
        self.action_context = Some(focus);
        self
    }

    pub fn header(mut self, title: impl Into<SharedString>) -> Self {
        self.items.push(ContextMenuItem::Header(title.into()));
        self
    }

    pub fn header_with_link(
        mut self,
        title: impl Into<SharedString>,
        link_label: impl Into<SharedString>,
        link_url: impl Into<SharedString>,
    ) -> Self {
        self.items.push(ContextMenuItem::HeaderWithLink(
            title.into(),
            link_label.into(),
            link_url.into(),
        ));
        self
    }

    pub fn separator(mut self) -> Self {
        self.items.push(ContextMenuItem::Separator);
        self
    }

    pub fn extend<I: Into<ContextMenuItem>>(mut self, items: impl IntoIterator<Item = I>) -> Self {
        self.items.extend(items.into_iter().map(Into::into));
        self
    }

    pub fn item(mut self, item: impl Into<ContextMenuItem>) -> Self {
        self.items.push(item.into());
        self
    }

    pub fn push_item(&mut self, item: impl Into<ContextMenuItem>) {
        self.items.push(item.into());
    }

    pub fn entry(
        mut self,
        label: impl Into<SharedString>,
        action: Option<Box<dyn Action>>,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.items.push(ContextMenuItem::Entry(ContextMenuEntry {
            toggle: None,
            label: label.into(),
            handler: Rc::new(move |_, window, cx| handler(window, cx)),
            secondary_handler: None,
            icon: None,
            custom_icon_path: None,
            custom_icon_svg: None,
            icon_position: IconPosition::End,
            icon_size: IconSize::Small,
            icon_color: None,
            action,
            disabled: false,
            documentation_aside: None,
            end_slot_icon: None,
            end_slot_title: None,
            end_slot_handler: None,
            show_end_slot_on_hover: false,
        }));
        self
    }

    pub fn entry_with_end_slot(
        mut self,
        label: impl Into<SharedString>,
        action: Option<Box<dyn Action>>,
        handler: impl Fn(&mut Window, &mut App) + 'static,
        end_slot_icon: IconName,
        end_slot_title: SharedString,
        end_slot_handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.items.push(ContextMenuItem::Entry(ContextMenuEntry {
            toggle: None,
            label: label.into(),
            handler: Rc::new(move |_, window, cx| handler(window, cx)),
            secondary_handler: None,
            icon: None,
            custom_icon_path: None,
            custom_icon_svg: None,
            icon_position: IconPosition::End,
            icon_size: IconSize::Small,
            icon_color: None,
            action,
            disabled: false,
            documentation_aside: None,
            end_slot_icon: Some(end_slot_icon),
            end_slot_title: Some(end_slot_title),
            end_slot_handler: Some(Rc::new(move |_, window, cx| end_slot_handler(window, cx))),
            show_end_slot_on_hover: false,
        }));
        self
    }

    pub fn entry_with_end_slot_on_hover(
        mut self,
        label: impl Into<SharedString>,
        action: Option<Box<dyn Action>>,
        handler: impl Fn(&mut Window, &mut App) + 'static,
        end_slot_icon: IconName,
        end_slot_title: SharedString,
        end_slot_handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.items.push(ContextMenuItem::Entry(ContextMenuEntry {
            toggle: None,
            label: label.into(),
            handler: Rc::new(move |_, window, cx| handler(window, cx)),
            secondary_handler: None,
            icon: None,
            custom_icon_path: None,
            custom_icon_svg: None,
            icon_position: IconPosition::End,
            icon_size: IconSize::Small,
            icon_color: None,
            action,
            disabled: false,
            documentation_aside: None,
            end_slot_icon: Some(end_slot_icon),
            end_slot_title: Some(end_slot_title),
            end_slot_handler: Some(Rc::new(move |_, window, cx| end_slot_handler(window, cx))),
            show_end_slot_on_hover: true,
        }));
        self
    }

    pub fn toggleable_entry(
        mut self,
        label: impl Into<SharedString>,
        toggled: bool,
        position: IconPosition,
        action: Option<Box<dyn Action>>,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.items.push(ContextMenuItem::Entry(ContextMenuEntry {
            toggle: Some((position, toggled)),
            label: label.into(),
            handler: Rc::new(move |_, window, cx| handler(window, cx)),
            secondary_handler: None,
            icon: None,
            custom_icon_path: None,
            custom_icon_svg: None,
            icon_position: position,
            icon_size: IconSize::Small,
            icon_color: None,
            action,
            disabled: false,
            documentation_aside: None,
            end_slot_icon: None,
            end_slot_title: None,
            end_slot_handler: None,
            show_end_slot_on_hover: false,
        }));
        self
    }

    pub fn custom_row(
        mut self,
        entry_render: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.items.push(ContextMenuItem::CustomEntry {
            entry_render: Box::new(entry_render),
            handler: Rc::new(|_, _, _| {}),
            selectable: false,
            documentation_aside: None,
        });
        self
    }

    pub fn custom_entry(
        mut self,
        entry_render: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.items.push(ContextMenuItem::CustomEntry {
            entry_render: Box::new(entry_render),
            handler: Rc::new(move |_, window, cx| handler(window, cx)),
            selectable: true,
            documentation_aside: None,
        });
        self
    }

    pub fn custom_entry_with_docs(
        mut self,
        entry_render: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
        handler: impl Fn(&mut Window, &mut App) + 'static,
        documentation_aside: Option<DocumentationAside>,
    ) -> Self {
        self.items.push(ContextMenuItem::CustomEntry {
            entry_render: Box::new(entry_render),
            handler: Rc::new(move |_, window, cx| handler(window, cx)),
            selectable: true,
            documentation_aside,
        });
        self
    }

    pub fn selectable(mut self, selectable: bool) -> Self {
        if let Some(ContextMenuItem::CustomEntry {
            selectable: entry_selectable,
            ..
        }) = self.items.last_mut()
        {
            *entry_selectable = selectable;
        }
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.items.push(ContextMenuItem::Label(label.into()));
        self
    }

    pub fn action(self, label: impl Into<SharedString>, action: Box<dyn Action>) -> Self {
        self.action_checked(label, action, false)
    }

    pub fn action_checked(
        self,
        label: impl Into<SharedString>,
        action: Box<dyn Action>,
        checked: bool,
    ) -> Self {
        self.action_checked_with_disabled(label, action, checked, false)
    }

    pub fn action_checked_with_disabled(
        mut self,
        label: impl Into<SharedString>,
        action: Box<dyn Action>,
        checked: bool,
        disabled: bool,
    ) -> Self {
        self.items.push(ContextMenuItem::Entry(ContextMenuEntry {
            toggle: if checked {
                Some((IconPosition::Start, true))
            } else {
                None
            },
            label: label.into(),
            action: Some(action.boxed_clone()),
            handler: Rc::new(move |context, window, cx| {
                if let Some(context) = &context {
                    window.focus(context, cx);
                }
                window.dispatch_action(action.boxed_clone(), cx);
            }),
            secondary_handler: None,
            icon: None,
            custom_icon_path: None,
            custom_icon_svg: None,
            icon_position: IconPosition::End,
            icon_size: IconSize::Small,
            icon_color: None,
            disabled,
            documentation_aside: None,
            end_slot_icon: None,
            end_slot_title: None,
            end_slot_handler: None,
            show_end_slot_on_hover: false,
        }));
        self
    }

    pub fn action_disabled_when(
        mut self,
        disabled: bool,
        label: impl Into<SharedString>,
        action: Box<dyn Action>,
    ) -> Self {
        self.items.push(ContextMenuItem::Entry(ContextMenuEntry {
            toggle: None,
            label: label.into(),
            action: Some(action.boxed_clone()),
            handler: Rc::new(move |context, window, cx| {
                if let Some(context) = &context {
                    window.focus(context, cx);
                }
                window.dispatch_action(action.boxed_clone(), cx);
            }),
            secondary_handler: None,
            icon: None,
            custom_icon_path: None,
            custom_icon_svg: None,
            icon_size: IconSize::Small,
            icon_position: IconPosition::End,
            icon_color: None,
            disabled,
            documentation_aside: None,
            end_slot_icon: None,
            end_slot_title: None,
            end_slot_handler: None,
            show_end_slot_on_hover: false,
        }));
        self
    }

    pub fn link(self, label: impl Into<SharedString>, action: Box<dyn Action>) -> Self {
        self.link_with_handler(label, action, |_, _| {})
    }

    pub fn link_with_handler(
        mut self,
        label: impl Into<SharedString>,
        action: Box<dyn Action>,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.items.push(ContextMenuItem::Entry(ContextMenuEntry {
            toggle: None,
            label: label.into(),
            action: Some(action.boxed_clone()),
            handler: Rc::new(move |_, window, cx| {
                handler(window, cx);
                window.dispatch_action(action.boxed_clone(), cx);
            }),
            secondary_handler: None,
            icon: Some(IconName::ArrowUpRight),
            custom_icon_path: None,
            custom_icon_svg: None,
            icon_size: IconSize::XSmall,
            icon_position: IconPosition::End,
            icon_color: None,
            disabled: false,
            documentation_aside: None,
            end_slot_icon: None,
            end_slot_title: None,
            end_slot_handler: None,
            show_end_slot_on_hover: false,
        }));
        self
    }

    pub fn submenu(
        mut self,
        label: impl Into<SharedString>,
        builder: impl Fn(ContextMenu, &mut Window, &mut Context<ContextMenu>) -> ContextMenu + 'static,
    ) -> Self {
        self.items.push(ContextMenuItem::Submenu {
            label: label.into(),
            icon: None,
            icon_color: None,
            builder: Rc::new(builder),
        });
        self
    }

    pub fn submenu_with_icon(
        mut self,
        label: impl Into<SharedString>,
        icon: IconName,
        builder: impl Fn(ContextMenu, &mut Window, &mut Context<ContextMenu>) -> ContextMenu + 'static,
    ) -> Self {
        self.items.push(ContextMenuItem::Submenu {
            label: label.into(),
            icon: Some(icon),
            icon_color: None,
            builder: Rc::new(builder),
        });
        self
    }

    pub fn submenu_with_colored_icon(
        mut self,
        label: impl Into<SharedString>,
        icon: IconName,
        icon_color: Color,
        builder: impl Fn(ContextMenu, &mut Window, &mut Context<ContextMenu>) -> ContextMenu + 'static,
    ) -> Self {
        self.items.push(ContextMenuItem::Submenu {
            label: label.into(),
            icon: Some(icon),
            icon_color: Some(icon_color),
            builder: Rc::new(builder),
        });
        self
    }

    pub fn keep_open_on_confirm(mut self, keep_open: bool) -> Self {
        self.keep_open_on_confirm = keep_open;
        self
    }

    pub fn trigger_end_slot_handler(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.selected_index.and_then(|ix| self.items.get(ix)) else {
            return;
        };
        let ContextMenuItem::Entry(entry) = entry else {
            return;
        };
        let Some(handler) = entry.end_slot_handler.as_ref() else {
            return;
        };
        handler(None, window, cx);
    }

    pub fn fixed_width(mut self, width: DefiniteLength) -> Self {
        self.fixed_width = Some(width);
        self
    }

    pub fn end_slot_action(mut self, action: Box<dyn Action>) -> Self {
        self.end_slot_action = Some(action);
        self
    }

    pub fn key_context(mut self, context: impl Into<SharedString>) -> Self {
        self.key_context = context.into();
        self
    }

    pub fn selected_index(&self) -> Option<usize> {
        self.selected_index
    }
}
