use super::{LspStore, lsp_ext_command};
use crate::lsp_command::{
    GetDocumentHighlights, GetDocumentSymbols, LinkedEditingRange, PerformRename, PrepareRename,
};
use rpc::AnyProtoClient;

pub(super) fn register_lsp_handlers(client: &AnyProtoClient) {
    client.add_entity_request_handler(LspStore::handle_lsp_query);
    client.add_entity_message_handler(LspStore::handle_lsp_query_response);
    client.add_entity_request_handler(LspStore::handle_restart_language_servers);
    client.add_entity_request_handler(LspStore::handle_stop_language_servers);
    client.add_entity_request_handler(LspStore::handle_cancel_language_server_work);
    client.add_entity_message_handler(LspStore::handle_start_language_server);
    client.add_entity_message_handler(LspStore::handle_update_language_server);
    client.add_entity_message_handler(LspStore::handle_language_server_log);
    client.add_entity_message_handler(LspStore::handle_update_diagnostic_summary);
    client.add_entity_request_handler(LspStore::handle_format_buffers);
    client.add_entity_request_handler(LspStore::handle_apply_code_action_kind);
    client.add_entity_request_handler(LspStore::handle_resolve_completion_documentation);
    client.add_entity_request_handler(LspStore::handle_apply_code_action);
    client.add_entity_request_handler(LspStore::handle_get_project_symbols);
    client.add_entity_request_handler(LspStore::handle_resolve_inlay_hint);
    client.add_entity_request_handler(LspStore::handle_resolve_code_action);
    client.add_entity_request_handler(LspStore::handle_resolve_document_link);
    client.add_entity_request_handler(LspStore::handle_get_color_presentation);
    client.add_entity_request_handler(LspStore::handle_open_buffer_for_symbol);
    client.add_entity_request_handler(LspStore::handle_refresh_inlay_hints);
    client.add_entity_request_handler(LspStore::handle_refresh_semantic_tokens);
    client.add_entity_request_handler(LspStore::handle_refresh_code_lens);
    client.add_entity_request_handler(LspStore::handle_on_type_formatting);
    client.add_entity_request_handler(LspStore::handle_apply_additional_edits_for_completion);
    client.add_entity_request_handler(LspStore::handle_register_buffer_with_language_servers);
    client.add_entity_request_handler(LspStore::handle_rename_project_entry);
    client.add_entity_request_handler(LspStore::handle_pull_workspace_diagnostics);
    client.add_entity_request_handler(LspStore::handle_lsp_get_completions);
    client.add_entity_request_handler(LspStore::handle_lsp_command::<GetDocumentHighlights>);
    client.add_entity_request_handler(LspStore::handle_lsp_command::<GetDocumentSymbols>);
    client.add_entity_request_handler(LspStore::handle_lsp_command::<PrepareRename>);
    client.add_entity_request_handler(LspStore::handle_lsp_command::<PerformRename>);
    client.add_entity_request_handler(LspStore::handle_lsp_command::<LinkedEditingRange>);

    client.add_entity_request_handler(LspStore::handle_lsp_ext_cancel_flycheck);
    client.add_entity_request_handler(LspStore::handle_lsp_ext_run_flycheck);
    client.add_entity_request_handler(LspStore::handle_lsp_ext_clear_flycheck);
    client.add_entity_request_handler(LspStore::handle_lsp_command::<lsp_ext_command::ExpandMacro>);
    client.add_entity_request_handler(LspStore::handle_lsp_command::<lsp_ext_command::OpenDocs>);
    client.add_entity_request_handler(
        LspStore::handle_lsp_command::<lsp_ext_command::GoToParentModule>,
    );
    client.add_entity_request_handler(
        LspStore::handle_lsp_command::<lsp_ext_command::GetLspRunnables>,
    );
    client.add_entity_request_handler(
        LspStore::handle_lsp_command::<lsp_ext_command::SwitchSourceHeader>,
    );
}
