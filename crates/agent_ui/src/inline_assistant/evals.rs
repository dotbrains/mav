use crate::InlineAssistant;
use agent::ThreadStore;
use client::{Client, RefreshLlmTokenListener, UserStore};
use editor::{Editor, MultiBuffer, MultiBufferOffset};
use eval_utils::{EvalOutput, NoProcessor};
use fs::FakeFs;
use futures::channel::mpsc;
use futures::stream::StreamExt as _;
use gpui::{AppContext, TestAppContext, UpdateGlobal as _};
use language::Buffer;
use language_model::{LanguageModelRegistry, SelectedModel};
use project::Project;
use prompt_store::PromptBuilder;
use std::str::FromStr;
use std::sync::Arc;
use util::test::marked_text_ranges;
use workspace::Workspace;

#[derive(Debug)]
enum InlineAssistantOutput {
    Success {
        completion: Option<String>,
        description: Option<String>,
        full_buffer_text: String,
    },
    Failure {
        failure: String,
    },
    // These fields are used for logging
    #[allow(unused)]
    Malformed {
        completion: Option<String>,
        description: Option<String>,
        failure: Option<String>,
    },
}

fn run_inline_assistant_test<SetupF, TestF>(
    base_buffer: String,
    prompt: String,
    setup: SetupF,
    test: TestF,
    cx: &mut TestAppContext,
) -> InlineAssistantOutput
where
    SetupF: FnOnce(&mut gpui::VisualTestContext),
    TestF: FnOnce(&mut gpui::VisualTestContext),
{
    let fs = FakeFs::new(cx.executor());
    let app_state = cx.update(|cx| workspace::AppState::test(cx));
    let prompt_builder = Arc::new(PromptBuilder::new(None).unwrap());
    let http = Arc::new(reqwest_client::ReqwestClient::user_agent("agent tests").unwrap());
    let client = cx.update(|cx| {
        cx.set_http_client(http);
        Client::production(cx)
    });
    let mut inline_assistant = InlineAssistant::new(fs.clone(), prompt_builder);

    let (tx, mut completion_rx) = mpsc::unbounded();
    inline_assistant.set_completion_receiver(tx);

    // Initialize settings and client
    cx.update(|cx| {
        gpui_tokio::init(cx);
        settings::init(cx);
        client::init(&client, cx);
        workspace::init(app_state.clone(), cx);
        let user_store = cx.new(|cx| UserStore::new(client.clone(), cx));
        language_model::init(cx);
        RefreshLlmTokenListener::register(client.clone(), user_store.clone(), cx);
        language_models::init(user_store, client.clone(), cx);

        cx.set_global(inline_assistant);
    });

    let foreground_executor = cx.foreground_executor().clone();
    let project = foreground_executor.block_test(async { Project::test(fs.clone(), [], cx).await });

    // Create workspace with window
    let (workspace, cx) = cx.add_window_view(|window, cx| {
        window.activate_window();
        Workspace::new(None, project.clone(), app_state.clone(), window, cx)
    });

    setup(cx);

    let (_editor, buffer) = cx.update(|window, cx| {
        let buffer = cx.new(|cx| Buffer::local("", cx));
        let multibuffer = cx.new(|cx| MultiBuffer::singleton(buffer.clone(), cx));
        let editor = cx.new(|cx| Editor::for_multibuffer(multibuffer, None, window, cx));
        editor.update(cx, |editor, cx| {
            let (unmarked_text, selection_ranges) = marked_text_ranges(&base_buffer, true);
            editor.set_text(unmarked_text, window, cx);
            editor.change_selections(Default::default(), window, cx, |s| {
                s.select_ranges(
                    selection_ranges
                        .into_iter()
                        .map(|range| MultiBufferOffset(range.start)..MultiBufferOffset(range.end)),
                )
            })
        });

        let thread_store = cx.new(|cx| ThreadStore::new(cx));

        // Add editor to workspace
        workspace.update(cx, |workspace, cx| {
            workspace.add_item_to_active_pane(Box::new(editor.clone()), None, true, window, cx);
        });

        // Call assist method
        InlineAssistant::update_global(cx, |inline_assistant, cx| {
            let assist_id = inline_assistant
                .assist(
                    &editor,
                    workspace.downgrade(),
                    project.downgrade(),
                    thread_store,
                    Some(prompt),
                    window,
                    cx,
                )
                .unwrap();

            inline_assistant.start_assist(assist_id, window, cx);
        });

        (editor, buffer)
    });

    cx.run_until_parked();

    test(cx);

    let assist_id = foreground_executor
        .block_test(async { completion_rx.next().await })
        .unwrap()
        .unwrap();

    let (completion, description, failure) = cx.update(|_, cx| {
        InlineAssistant::update_global(cx, |inline_assistant, cx| {
            let codegen = inline_assistant.get_codegen(assist_id, cx).unwrap();

            let completion = codegen.read(cx).current_completion();
            let description = codegen.read(cx).current_description();
            let failure = codegen.read(cx).current_failure();

            (completion, description, failure)
        })
    });

    if failure.is_some() && (completion.is_some() || description.is_some()) {
        InlineAssistantOutput::Malformed {
            completion,
            description,
            failure,
        }
    } else if let Some(failure) = failure {
        InlineAssistantOutput::Failure { failure }
    } else {
        InlineAssistantOutput::Success {
            completion,
            description,
            full_buffer_text: buffer.read_with(cx, |buffer, _| buffer.text()),
        }
    }
}

#[test]
#[cfg_attr(not(feature = "unit-eval"), ignore)]
fn eval_single_cursor_edit() {
    run_eval(
        20,
        1.0,
        "Rename this variable to buffer_text".to_string(),
        indoc::indoc! {"
            struct EvalExampleStruct {
                text: Strˇing,
                prompt: String,
            }
        "}
        .to_string(),
        exact_buffer_match(indoc::indoc! {"
            struct EvalExampleStruct {
                buffer_text: String,
                prompt: String,
            }
        "}),
    );
}

#[test]
#[cfg_attr(not(feature = "unit-eval"), ignore)]
fn eval_cant_do() {
    run_eval(
        20,
        0.95,
        "Rename the struct to EvalExampleStructNope",
        indoc::indoc! {"
            struct EvalExampleStruct {
                text: Strˇing,
                prompt: String,
            }
        "},
        uncertain_output,
    );
}

#[test]
#[cfg_attr(not(feature = "unit-eval"), ignore)]
fn eval_unclear() {
    run_eval(
        20,
        0.95,
        "Make exactly the change I want you to make",
        indoc::indoc! {"
            struct EvalExampleStruct {
                text: Strˇing,
                prompt: String,
            }
        "},
        uncertain_output,
    );
}

#[test]
#[cfg_attr(not(feature = "unit-eval"), ignore)]
fn eval_empty_buffer() {
    run_eval(
        20,
        1.0,
        "Write a Python hello, world program".to_string(),
        "ˇ".to_string(),
        |output| match output {
            InlineAssistantOutput::Success {
                full_buffer_text, ..
            } => {
                if full_buffer_text.is_empty() {
                    EvalOutput::failed("expected some output".to_string())
                } else {
                    EvalOutput::passed(format!("Produced {full_buffer_text}"))
                }
            }
            o @ InlineAssistantOutput::Failure { .. } => EvalOutput::failed(format!(
                "Assistant output does not match expected output: {:?}",
                o
            )),
            o @ InlineAssistantOutput::Malformed { .. } => EvalOutput::failed(format!(
                "Assistant output does not match expected output: {:?}",
                o
            )),
        },
    );
}

fn run_eval(
    iterations: usize,
    expected_pass_ratio: f32,
    prompt: impl Into<String>,
    buffer: impl Into<String>,
    judge: impl Fn(InlineAssistantOutput) -> eval_utils::EvalOutput<()> + Send + Sync + 'static,
) {
    let buffer = buffer.into();
    let prompt = prompt.into();

    eval_utils::eval(iterations, expected_pass_ratio, NoProcessor, move || {
        let dispatcher = gpui::TestDispatcher::new(rand::random());
        let mut cx = TestAppContext::build(dispatcher, None);
        cx.skip_drawing();

        let output = run_inline_assistant_test(
            buffer.clone(),
            prompt.clone(),
            |cx| {
                // Reconfigure to use a real model instead of the fake one
                let model_name = std::env::var("MAV_AGENT_MODEL")
                    .unwrap_or("anthropic/claude-sonnet-4-latest".into());

                let selected_model = SelectedModel::from_str(&model_name)
                    .expect("Invalid model format. Use 'provider/model-id'");

                log::info!("Selected model: {selected_model:?}");

                cx.update(|_, cx| {
                    LanguageModelRegistry::global(cx).update(cx, |registry, cx| {
                        registry.select_inline_assistant_model(Some(&selected_model), cx);
                    });
                });
            },
            |_cx| {
                log::info!("Waiting for actual response from the LLM...");
            },
            &mut cx,
        );

        cx.quit();

        judge(output)
    });
}

fn uncertain_output(output: InlineAssistantOutput) -> EvalOutput<()> {
    match &output {
        o @ InlineAssistantOutput::Success {
            completion,
            description,
            ..
        } => {
            if description.is_some() && completion.is_none() {
                EvalOutput::passed(format!(
                    "Assistant produced no completion, but a description:\n{}",
                    description.as_ref().unwrap()
                ))
            } else {
                EvalOutput::failed(format!("Assistant produced a completion:\n{:?}", o))
            }
        }
        InlineAssistantOutput::Failure {
            failure: error_message,
        } => EvalOutput::passed(format!(
            "Assistant produced a failure message: {}",
            error_message
        )),
        o @ InlineAssistantOutput::Malformed { .. } => {
            EvalOutput::failed(format!("Assistant produced a malformed response:\n{:?}", o))
        }
    }
}

fn exact_buffer_match(
    correct_output: impl Into<String>,
) -> impl Fn(InlineAssistantOutput) -> EvalOutput<()> {
    let correct_output = correct_output.into();
    move |output| match output {
        InlineAssistantOutput::Success {
            description,
            full_buffer_text,
            ..
        } => {
            if full_buffer_text == correct_output && description.is_none() {
                EvalOutput::passed("Assistant output matches")
            } else if full_buffer_text == correct_output {
                EvalOutput::failed(format!(
                    "Assistant output produced an unescessary description description:\n{:?}",
                    description
                ))
            } else {
                EvalOutput::failed(format!(
                    "Assistant output does not match expected output:\n{:?}\ndescription:\n{:?}",
                    full_buffer_text, description
                ))
            }
        }
        o @ InlineAssistantOutput::Failure { .. } => EvalOutput::failed(format!(
            "Assistant output does not match expected output: {:?}",
            o
        )),
        o @ InlineAssistantOutput::Malformed { .. } => EvalOutput::failed(format!(
            "Assistant output does not match expected output: {:?}",
            o
        )),
    }
}
