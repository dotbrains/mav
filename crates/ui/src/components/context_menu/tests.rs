use super::*;

mod tests {
    use gpui::TestAppContext;

    use super::*;

    #[gpui::test]
    fn can_navigate_back_over_headers(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let context_menu = cx.update(|window, cx| {
            ContextMenu::build(window, cx, |menu, _, _| {
                menu.header("First header")
                    .separator()
                    .entry("First entry", None, |_, _| {})
                    .separator()
                    .separator()
                    .entry("Last entry", None, |_, _| {})
                    .header("Last header")
            })
        });

        context_menu.update_in(cx, |context_menu, window, cx| {
            assert_eq!(
                None, context_menu.selected_index,
                "No selection is in the menu initially"
            );

            context_menu.select_first(&SelectFirst, window, cx);
            assert_eq!(
                Some(2),
                context_menu.selected_index,
                "Should select first selectable entry, skipping the header and the separator"
            );

            context_menu.select_next(&SelectNext, window, cx);
            assert_eq!(
                Some(5),
                context_menu.selected_index,
                "Should select next selectable entry, skipping 2 separators along the way"
            );

            context_menu.select_next(&SelectNext, window, cx);
            assert_eq!(
                Some(2),
                context_menu.selected_index,
                "Should wrap around to first selectable entry"
            );
        });

        context_menu.update_in(cx, |context_menu, window, cx| {
            assert_eq!(
                Some(2),
                context_menu.selected_index,
                "Should start from the first selectable entry"
            );

            context_menu.select_previous(&SelectPrevious, window, cx);
            assert_eq!(
                Some(5),
                context_menu.selected_index,
                "Should wrap around to previous selectable entry (last)"
            );

            context_menu.select_previous(&SelectPrevious, window, cx);
            assert_eq!(
                Some(2),
                context_menu.selected_index,
                "Should go back to previous selectable entry (first)"
            );
        });

        context_menu.update_in(cx, |context_menu, window, cx| {
            context_menu.select_first(&SelectFirst, window, cx);
            assert_eq!(
                Some(2),
                context_menu.selected_index,
                "Should start from the first selectable entry"
            );

            context_menu.select_previous(&SelectPrevious, window, cx);
            assert_eq!(
                Some(5),
                context_menu.selected_index,
                "Should wrap around to last selectable entry"
            );
            context_menu.select_next(&SelectNext, window, cx);
            assert_eq!(
                Some(2),
                context_menu.selected_index,
                "Should wrap around to first selectable entry"
            );
        });
    }
}
