use super::*;

impl LocalLspStore {
    pub(super) async fn try_resolve_code_action(
        lang_server: &LanguageServer,
        action: &mut CodeAction,
        request_timeout: Duration,
    ) -> anyhow::Result<()> {
        match &mut action.lsp_action {
            LspAction::Action(lsp_action) => {
                if !action.resolved
                    && GetCodeActions::can_resolve_actions(&lang_server.capabilities())
                    && lsp_action.data.is_some()
                    && (lsp_action.command.is_none() || lsp_action.edit.is_none())
                {
                    **lsp_action = lang_server
                        .request::<lsp::request::CodeActionResolveRequest>(
                            *lsp_action.clone(),
                            request_timeout,
                        )
                        .await
                        .into_response()?;
                }
            }
            LspAction::CodeLens(lens) => {
                if !action.resolved && GetCodeLens::can_resolve_lens(&lang_server.capabilities()) {
                    *lens = lang_server
                        .request::<lsp::request::CodeLensResolve>(lens.clone(), request_timeout)
                        .await
                        .into_response()?;
                }
            }
            LspAction::Command(_) => {}
        }

        action.resolved = true;
        anyhow::Ok(())
    }
}
