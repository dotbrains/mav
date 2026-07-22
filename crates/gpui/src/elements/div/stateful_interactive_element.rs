use super::*;

/// A trait for elements that want to use the standard GPUI interactivity features
/// that require state.
pub trait StatefulInteractiveElement: InteractiveElement {
    /// Set the accessible role for this element.
    ///
    /// See the [accessibility guide](crate::_accessibility) for an overview.
    fn role(mut self, role: accesskit::Role) -> Self {
        debug_assert!(
            role != accesskit::Role::GenericContainer,
            "GenericContainer is filtered out of the a11y tree and has no effect"
        );
        self.interactivity().override_role = Some(role);
        self
    }

    /// Set the accessible label for this element.
    fn aria_label(mut self, label: impl Into<SharedString>) -> Self {
        self.interactivity().aria_label = Some(label.into());
        self
    }

    /// Report this element as the focused node in the accessibility tree,
    /// overriding the element that holds real keyboard focus — but only while
    /// one of its ancestors actually holds focus.
    ///
    /// This implements the `aria-activedescendant` pattern for composite
    /// widgets that keep keyboard focus on a container (e.g. a menu or
    /// listbox) while a child is "selected": set this on the selected child so
    /// assistive technology announces and highlights it as focused.
    ///
    /// The element must also have a [`role`][Self::role] (and an id) so it
    /// produces an accessibility node. Unlike the web's container-side
    /// `aria-activedescendant`, this is set on the descendant; GPUI honors it
    /// only when a focused ancestor is present in the tree, so it is safe to
    /// set unconditionally on the selected child — if the container isn't
    /// focused, the claim is ignored.
    fn aria_active_descendant(mut self) -> Self {
        self.interactivity().report_active_descendant_focus = true;
        self
    }

    /// Contribute synthetic accessibility nodes — nodes that don't correspond
    /// to any element — as children of this element's a11y node. For example,
    /// text runs describing an editor's text content.
    ///
    /// The closure is called after this element is prepainted, and only if it
    /// contributed a node to the accessibility tree (i.e. it has an id and a
    /// [`role`][StatefulInteractiveElement::role]).
    ///
    /// See [`Element::a11y_synthetic_children`] for details.
    fn a11y_synthetic_children(
        mut self,
        f: impl FnOnce(&mut crate::A11ySubtreeBuilder) + 'static,
    ) -> Self {
        self.interactivity().a11y_synthetic_children = Some(Box::new(f));
        self
    }

    /// Set the selected state for this element.
    fn aria_selected(mut self, selected: bool) -> Self {
        self.interactivity().aria_selected = Some(selected);
        self
    }

    /// Set the expanded state for this element.
    fn aria_expanded(mut self, expanded: bool) -> Self {
        self.interactivity().aria_expanded = Some(expanded);
        self
    }

    /// Set the toggled state for this element.
    fn aria_toggled(mut self, toggled: accesskit::Toggled) -> Self {
        self.interactivity().aria_toggled = Some(toggled);
        self
    }

    /// Set the numeric value for this element.
    fn aria_numeric_value(mut self, value: f64) -> Self {
        self.interactivity().aria_numeric_value = Some(value);
        self
    }

    /// Set the step by which assistive technology should expect the numeric
    /// value of this element to change (e.g. when incrementing a spin button).
    fn aria_numeric_value_step(mut self, step: f64) -> Self {
        self.interactivity().aria_numeric_value_step = Some(step);
        self
    }

    /// Set the string value of this element, e.g. the text content of a simple
    /// text input.
    fn aria_value(mut self, value: impl Into<SharedString>) -> Self {
        self.interactivity().aria_value = Some(value.into());
        self
    }

    /// Set the placeholder text reported to assistive technology for this
    /// element, shown when a text input is empty.
    fn aria_placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.interactivity().aria_placeholder = Some(placeholder.into());
        self
    }

    /// Set the minimum numeric value for this element.
    fn aria_min_numeric_value(mut self, value: f64) -> Self {
        self.interactivity().aria_min_numeric_value = Some(value);
        self
    }

    /// Set the maximum numeric value for this element.
    fn aria_max_numeric_value(mut self, value: f64) -> Self {
        self.interactivity().aria_max_numeric_value = Some(value);
        self
    }

    /// Set the orientation of this element.
    fn aria_orientation(mut self, orientation: accesskit::Orientation) -> Self {
        self.interactivity().aria_orientation = Some(orientation);
        self
    }

    /// Set the heading level of this element.
    fn aria_level(mut self, level: usize) -> Self {
        self.interactivity().aria_level = Some(level);
        self
    }

    /// Set the position in set of this element.
    fn aria_position_in_set(mut self, position: usize) -> Self {
        self.interactivity().aria_position_in_set = Some(position);
        self
    }

    /// Set the size of set for this element.
    fn aria_size_of_set(mut self, size: usize) -> Self {
        self.interactivity().aria_size_of_set = Some(size);
        self
    }

    /// Set the row index for this element.
    fn aria_row_index(mut self, index: usize) -> Self {
        self.interactivity().aria_row_index = Some(index);
        self
    }

    /// Set the column index for this element.
    fn aria_column_index(mut self, index: usize) -> Self {
        self.interactivity().aria_column_index = Some(index);
        self
    }

    /// Set the row count for this element.
    fn aria_row_count(mut self, count: usize) -> Self {
        self.interactivity().aria_row_count = Some(count);
        self
    }

    /// Set the column count for this element.
    fn aria_column_count(mut self, count: usize) -> Self {
        self.interactivity().aria_column_count = Some(count);
        self
    }

    /// Register a handler for an accessibility action on this element.
    /// The handler is called when a screen reader requests the given action.
    ///
    /// See the [accessibility guide](crate::_accessibility) for an overview.
    fn on_a11y_action(
        mut self,
        action: accesskit::Action,
        listener: impl FnMut(Option<&accesskit::ActionData>, &mut crate::Window, &mut crate::App)
        + 'static,
    ) -> Self {
        self.interactivity()
            .a11y_action_listeners
            .push((action, Box::new(listener)));
        self
    }

    /// Set this element to focusable.
    fn focusable(mut self) -> Self {
        self.interactivity().focusable = true;
        self
    }

    /// Set the overflow x and y to scroll.
    fn overflow_scroll(mut self) -> Self {
        self.interactivity().base_style.overflow.x = Some(Overflow::Scroll);
        self.interactivity().base_style.overflow.y = Some(Overflow::Scroll);
        self
    }

    /// Set the overflow x to scroll.
    fn overflow_x_scroll(mut self) -> Self {
        self.interactivity().base_style.overflow.x = Some(Overflow::Scroll);
        self
    }

    /// Set the overflow y to scroll.
    fn overflow_y_scroll(mut self) -> Self {
        self.interactivity().base_style.overflow.y = Some(Overflow::Scroll);
        self
    }

    /// Track the scroll state of this element with the given handle.
    fn track_scroll(mut self, scroll_handle: &ScrollHandle) -> Self {
        self.interactivity().tracked_scroll_handle = Some(scroll_handle.clone());
        self
    }

    /// Track the scroll state of this element with the given handle.
    fn anchor_scroll(mut self, scroll_anchor: Option<ScrollAnchor>) -> Self {
        self.interactivity().scroll_anchor = scroll_anchor;
        self
    }

    /// Set the given styles to be applied when this element is active.
    fn active(mut self, f: impl FnOnce(StyleRefinement) -> StyleRefinement) -> Self
    where
        Self: Sized,
    {
        self.interactivity().active_style = Some(Box::new(f(StyleRefinement::default())));
        self
    }

    /// Set the given styles to be applied when this element's group is active.
    fn group_active(
        mut self,
        group_name: impl Into<SharedString>,
        f: impl FnOnce(StyleRefinement) -> StyleRefinement,
    ) -> Self
    where
        Self: Sized,
    {
        self.interactivity().group_active_style = Some(GroupStyle {
            group: group_name.into(),
            style: Box::new(f(StyleRefinement::default())),
        });
        self
    }

    /// Bind the given callback to click events of this element.
    /// The fluent API equivalent to [`Interactivity::on_click`].
    ///
    /// See [`Context::listener`](crate::Context::listener) to get access to a view's state from this callback.
    fn on_click(mut self, listener: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self
    where
        Self: Sized,
    {
        self.interactivity().on_click(listener);
        self
    }

    /// Bind the given callback to non-primary click events of this element.
    /// The fluent API equivalent to [`Interactivity::on_aux_click`].
    ///
    /// See [`Context::listener`](crate::Context::listener) to get access to a view's state from this callback.
    fn on_aux_click(
        mut self,
        listener: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self
    where
        Self: Sized,
    {
        self.interactivity().on_aux_click(listener);
        self
    }

    /// On drag initiation, this callback will be used to create a new view to render the dragged value for a
    /// drag and drop operation. This API should also be used as the equivalent of 'on drag start' with
    /// the [`InteractiveElement::on_drag_move`] API.
    /// The callback also has access to the offset of triggering click from the origin of parent element.
    /// The fluent API equivalent to [`Interactivity::on_drag`].
    ///
    /// See [`Context::listener`](crate::Context::listener) to get access to a view's state from this callback.
    fn on_drag<T, W>(
        mut self,
        value: T,
        constructor: impl Fn(&T, Point<Pixels>, &mut Window, &mut App) -> Entity<W> + 'static,
    ) -> Self
    where
        Self: Sized,
        T: 'static,
        W: 'static + Render,
    {
        self.interactivity().on_drag(value, constructor);
        self
    }

    /// Bind the given callback on the hover start and end events of this element. Note that the boolean
    /// passed to the callback is true when the hover starts and false when it ends.
    /// The fluent API equivalent to [`Interactivity::on_hover`].
    ///
    /// See [`Context::listener`](crate::Context::listener) to get access to a view's state from this callback.
    fn on_hover(mut self, listener: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self
    where
        Self: Sized,
    {
        self.interactivity().on_hover(listener);
        self
    }

    /// Use the given callback to construct a new tooltip view when the mouse hovers over this element.
    /// The fluent API equivalent to [`Interactivity::tooltip`].
    fn tooltip(mut self, build_tooltip: impl Fn(&mut Window, &mut App) -> AnyView + 'static) -> Self
    where
        Self: Sized,
    {
        self.interactivity().tooltip(build_tooltip);
        self
    }

    /// Use the given callback to construct a new tooltip view when the mouse hovers over this element.
    /// The tooltip itself is also hoverable and won't disappear when the user moves the mouse into
    /// the tooltip. The fluent API equivalent to [`Interactivity::hoverable_tooltip`].
    fn hoverable_tooltip(
        mut self,
        build_tooltip: impl Fn(&mut Window, &mut App) -> AnyView + 'static,
    ) -> Self
    where
        Self: Sized,
    {
        self.interactivity().hoverable_tooltip(build_tooltip);
        self
    }

    /// Set the delay before this element's tooltip is shown.
    /// The fluent API equivalent to [`Interactivity::tooltip_show_delay`].
    fn tooltip_show_delay(mut self, delay: Duration) -> Self
    where
        Self: Sized,
    {
        self.interactivity().tooltip_show_delay(delay);
        self
    }
}
