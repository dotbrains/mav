use super::*;

impl Client {
    pub fn authenticate_with_browser(self: &Arc<Self>, cx: &AsyncApp) -> Task<Result<Credentials>> {
        let http = self.http.clone();
        let this = self.clone();
        cx.spawn(async move |cx| {
            let background = cx.background_executor().clone();

            let (open_url_tx, open_url_rx) = oneshot::channel::<String>();
            cx.update(|cx| {
                cx.spawn(async move |cx| {
                    if let Ok(url) = open_url_rx.await {
                        cx.update(|cx| cx.open_url(&url));
                    }
                })
                .detach();
            });

            let credentials = background
                .clone()
                .spawn(async move {
                    // Generate a pair of asymmetric encryption keys. The public key will be used by the
                    // mav server to encrypt the user's access token, so that it can'be intercepted by
                    // any other app running on the user's device.
                    let (public_key, private_key) =
                        rpc::auth::keypair().context("failed to generate keypair for auth")?;
                    let public_key = String::try_from(public_key)
                        .context("failed to serialize public key for auth")?;

                    if let Some((login, token)) =
                        IMPERSONATE_LOGIN.as_ref().zip(ADMIN_API_TOKEN.as_ref())
                    {
                        if !*USE_WEB_LOGIN {
                            eprintln!("authenticate as admin {login}, {token}");

                            return this
                                .authenticate_as_admin(http, login.clone(), token.clone())
                                .await;
                        }
                    }

                    // Start an HTTP server to receive the redirect from Mav's sign-in page.
                    let server = tiny_http::Server::http("127.0.0.1:0")
                        .map_err(|e| anyhow!(e).context("failed to bind callback port"))?;
                    let port = server
                        .server_addr()
                        .to_ip()
                        .context("server not bound to a TCP address")?
                        .port();

                    #[derive(Serialize)]
                    struct NativeAppSignInQueryParams {
                        native_app_port: u16,
                        native_app_public_key: String,
                        system_id: Option<Arc<str>>,
                    }

                    // Open the Mav sign-in page in the user's browser, with query parameters that indicate
                    // that the user is signing in from a Mav app running on the same device.
                    let url = http.build_url(&format!(
                        "/native_app_signin?{}",
                        serde_urlencoded::to_string(&NativeAppSignInQueryParams {
                            native_app_port: port,
                            native_app_public_key: public_key,
                            system_id: this.telemetry.system_id(),
                        })?
                    ));

                    open_url_tx.send(url).log_err();

                    #[derive(Deserialize)]
                    struct CallbackParams {
                        pub user_id: String,
                        pub access_token: String,
                    }

                    // Receive the HTTP request from the user's browser. Retrieve the user id and encrypted
                    // access token from the query params.
                    //
                    // TODO - Avoid ever starting more than one HTTP server. Maybe switch to using a
                    // custom URL scheme instead of this local HTTP server.
                    let (user_id, access_token) = background
                        .spawn(async move {
                            for _ in 0..100 {
                                if let Some(req) = server.recv_timeout(Duration::from_secs(1))? {
                                    let path = req.url();
                                    let url = Url::parse(&format!("http://example.com{}", path))
                                        .context("failed to parse login notification url")?;
                                    let callback_params: CallbackParams =
                                        serde_urlencoded::from_str(url.query().unwrap_or_default())
                                            .context(
                                                "failed to parse sign-in callback query parameters",
                                            )?;

                                    let post_auth_url =
                                        http.build_url("/native_app_signin_succeeded");
                                    req.respond(
                                        tiny_http::Response::empty(302).with_header(
                                            tiny_http::Header::from_bytes(
                                                &b"Location"[..],
                                                post_auth_url.as_bytes(),
                                            )
                                            .unwrap(),
                                        ),
                                    )
                                    .context("failed to respond to login http request")?;
                                    return Ok((
                                        callback_params.user_id,
                                        callback_params.access_token,
                                    ));
                                }
                            }

                            anyhow::bail!("didn't receive login redirect");
                        })
                        .await?;

                    let access_token = private_key
                        .decrypt_string(&access_token)
                        .context("failed to decrypt access token")?;

                    Ok(Credentials {
                        user_id: user_id.parse()?,
                        access_token,
                    })
                })
                .await?;

            cx.update(|cx| cx.activate(true));
            Ok(credentials)
        })
    }

    async fn authenticate_as_admin(
        self: &Arc<Self>,
        http: Arc<HttpClientWithUrl>,
        login: String,
        api_token: String,
    ) -> Result<Credentials> {
        #[derive(Serialize)]
        struct ImpersonateUserBody {
            github_login: String,
        }

        #[derive(Deserialize)]
        struct ImpersonateUserResponse {
            user_id: u64,
            access_token: String,
        }

        let url = self
            .http
            .build_mav_cloud_url("/internal/users/impersonate")?;
        let request = Request::post(url.as_str())
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {api_token}"))
            .body(
                serde_json::to_string(&ImpersonateUserBody {
                    github_login: login,
                })?
                .into(),
            )?;

        let mut response = http.send(request).await?;
        let mut body = String::new();
        response.body_mut().read_to_string(&mut body).await?;
        anyhow::ensure!(
            response.status().is_success(),
            "admin user request failed {} - {}",
            response.status().as_u16(),
            body,
        );
        let response: ImpersonateUserResponse = serde_json::from_str(&body)?;

        Ok(Credentials {
            user_id: response.user_id,
            access_token: response.access_token,
        })
    }
}
