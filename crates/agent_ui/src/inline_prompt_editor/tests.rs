use super::*;
use crate::terminal_codegen::TerminalCodegen;
use agent::ThreadStore;
use collections::VecDeque;
use fs::FakeFs;
use gpui::{TestAppContext, VisualTestContext};
use language::Buffer;
use project::Project;
use settings::SettingsStore;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use terminal::TerminalBuilder;
use terminal::terminal_settings::CursorShape;
use util::path;
use util::paths::PathStyle;
use uuid::Uuid;

fn init_test(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
        theme::init(theme::LoadThemes::JustBase, cx);
        theme_settings::init(theme::LoadThemes::JustBase, cx);
        editor::init(cx);
        release_channel::init(semver::Version::new(0, 0, 0), cx);
        language_model::LanguageModelRegistry::test(cx);
        prompt_store::init(cx);
    });
}

fn build_terminal_prompt_editor(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
) -> Entity<PromptEditor<TerminalCodegen>> {
    let thread_store = cx.update(|_window, cx| cx.new(|cx| ThreadStore::new(cx)));
    let fs = FakeFs::new(cx.executor());

    let terminal = cx.update(|_window, cx| {
        cx.new(|cx| {
            TerminalBuilder::new_display_only(
                CursorShape::default(),
                settings::AlternateScroll::On,
                None,
                0,
                cx.background_executor(),
                PathStyle::local(),
            )
            .subscribe(cx)
        })
    });

    let session_id = Uuid::new_v4();
    let codegen = cx.update(|_window, cx| cx.new(|_| TerminalCodegen::new(terminal, session_id)));

    let prompt_buffer = cx.update(|_window, cx| {
        cx.new(|cx| MultiBuffer::singleton(cx.new(|cx| Buffer::local("", cx)), cx))
    });

    let project = workspace.update(cx, |workspace, _cx| workspace.project().downgrade());

    cx.update(|window, cx| {
        cx.new(|cx| {
            PromptEditor::new_terminal(
                TerminalInlineAssistId::default(),
                VecDeque::new(),
                prompt_buffer,
                codegen,
                session_id,
                fs,
                thread_store,
                project,
                workspace.downgrade(),
                window,
                cx,
            )
        })
    })
}

#[gpui::test]
async fn test_secondary_confirm_emits_execute_true_in_terminal_mode(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project", serde_json::json!({"file": ""}))
        .await;
    let project = Project::test(fs, [Path::new(path!("/project"))], cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));

    let prompt_editor = build_terminal_prompt_editor(&workspace, cx);

    // Set the codegen status to Done so that confirm logic emits ConfirmRequested.
    prompt_editor.update(cx, |editor, cx| {
        editor.codegen().update(cx, |codegen, _| {
            codegen.status = CodegenStatus::Done;
        });
        editor.edited_since_done = false;
    });

    let events: Rc<RefCell<Vec<PromptEditorEvent>>> = Rc::new(RefCell::new(Vec::new()));
    let events_clone = events.clone();
    cx.update(|_window, cx| {
        cx.subscribe(&prompt_editor, move |_, event: &PromptEditorEvent, _cx| {
            events_clone.borrow_mut().push(match event {
                PromptEditorEvent::ConfirmRequested { execute } => {
                    PromptEditorEvent::ConfirmRequested { execute: *execute }
                }
                PromptEditorEvent::StartRequested => PromptEditorEvent::StartRequested,
                PromptEditorEvent::StopRequested => PromptEditorEvent::StopRequested,
                PromptEditorEvent::CancelRequested => PromptEditorEvent::CancelRequested,
                PromptEditorEvent::Resized { height_in_lines } => PromptEditorEvent::Resized {
                    height_in_lines: *height_in_lines,
                },
            });
        })
        .detach();
    });

    // Dispatch menu::SecondaryConfirm (cmd-enter).
    prompt_editor.update(cx, |editor, cx| {
        editor.handle_confirm(true, cx);
    });

    let events = events.borrow();
    assert_eq!(events.len(), 1, "Expected exactly one event");
    assert!(
        matches!(
            events[0],
            PromptEditorEvent::ConfirmRequested { execute: true }
        ),
        "Expected ConfirmRequested with execute: true, got {:?}",
        match &events[0] {
            PromptEditorEvent::ConfirmRequested { execute } =>
                format!("ConfirmRequested {{ execute: {} }}", execute),
            _ => "other event".to_string(),
        }
    );
}

#[gpui::test]
async fn test_confirm_emits_execute_false_in_terminal_mode(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project", serde_json::json!({"file": ""}))
        .await;
    let project = Project::test(fs, [Path::new(path!("/project"))], cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));

    let prompt_editor = build_terminal_prompt_editor(&workspace, cx);

    prompt_editor.update(cx, |editor, cx| {
        editor.codegen().update(cx, |codegen, _| {
            codegen.status = CodegenStatus::Done;
        });
        editor.edited_since_done = false;
    });

    let events: Rc<RefCell<Vec<PromptEditorEvent>>> = Rc::new(RefCell::new(Vec::new()));
    let events_clone = events.clone();
    cx.update(|_window, cx| {
        cx.subscribe(&prompt_editor, move |_, event: &PromptEditorEvent, _cx| {
            events_clone.borrow_mut().push(match event {
                PromptEditorEvent::ConfirmRequested { execute } => {
                    PromptEditorEvent::ConfirmRequested { execute: *execute }
                }
                PromptEditorEvent::StartRequested => PromptEditorEvent::StartRequested,
                PromptEditorEvent::StopRequested => PromptEditorEvent::StopRequested,
                PromptEditorEvent::CancelRequested => PromptEditorEvent::CancelRequested,
                PromptEditorEvent::Resized { height_in_lines } => PromptEditorEvent::Resized {
                    height_in_lines: *height_in_lines,
                },
            });
        })
        .detach();
    });

    // Dispatch menu::Confirm (enter) — should emit execute: false even in terminal mode.
    prompt_editor.update(cx, |editor, cx| {
        editor.handle_confirm(false, cx);
    });

    let events = events.borrow();
    assert_eq!(events.len(), 1, "Expected exactly one event");
    assert!(
        matches!(
            events[0],
            PromptEditorEvent::ConfirmRequested { execute: false }
        ),
        "Expected ConfirmRequested with execute: false"
    );
}
