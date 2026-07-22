use super::*;

pub(super) fn variable(name: &str, value: &str, variables_reference: i64) -> Variable {
    Variable {
        name: name.into(),
        value: value.into(),
        type_: None,
        presentation_hint: None,
        evaluate_name: None,
        variables_reference,
        named_variables: None,
        indexed_variables: None,
        memory_reference: None,
        declaration_location_reference: None,
        value_location_reference: None,
    }
}

pub(super) fn simple_variable(name: &str, value: &str) -> Variable {
    variable(name, value, 0)
}

pub(super) fn scope(name: &str, variables_reference: i64) -> Scope {
    Scope {
        name: name.into(),
        presentation_hint: None,
        variables_reference,
        named_variables: None,
        indexed_variables: None,
        expensive: false,
        source: None,
        line: None,
        column: None,
        end_line: None,
        end_column: None,
    }
}

pub(super) fn local_scope(name: &str, variables_reference: i64) -> Scope {
    Scope {
        presentation_hint: Some(dap::ScopePresentationHint::Locals),
        ..scope(name, variables_reference)
    }
}

pub(super) fn test_js_stack_frame() -> StackFrame {
    StackFrame {
        id: 1,
        name: "Stack Frame 1".into(),
        source: Some(dap::Source {
            name: Some("test.js".into()),
            path: Some(path!("/project/src/test.js").into()),
            source_reference: None,
            presentation_hint: None,
            origin: None,
            sources: None,
            adapter_data: None,
            checksums: None,
        }),
        line: 1,
        column: 1,
        end_line: None,
        end_column: None,
        can_restart: None,
        instruction_pointer_reference: None,
        module_id: None,
        presentation_hint: None,
    }
}

pub(super) async fn emit_stopped(client: &dap::client::DebugAdapterClient) {
    client
        .fake_event(dap::messages::Events::Stopped(dap::StoppedEvent {
            reason: dap::StoppedEventReason::Pause,
            description: None,
            thread_id: Some(1),
            preserve_focus_hint: None,
            text: None,
            all_threads_stopped: None,
            hit_breakpoint_ids: None,
        }))
        .await;
}
