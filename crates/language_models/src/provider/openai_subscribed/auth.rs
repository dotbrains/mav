use super::*;

pub(super) async fn get_fresh_credentials(
    state: &gpui::WeakEntity<State>,
    http_client: &Arc<dyn HttpClient>,
    cx: &mut AsyncApp,
) -> Result<CodexCredentials, LanguageModelCompletionError> {
    let (creds, existing_task) = state
        .read_with(&*cx, |s, _| (s.credentials.clone(), s.refresh_task.clone()))
        .map_err(LanguageModelCompletionError::Other)?;

    let creds = creds.ok_or(LanguageModelCompletionError::NoApiKey {
        provider: PROVIDER_NAME,
    })?;

    if !creds.is_expired() {
        return Ok(creds);
    }

    // If another caller is already refreshing, await their result.
    if let Some(shared_task) = existing_task {
        return shared_task
            .await
            .map_err(|e| LanguageModelCompletionError::Other(anyhow::anyhow!("{e}")));
    }

    // We are the first caller to notice expiry — spawn the refresh task.
    let http_client_clone = http_client.clone();
    let state_clone = state.clone();
    let refresh_token_value = creds.refresh_token.clone();

    // Capture the generation so we can detect sign-outs that happened during refresh.
    let generation = state
        .read_with(&*cx, |s, _| s.auth_generation)
        .map_err(LanguageModelCompletionError::Other)?;

    let shared_task = cx
        .spawn(async move |cx| {
            let result = refresh_token(&http_client_clone, &refresh_token_value).await;

            match result {
                Ok(refreshed) => {
                    let persist_result: Result<CodexCredentials, Arc<anyhow::Error>> = async {
                        // Check if auth_generation changed (sign-out during refresh).
                        let current_generation = state_clone
                            .read_with(&*cx, |s, _| s.auth_generation)
                            .map_err(|e| Arc::new(e))?;
                        if current_generation != generation {
                            return Err(Arc::new(anyhow!(
                                "Sign-out occurred during token refresh"
                            )));
                        }

                        let credentials_provider = state_clone
                            .read_with(&*cx, |s, _| s.credentials_provider.clone())
                            .map_err(|e| Arc::new(e))?;

                        let json =
                            serde_json::to_vec(&refreshed).map_err(|e| Arc::new(e.into()))?;

                        credentials_provider
                            .write_credentials(CREDENTIALS_KEY, "Bearer", &json, &*cx)
                            .await
                            .map_err(|e| Arc::new(e))?;

                        state_clone
                            .update(cx, |s, _| {
                                s.credentials = Some(refreshed.clone());
                                s.refresh_task = None;
                            })
                            .map_err(|e| Arc::new(e))?;

                        Ok(refreshed)
                    }
                    .await;

                    // Clear refresh_task on failure too.
                    if persist_result.is_err() {
                        let _ = state_clone.update(cx, |s, _| {
                            s.refresh_task = None;
                        });
                    }

                    persist_result
                }
                Err(RefreshError::Fatal(e)) => {
                    log::error!("ChatGPT subscription token refresh failed fatally: {e:?}");
                    let _ = state_clone.update(cx, |s, cx| {
                        s.refresh_task = None;
                        s.credentials = None;
                        s.last_auth_error =
                            Some("Your session has expired. Please sign in again.".into());
                        cx.notify();
                    });
                    // Also clear the keychain so stale credentials aren't loaded next time.
                    if let Ok(credentials_provider) =
                        state_clone.read_with(&*cx, |s, _| s.credentials_provider.clone())
                    {
                        credentials_provider
                            .delete_credentials(CREDENTIALS_KEY, &*cx)
                            .await
                            .log_err();
                    }
                    Err(Arc::new(e))
                }
                Err(RefreshError::Transient(e)) => {
                    log::warn!("ChatGPT subscription token refresh failed transiently: {e:?}");
                    let _ = state_clone.update(cx, |s, _| {
                        s.refresh_task = None;
                    });
                    Err(Arc::new(e))
                }
            }
        })
        .shared();

    // Store the shared task so concurrent callers can join on it.
    state
        .update(cx, |s, _| {
            s.refresh_task = Some(shared_task.clone());
        })
        .map_err(LanguageModelCompletionError::Other)?;

    shared_task
        .await
        .map_err(|e| LanguageModelCompletionError::Other(anyhow::anyhow!("{e}")))
}
