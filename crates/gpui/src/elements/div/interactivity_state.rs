use super::*;

/// The interactivity struct. Powers all of the general-purpose
/// interactivity in the `Div` element.
#[derive(Default)]
pub struct Interactivity {
    /// The element ID of the element. In id is required to support a stateful subset of the interactivity such as on_click.
    pub element_id: Option<ElementId>,
    /// Whether the element was clicked. This will only be present after layout.
    pub active: Option<bool>,
    /// Whether the element was hovered. This will only be present after paint if an hitbox
    /// was created for the interactive element.
    pub hovered: Option<bool>,
    pub(crate) tooltip_id: Option<TooltipId>,
    pub(crate) content_size: Size<Pixels>,
    pub(crate) key_context: Option<KeyContext>,
    pub(crate) focusable: bool,
    pub(crate) tracked_focus_handle: Option<FocusHandle>,
    pub(crate) tracked_scroll_handle: Option<ScrollHandle>,
    pub(crate) scroll_anchor: Option<ScrollAnchor>,
    pub(crate) scroll_offset: Option<Rc<RefCell<Point<Pixels>>>>,
    pub(crate) group: Option<SharedString>,
    /// The base style of the element, before any modifications are applied
    /// by focus, active, etc.
    pub base_style: Box<StyleRefinement>,
    pub(crate) focus_style: Option<Box<StyleRefinement>>,
    pub(crate) in_focus_style: Option<Box<StyleRefinement>>,
    pub(crate) focus_visible_style: Option<Box<StyleRefinement>>,
    pub(crate) hover_style: Option<Box<StyleRefinement>>,
    pub(crate) group_hover_style: Option<GroupStyle>,
    pub(crate) active_style: Option<Box<StyleRefinement>>,
    pub(crate) group_active_style: Option<GroupStyle>,
    pub(crate) drag_over_styles: Vec<(
        TypeId,
        Box<dyn Fn(&dyn Any, &mut Window, &mut App) -> StyleRefinement>,
    )>,
    pub(crate) group_drag_over_styles: Vec<(TypeId, GroupStyle)>,
    pub(crate) mouse_down_listeners: Vec<MouseDownListener>,
    pub(crate) mouse_up_listeners: Vec<MouseUpListener>,
    pub(crate) mouse_pressure_listeners: Vec<MousePressureListener>,
    pub(crate) mouse_move_listeners: Vec<MouseMoveListener>,
    pub(crate) scroll_wheel_listeners: Vec<ScrollWheelListener>,
    pub(crate) pinch_listeners: Vec<PinchListener>,
    pub(crate) key_down_listeners: Vec<KeyDownListener>,
    pub(crate) key_up_listeners: Vec<KeyUpListener>,
    pub(crate) modifiers_changed_listeners: Vec<ModifiersChangedListener>,
    pub(crate) action_listeners: Vec<(TypeId, ActionListener)>,
    pub(crate) drop_listeners: Vec<(TypeId, DropListener)>,
    pub(crate) can_drop_predicate: Option<CanDropPredicate>,
    pub(crate) click_listeners: Vec<ClickListener>,
    pub(crate) aux_click_listeners: Vec<ClickListener>,
    pub(crate) drag_listener: Option<(Arc<dyn Any>, DragListener)>,
    pub(crate) hover_listener: Option<Box<dyn Fn(&bool, &mut Window, &mut App)>>,
    pub(crate) tooltip_builder: Option<TooltipBuilder>,
    pub(crate) tooltip_show_delay: Option<Duration>,
    pub(crate) window_control: Option<WindowControlArea>,
    pub(crate) hitbox_behavior: HitboxBehavior,
    pub(crate) tab_index: Option<isize>,
    pub(crate) tab_group: bool,
    pub(crate) tab_stop: bool,

    pub(crate) a11y_action_listeners:
        Vec<(accesskit::Action, crate::window::a11y::A11yActionListener)>,
    pub(crate) a11y_synthetic_children: Option<Box<dyn FnOnce(&mut crate::A11ySubtreeBuilder)>>,
    pub(crate) report_active_descendant_focus: bool,
    pub(crate) override_role: Option<accesskit::Role>,
    pub(crate) aria_label: Option<SharedString>,
    pub(crate) aria_selected: Option<bool>,
    pub(crate) aria_expanded: Option<bool>,
    pub(crate) aria_toggled: Option<accesskit::Toggled>,
    pub(crate) aria_numeric_value: Option<f64>,
    pub(crate) aria_min_numeric_value: Option<f64>,
    pub(crate) aria_max_numeric_value: Option<f64>,
    pub(crate) aria_numeric_value_step: Option<f64>,
    pub(crate) aria_value: Option<SharedString>,
    pub(crate) aria_placeholder: Option<SharedString>,
    pub(crate) aria_orientation: Option<accesskit::Orientation>,
    pub(crate) aria_level: Option<usize>,
    pub(crate) aria_position_in_set: Option<usize>,
    pub(crate) aria_size_of_set: Option<usize>,
    pub(crate) aria_row_index: Option<usize>,
    pub(crate) aria_column_index: Option<usize>,
    pub(crate) aria_row_count: Option<usize>,
    pub(crate) aria_column_count: Option<usize>,

    #[cfg(any(feature = "inspector", debug_assertions))]
    pub(crate) source_location: Option<&'static core::panic::Location<'static>>,

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) debug_selector: Option<String>,
}
