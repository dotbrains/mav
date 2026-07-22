pub(crate) use crate::{
    ButtonCommon, ButtonStyle, IconButtonShape, KeyBinding, List, ListItem, ListSeparator,
    ListSubHeader, Tooltip, prelude::*, utils::WithRemSize,
};
pub(crate) use gpui::{
    Action, Anchor, AnyElement, App, Bounds, DismissEvent, Entity, EventEmitter, FocusHandle,
    Focusable, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, Role,
    Size, Subscription, TaskExt, anchored, canvas, prelude::*, px,
};
pub(crate) use menu::{
    SelectChild, SelectFirst, SelectLast, SelectNext, SelectParent, SelectPrevious,
};
pub(crate) use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    time::{Duration, Instant},
};

mod actions;
mod builder;
mod item_selectable;
mod lifecycle;
mod render;
mod render_entry;
mod render_item;
mod render_submenu;
#[cfg(test)]
mod tests;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum SubmenuOpenTrigger {
    Pointer,
    Keyboard,
}

struct OpenSubmenu {
    item_index: usize,
    entity: Entity<ContextMenu>,
    trigger_bounds: Option<Bounds<Pixels>>,
    offset: Option<Pixels>,
    flip_left: bool,
    _dismiss_subscription: Subscription,
}

enum SubmenuState {
    Closed,
    Open(OpenSubmenu),
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum HoverTarget {
    #[default]
    None,
    MainMenu,
    Submenu,
}

pub enum ContextMenuItem {
    Separator,
    Header(SharedString),
    /// title, link_label, link_url
    HeaderWithLink(SharedString, SharedString, SharedString), // This could be folded into header
    Label(SharedString),
    Entry(ContextMenuEntry),
    CustomEntry {
        entry_render: Box<dyn Fn(&mut Window, &mut App) -> AnyElement>,
        handler: Rc<dyn Fn(Option<&FocusHandle>, &mut Window, &mut App)>,
        selectable: bool,
        documentation_aside: Option<DocumentationAside>,
    },
    Submenu {
        label: SharedString,
        icon: Option<IconName>,
        icon_color: Option<Color>,
        builder: Rc<dyn Fn(ContextMenu, &mut Window, &mut Context<ContextMenu>) -> ContextMenu>,
    },
}

impl ContextMenuItem {
    pub fn custom_entry(
        entry_render: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
        handler: impl Fn(&mut Window, &mut App) + 'static,
        documentation_aside: Option<DocumentationAside>,
    ) -> Self {
        Self::CustomEntry {
            entry_render: Box::new(entry_render),
            handler: Rc::new(move |_, window, cx| handler(window, cx)),
            selectable: true,
            documentation_aside,
        }
    }
}

pub struct ContextMenuEntry {
    toggle: Option<(IconPosition, bool)>,
    label: SharedString,
    icon: Option<IconName>,
    custom_icon_path: Option<SharedString>,
    custom_icon_svg: Option<SharedString>,
    icon_position: IconPosition,
    icon_size: IconSize,
    icon_color: Option<Color>,
    handler: Rc<dyn Fn(Option<&FocusHandle>, &mut Window, &mut App)>,
    secondary_handler: Option<Rc<dyn Fn(Option<&FocusHandle>, &mut Window, &mut App)>>,
    action: Option<Box<dyn Action>>,
    disabled: bool,
    documentation_aside: Option<DocumentationAside>,
    end_slot_icon: Option<IconName>,
    end_slot_title: Option<SharedString>,
    end_slot_handler: Option<Rc<dyn Fn(Option<&FocusHandle>, &mut Window, &mut App)>>,
    show_end_slot_on_hover: bool,
}

impl ContextMenuEntry {
    pub fn new(label: impl Into<SharedString>) -> Self {
        ContextMenuEntry {
            toggle: None,
            label: label.into(),
            icon: None,
            custom_icon_path: None,
            custom_icon_svg: None,
            icon_position: IconPosition::Start,
            icon_size: IconSize::Small,
            icon_color: None,
            handler: Rc::new(|_, _, _| {}),
            secondary_handler: None,
            action: None,
            disabled: false,
            documentation_aside: None,
            end_slot_icon: None,
            end_slot_title: None,
            end_slot_handler: None,
            show_end_slot_on_hover: false,
        }
    }

    pub fn toggleable(mut self, toggle_position: IconPosition, toggled: bool) -> Self {
        self.toggle = Some((toggle_position, toggled));
        self
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn custom_icon_path(mut self, path: impl Into<SharedString>) -> Self {
        self.custom_icon_path = Some(path.into());
        self.custom_icon_svg = None; // Clear other icon sources if custom path is set
        self.icon = None;
        self
    }

    pub fn custom_icon_svg(mut self, svg: impl Into<SharedString>) -> Self {
        self.custom_icon_svg = Some(svg.into());
        self.custom_icon_path = None; // Clear other icon sources if custom path is set
        self.icon = None;
        self
    }

    pub fn icon_position(mut self, position: IconPosition) -> Self {
        self.icon_position = position;
        self
    }

    pub fn icon_size(mut self, icon_size: IconSize) -> Self {
        self.icon_size = icon_size;
        self
    }

    pub fn icon_color(mut self, icon_color: Color) -> Self {
        self.icon_color = Some(icon_color);
        self
    }

    pub fn toggle(mut self, toggle_position: IconPosition, toggled: bool) -> Self {
        self.toggle = Some((toggle_position, toggled));
        self
    }

    pub fn action(mut self, action: Box<dyn Action>) -> Self {
        self.action = Some(action);
        self
    }

    pub fn handler(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.handler = Rc::new(move |_, window, cx| handler(window, cx));
        self
    }

    pub fn secondary_handler(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.secondary_handler = Some(Rc::new(move |_, window, cx| handler(window, cx)));
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn documentation_aside(
        mut self,
        side: DocumentationSide,
        render: impl Fn(&mut App) -> AnyElement + 'static,
    ) -> Self {
        self.documentation_aside = Some(DocumentationAside {
            side,
            render: Rc::new(render),
        });

        self
    }
}

impl FluentBuilder for ContextMenuEntry {}

impl From<ContextMenuEntry> for ContextMenuItem {
    fn from(entry: ContextMenuEntry) -> Self {
        ContextMenuItem::Entry(entry)
    }
}

pub struct ContextMenu {
    builder: Option<Rc<dyn Fn(Self, &mut Window, &mut Context<Self>) -> Self>>,
    items: Vec<ContextMenuItem>,
    focus_handle: FocusHandle,
    action_context: Option<FocusHandle>,
    selected_index: Option<usize>,
    delayed: bool,
    clicked: bool,
    end_slot_action: Option<Box<dyn Action>>,
    key_context: SharedString,
    _on_blur_subscription: Subscription,
    keep_open_on_confirm: bool,
    fixed_width: Option<DefiniteLength>,
    main_menu: Option<Entity<ContextMenu>>,
    main_menu_observed_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    // Docs aide-related fields
    documentation_aside: Option<(usize, DocumentationAside)>,
    aside_trigger_bounds: Rc<RefCell<HashMap<usize, Bounds<Pixels>>>>,
    // Submenu-related fields
    submenu_state: SubmenuState,
    hover_target: HoverTarget,
    submenu_safety_threshold_x: Option<Pixels>,
    submenu_trigger_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    submenu_trigger_mouse_down: bool,
    ignore_blur_until: Option<Instant>,
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum DocumentationSide {
    Left,
    Right,
}

#[derive(Clone)]
pub struct DocumentationAside {
    pub side: DocumentationSide,
    pub render: Rc<dyn Fn(&mut App) -> AnyElement>,
}

impl DocumentationAside {
    pub fn new(side: DocumentationSide, render: Rc<dyn Fn(&mut App) -> AnyElement>) -> Self {
        Self { side, render }
    }
}

impl Focusable for ContextMenu {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<DismissEvent> for ContextMenu {}

impl FluentBuilder for ContextMenu {}
