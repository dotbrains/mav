use super::*;

pub struct State {
    pub(super) api_key_state: ApiKeyState,
    pub(super) credentials_provider: Arc<dyn CredentialsProvider>,
}

impl State {
    pub(super) fn is_authenticated(&self) -> bool {
        self.api_key_state.has_key()
    }

    pub(super) fn set_api_key(
        &mut self,
        api_key: Option<String>,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let credentials_provider = self.credentials_provider.clone();
        let api_url = MistralLanguageModelProvider::api_url(cx);
        self.api_key_state.store(
            api_url,
            api_key,
            |this| &mut this.api_key_state,
            credentials_provider,
            cx,
        )
    }

    pub(super) fn authenticate(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), AuthenticateError>> {
        let credentials_provider = self.credentials_provider.clone();
        let api_url = MistralLanguageModelProvider::api_url(cx);
        self.api_key_state.load_if_needed(
            api_url,
            |this| &mut this.api_key_state,
            credentials_provider,
            cx,
        )
    }
}
