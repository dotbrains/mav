use super::*;

impl SerializableItem for KeymapEditor {
    fn serialized_item_kind() -> &'static str {
        "KeymapEditor"
    }

    fn cleanup(
        workspace_id: workspace::WorkspaceId,
        alive_items: Vec<workspace::ItemId>,
        _window: &mut Window,
        cx: &mut App,
    ) -> gpui::Task<gpui::Result<()>> {
        let db = KeybindingEditorDb::global(cx);
        workspace::delete_unloaded_items(alive_items, workspace_id, "keybinding_editors", &db, cx)
    }

    fn deserialize(
        _project: Entity<project::Project>,
        workspace: WeakEntity<Workspace>,
        workspace_id: workspace::WorkspaceId,
        item_id: workspace::ItemId,
        window: &mut Window,
        cx: &mut App,
    ) -> gpui::Task<gpui::Result<Entity<Self>>> {
        let db = KeybindingEditorDb::global(cx);
        window.spawn(cx, async move |cx| {
            if db.get_keybinding_editor(item_id, workspace_id)?.is_some() {
                cx.update(|window, cx| cx.new(|cx| KeymapEditor::new(workspace, window, cx)))
            } else {
                Err(anyhow!("No keybinding editor to deserialize"))
            }
        })
    }

    fn serialize(
        &mut self,
        workspace: &mut Workspace,
        item_id: workspace::ItemId,
        _closing: bool,
        _window: &mut Window,
        cx: &mut ui::Context<Self>,
    ) -> Option<gpui::Task<gpui::Result<()>>> {
        let workspace_id = workspace.database_id()?;
        let db = KeybindingEditorDb::global(cx);
        Some(cx.background_spawn(
            async move { db.save_keybinding_editor(item_id, workspace_id).await },
        ))
    }

    fn should_serialize(&self, _event: &Self::Event) -> bool {
        false
    }
}

mod persistence {
    use db::{query, sqlez::domain::Domain, sqlez_macros::sql};
    use workspace::WorkspaceDb;

    pub struct KeybindingEditorDb(db::sqlez::thread_safe_connection::ThreadSafeConnection);

    impl Domain for KeybindingEditorDb {
        const NAME: &str = stringify!(KeybindingEditorDb);

        const MIGRATIONS: &[&str] = &[sql!(
                CREATE TABLE keybinding_editors (
                    workspace_id INTEGER,
                    item_id INTEGER UNIQUE,

                    PRIMARY KEY(workspace_id, item_id),
                    FOREIGN KEY(workspace_id) REFERENCES workspaces(workspace_id)
                    ON DELETE CASCADE
                ) STRICT;
        )];
    }

    db::static_connection!(KeybindingEditorDb, [WorkspaceDb]);

    impl KeybindingEditorDb {
        query! {
            pub async fn save_keybinding_editor(
                item_id: workspace::ItemId,
                workspace_id: workspace::WorkspaceId
            ) -> Result<()> {
                INSERT OR REPLACE INTO keybinding_editors(item_id, workspace_id)
                VALUES (?, ?)
            }
        }

        query! {
            pub fn get_keybinding_editor(
                item_id: workspace::ItemId,
                workspace_id: workspace::WorkspaceId
            ) -> Result<Option<workspace::ItemId>> {
                SELECT item_id
                FROM keybinding_editors
                WHERE item_id = ? AND workspace_id = ?
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_ctx_cmp() {
        #[track_caller]
        fn cmp(a: &str, b: &str) -> bool {
            let a = gpui::KeyBindingContextPredicate::parse(a)
                .expect("Failed to parse keybinding context a");
            let b = gpui::KeyBindingContextPredicate::parse(b)
                .expect("Failed to parse keybinding context b");
            normalized_ctx_eq(&a, &b)
        }

        // Basic equality - identical expressions
        assert!(cmp("a && b", "a && b"));
        assert!(cmp("a || b", "a || b"));
        assert!(cmp("a == b", "a == b"));
        assert!(cmp("a != b", "a != b"));
        assert!(cmp("a > b", "a > b"));
        assert!(cmp("!a", "!a"));

        // AND operator - associative/commutative
        assert!(cmp("a && b", "b && a"));
        assert!(cmp("a && b && c", "c && b && a"));
        assert!(cmp("a && b && c", "b && a && c"));
        assert!(cmp("a && b && c && d", "d && c && b && a"));

        // OR operator - associative/commutative
        assert!(cmp("a || b", "b || a"));
        assert!(cmp("a || b || c", "c || b || a"));
        assert!(cmp("a || b || c", "b || a || c"));
        assert!(cmp("a || b || c || d", "d || c || b || a"));

        // Equality operator - associative/commutative
        assert!(cmp("a == b", "b == a"));
        assert!(cmp("x == y", "y == x"));

        // Inequality operator - associative/commutative
        assert!(cmp("a != b", "b != a"));
        assert!(cmp("x != y", "y != x"));

        // Complex nested expressions with associative operators
        assert!(cmp("(a && b) || c", "c || (a && b)"));
        assert!(cmp("(a && b) || c", "c || (b && a)"));
        assert!(cmp("(a || b) && c", "c && (a || b)"));
        assert!(cmp("(a || b) && c", "c && (b || a)"));
        assert!(cmp("(a && b) || (c && d)", "(c && d) || (a && b)"));
        assert!(cmp("(a && b) || (c && d)", "(d && c) || (b && a)"));

        // Multiple levels of nesting
        assert!(cmp("((a && b) || c) && d", "d && ((a && b) || c)"));
        assert!(cmp("((a && b) || c) && d", "d && (c || (b && a))"));
        assert!(cmp("a && (b || (c && d))", "(b || (c && d)) && a"));
        assert!(cmp("a && (b || (c && d))", "(b || (d && c)) && a"));

        // Negation with associative operators
        assert!(cmp("!a && b", "b && !a"));
        assert!(cmp("!a || b", "b || !a"));
        assert!(cmp("!(a && b) || c", "c || !(a && b)"));
        assert!(cmp("!(a && b) || c", "c || !(b && a)"));

        // Descendant operator (>) - NOT associative/commutative
        assert!(cmp("a > b", "a > b"));
        assert!(!cmp("a > b", "b > a"));
        assert!(!cmp("a > b > c", "c > b > a"));
        assert!(!cmp("a > b > c", "a > c > b"));

        // Mixed operators with descendant
        assert!(cmp("(a > b) && c", "c && (a > b)"));
        assert!(!cmp("(a > b) && c", "c && (b > a)"));
        assert!(cmp("(a > b) || (c > d)", "(c > d) || (a > b)"));
        assert!(!cmp("(a > b) || (c > d)", "(b > a) || (d > c)"));

        // Negative cases - different operators
        assert!(!cmp("a && b", "a || b"));
        assert!(!cmp("a == b", "a != b"));
        assert!(!cmp("a && b", "a > b"));
        assert!(!cmp("a || b", "a > b"));
        assert!(!cmp("a == b", "a && b"));
        assert!(!cmp("a != b", "a || b"));

        // Negative cases - different operands
        assert!(!cmp("a && b", "a && c"));
        assert!(!cmp("a && b", "c && d"));
        assert!(!cmp("a || b", "a || c"));
        assert!(!cmp("a || b", "c || d"));
        assert!(!cmp("a == b", "a == c"));
        assert!(!cmp("a != b", "a != c"));
        assert!(!cmp("a > b", "a > c"));
        assert!(!cmp("a > b", "c > b"));

        // Negative cases - with negation
        assert!(!cmp("!a", "a"));
        assert!(!cmp("!a && b", "a && b"));
        assert!(!cmp("!(a && b)", "a && b"));
        assert!(!cmp("!a || b", "a || b"));
        assert!(!cmp("!(a || b)", "a || b"));

        // Negative cases - complex expressions
        assert!(!cmp("(a && b) || c", "(a || b) && c"));
        assert!(!cmp("a && (b || c)", "a || (b && c)"));
        assert!(!cmp("(a && b) || (c && d)", "(a || b) && (c || d)"));
        assert!(!cmp("a > b && c", "a && b > c"));

        // Edge cases - multiple same operands
        assert!(cmp("a && a", "a && a"));
        assert!(cmp("a || a", "a || a"));
        assert!(cmp("a && a && b", "b && a && a"));
        assert!(cmp("a || a || b", "b || a || a"));

        // Edge cases - deeply nested
        assert!(cmp(
            "((a && b) || (c && d)) && ((e || f) && g)",
            "((e || f) && g) && ((c && d) || (a && b))"
        ));
        assert!(cmp(
            "((a && b) || (c && d)) && ((e || f) && g)",
            "(g && (f || e)) && ((d && c) || (b && a))"
        ));

        // Edge cases - repeated patterns
        assert!(cmp("(a && b) || (a && b)", "(b && a) || (b && a)"));
        assert!(cmp("(a || b) && (a || b)", "(b || a) && (b || a)"));

        // Negative cases - subtle differences
        assert!(!cmp("a && b && c", "a && b"));
        assert!(!cmp("a || b || c", "a || b"));
        assert!(!cmp("(a && b) || c", "a && (b || c)"));

        // a > b > c is not the same as a > c, should not be equal
        assert!(!cmp("a > b > c", "a > c"));

        // Double negation with complex expressions
        assert!(cmp("!(!(a && b))", "a && b"));
        assert!(cmp("!(!(a || b))", "a || b"));
        assert!(cmp("!(!(a > b))", "a > b"));
        assert!(cmp("!(!a) && b", "a && b"));
        assert!(cmp("!(!a) || b", "a || b"));
        assert!(cmp("!(!(a && b)) || c", "(a && b) || c"));
        assert!(cmp("!(!(a && b)) || c", "(b && a) || c"));
        assert!(cmp("!(!a)", "a"));
        assert!(cmp("a", "!(!a)"));
        assert!(cmp("!(!(!a))", "!a"));
        assert!(cmp("!(!(!(!a)))", "a"));
    }

    #[test]
    fn binding_is_unbound_by_unbind_respects_precedence() {
        let binding = gpui::KeyBinding::new("tab", mav_actions::OpenKeymap, None);
        let unbind =
            gpui::KeyBinding::new("tab", gpui::Unbind(binding.action().name().into()), None);

        let unbind_then_binding = vec![&unbind, &binding];
        assert!(!binding_is_unbound_by_unbind(
            &binding,
            1,
            &unbind_then_binding,
        ));

        let binding_then_unbind = vec![&binding, &unbind];
        assert!(binding_is_unbound_by_unbind(
            &binding,
            0,
            &binding_then_unbind,
        ));
    }
}
