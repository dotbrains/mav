pub(super) fn should_log_lsp_request_failure(message: &str) -> bool {
    // content modified is a weird failure mode of rust-analyzer
    // where requests are denied before its loaded a project
    message.ends_with("content modified") || message.ends_with("server cancelled the request")
}
