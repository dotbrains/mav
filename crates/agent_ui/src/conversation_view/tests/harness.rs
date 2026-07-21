use super::*;

pub(crate) async fn setup_conversation_view(
    agent: impl AgentServer + 'static,
    cx: &mut TestAppContext,
) -> (Entity<ConversationView>, &mut VisualTestContext) {
    setup_conversation_view_with_initial_content_opt(agent, None, cx).await
}

pub(crate) async fn setup_conversation_view_with_initial_content(
    agent: impl AgentServer + 'static,
    initial_content: AgentInitialContent,
    cx: &mut TestAppContext,
) -> (Entity<ConversationView>, &mut VisualTestContext) {
    setup_conversation_view_with_initial_content_opt(agent, Some(initial_content), cx).await
}

async fn setup_conversation_view_with_initial_content_opt(
    agent: impl AgentServer + 'static,
    initial_content: Option<AgentInitialContent>,
    cx: &mut TestAppContext,
) -> (Entity<ConversationView>, &mut VisualTestContext) {
    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let thread_store = cx.update(|_window, cx| cx.new(|cx| ThreadStore::new(cx)));
    let connection_store =
        cx.update(|_window, cx| cx.new(|cx| AgentConnectionStore::new(project.clone(), cx)));
    let agent_key = Agent::Custom { id: "Test".into() };

    let conversation_view = cx.update(|window, cx| {
        cx.new(|cx| {
            ConversationView::new(
                Rc::new(agent),
                connection_store.clone(),
                agent_key.clone(),
                None,
                None,
                None,
                None,
                initial_content,
                workspace.downgrade(),
                project,
                Some(thread_store),
                AgentThreadSource::AgentPanel,
                window,
                cx,
            )
        })
    });
    cx.run_until_parked();

    (conversation_view, cx)
}

pub(crate) fn add_to_workspace(
    conversation_view: Entity<ConversationView>,
    cx: &mut VisualTestContext,
) {
    let workspace =
        conversation_view.read_with(cx, |thread_view, _cx| thread_view.workspace.clone());

    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.add_item_to_active_pane(
                Box::new(cx.new(|_| ThreadViewItem(conversation_view.clone()))),
                None,
                true,
                window,
                cx,
            );
        })
        .unwrap();
}

struct ThreadViewItem(Entity<ConversationView>);

impl Item for ThreadViewItem {
    type Event = ();

    fn include_in_nav_history() -> bool {
        false
    }

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        "Test".into()
    }
}

impl EventEmitter<()> for ThreadViewItem {}

impl Focusable for ThreadViewItem {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.0.read(cx).focus_handle(cx)
    }
}

impl Render for ThreadViewItem {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title_editor = self
            .0
            .read(cx)
            .active_thread()
            .map(|t| t.read(cx).title_editor.clone());

        v_flex().children(title_editor).child(self.0.clone())
    }
}
