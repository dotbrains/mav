use super::*;

pub(super) fn do_sign_in(state: &Entity<State>, http_client: &Arc<dyn HttpClient>, cx: &mut App) {
    if state.read(cx).is_signing_in() {
        return;
    }

    let weak_state = state.downgrade();
    let http_client = http_client.clone();

    let task = cx.spawn(async move |cx| {
        match do_oauth_flow(http_client, &*cx).await {
            Ok(creds) => {
                let persist_result = async {
                    let credentials_provider =
                        weak_state.read_with(&*cx, |s, _| s.credentials_provider.clone())?;
                    let json = serde_json::to_vec(&creds)?;
                    credentials_provider
                        .write_credentials(CREDENTIALS_KEY, "Bearer", &json, &*cx)
                        .await?;
                    anyhow::Ok(())
                }
                .await;

                match persist_result {
                    Ok(()) => {
                        weak_state
                            .update(cx, |s, cx| {
                                s.credentials = Some(creds);
                                s.sign_in_task = None;
                                s.last_auth_error = None;
                                cx.notify();
                            })
                            .log_err();
                    }
                    Err(err) => {
                        log::error!(
                            "ChatGPT subscription sign-in failed to persist credentials: {err:?}"
                        );
                        weak_state
                            .update(cx, |s, cx| {
                                s.sign_in_task = None;
                                s.last_auth_error =
                                    Some("Failed to save credentials. Please try again.".into());
                                cx.notify();
                            })
                            .log_err();
                    }
                }
            }
            Err(err) => {
                log::error!("ChatGPT subscription sign-in failed: {err:?}");
                weak_state
                    .update(cx, |s, cx| {
                        s.sign_in_task = None;
                        s.last_auth_error = Some("Sign-in failed. Please try again.".into());
                        cx.notify();
                    })
                    .log_err();
            }
        }
        anyhow::Ok(())
    });

    state.update(cx, |s, cx| {
        s.last_auth_error = None;
        s.sign_in_task = Some(task);
        cx.notify();
    });
}

pub(super) fn do_sign_out(state: &gpui::WeakEntity<State>, cx: &mut App) -> Task<Result<()>> {
    let weak_state = state.clone();
    // Clear credentials and cancel in-flight work immediately so the UI
    // reflects the sign-out right away.
    weak_state
        .update(cx, |s, cx| {
            s.auth_generation += 1;
            s.credentials = None;
            s.sign_in_task = None;
            s.refresh_task = None;
            s.last_auth_error = None;
            cx.notify();
        })
        .log_err();

    cx.spawn(async move |cx| {
        let credentials_provider =
            weak_state.read_with(&*cx, |s, _| s.credentials_provider.clone())?;
        credentials_provider
            .delete_credentials(CREDENTIALS_KEY, &*cx)
            .await
            .context("Failed to delete ChatGPT subscription credentials from keychain")?;
        anyhow::Ok(())
    })
}
