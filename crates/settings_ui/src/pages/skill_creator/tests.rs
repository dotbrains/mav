#[cfg(test)]
mod tests {
    use super::github_import::*;
    use super::persistence::*;
    use super::*;
    use agent_skills::{SkillSource, parse_skill_frontmatter};
    use fs::FakeFs;
    use std::{
        collections::VecDeque,
        io,
        path::Path,
        pin::Pin,
        sync::{Arc, Mutex},
        task::{self, Poll},
    };

    struct TestHttpClient {
        responses: Mutex<VecDeque<(StatusCode, AsyncBody)>>,
        authorization_headers: Mutex<Vec<Option<String>>>,
        redirect_policies: Mutex<Vec<http_client::RedirectPolicy>>,
    }

    impl TestHttpClient {
        fn new(status: u16, body: AsyncBody) -> Arc<dyn HttpClient> {
            Self::new_sequence(vec![(status, body)])
        }

        fn new_sequence(responses: Vec<(u16, AsyncBody)>) -> Arc<Self> {
            Arc::new(Self {
                responses: Mutex::new(
                    responses
                        .into_iter()
                        .map(|(status, body)| {
                            (
                                StatusCode::from_u16(status)
                                    .expect("test status code should be valid"),
                                body,
                            )
                        })
                        .collect(),
                ),
                authorization_headers: Mutex::new(Vec::new()),
                redirect_policies: Mutex::new(Vec::new()),
            })
        }

        fn authorization_headers(&self) -> Vec<Option<String>> {
            self.authorization_headers
                .lock()
                .expect("authorization header mutex should not be poisoned")
                .clone()
        }

        fn redirect_policies(&self) -> Vec<http_client::RedirectPolicy> {
            self.redirect_policies
                .lock()
                .expect("redirect policy mutex should not be poisoned")
                .clone()
        }
    }

    impl HttpClient for TestHttpClient {
        fn user_agent(&self) -> Option<&http_client::http::HeaderValue> {
            None
        }

        fn proxy(&self) -> Option<&Url> {
            None
        }

        fn send(
            &self,
            req: http_client::Request<AsyncBody>,
        ) -> futures::future::BoxFuture<'static, Result<http_client::Response<AsyncBody>>> {
            let authorization_header = req
                .headers()
                .get("Authorization")
                .and_then(|header| header.to_str().ok())
                .map(ToString::to_string);

            match self.authorization_headers.lock() {
                Ok(mut authorization_headers) => authorization_headers.push(authorization_header),
                Err(_) => {
                    return Box::pin(async {
                        Err(anyhow::anyhow!(
                            "test authorization header mutex was poisoned"
                        ))
                    });
                }
            }

            let redirect_policy = req
                .extensions()
                .get::<http_client::RedirectPolicy>()
                .cloned()
                .unwrap_or_default();
            match self.redirect_policies.lock() {
                Ok(mut redirect_policies) => redirect_policies.push(redirect_policy),
                Err(_) => {
                    return Box::pin(async {
                        Err(anyhow::anyhow!("test redirect policy mutex was poisoned"))
                    });
                }
            }

            let response = match self.responses.lock() {
                Ok(mut responses) => responses.pop_front(),
                Err(_) => {
                    return Box::pin(async {
                        Err(anyhow::anyhow!("test response body mutex was poisoned"))
                    });
                }
            };
            let Some((status, body)) = response else {
                return Box::pin(async {
                    Err(anyhow::anyhow!("test response body was already consumed"))
                });
            };

            Box::pin(async move {
                http_client::Response::builder()
                    .status(status)
                    .body(body)
                    .map_err(anyhow::Error::new)
            })
        }
    }

    struct FailsAfterLimitReader {
        bytes_read: usize,
        limit: usize,
    }

    impl futures::AsyncRead for FailsAfterLimitReader {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _cx: &mut task::Context<'_>,
            buffer: &mut [u8],
        ) -> Poll<io::Result<usize>> {
            if self.bytes_read >= self.limit {
                return Poll::Ready(Err(io::Error::other("read past limit")));
            }

            let byte_count = buffer.len().min(self.limit - self.bytes_read);
            buffer[..byte_count].fill(b'a');
            self.bytes_read += byte_count;
            Poll::Ready(Ok(byte_count))
        }
    }

    // Name and description validation rules are unit-tested in
    // `agent_skills`, which owns `validate_name` / `validate_description`
    // / `MAX_SKILL_DESCRIPTION_LEN`. The tests below cover the skill
    // creator's own surface area: SKILL.md formatting and disk-writing.

    #[test]
    fn format_skill_file_round_trips_through_parser() {
        let content =
            format_skill_file("draft-pr", "Push a draft PR", "Do the thing.", false).unwrap();
        let skill = parse_skill_frontmatter(
            Path::new("/skills/draft-pr/SKILL.md"),
            &content,
            SkillSource::Global,
        )
        .expect("generated frontmatter must round-trip through parse_skill_frontmatter");
        assert_eq!(skill.name, "draft-pr");
        assert_eq!(skill.description, "Push a draft PR");
        assert!(!skill.disable_model_invocation);
    }

    #[test]
    fn format_skill_file_writes_disable_model_invocation_true() {
        let content = format_skill_file("my-skill", "description", "body", true).unwrap();
        assert!(content.contains("disable-model-invocation: true"));
    }

    #[test]
    fn format_skill_file_omits_body_when_empty() {
        let content = format_skill_file("my-skill", "description", "   ", false).unwrap();
        // The trailing closing-delimiter newline is the last byte.
        assert!(content.ends_with("---\n"));
    }

    #[test]
    fn format_skill_file_escapes_yaml_specials_in_description() {
        // serde_yaml_ng must quote/escape descriptions that contain YAML
        // specials so the file round-trips. If we ever swap formatters,
        // this test will catch a regression.
        let tricky = "contains: a colon, # a hash, and a \"quote\"";
        let content = format_skill_file("weird-skill", tricky, "body", false).unwrap();
        let skill = parse_skill_frontmatter(
            Path::new("/skills/weird-skill/SKILL.md"),
            &content,
            SkillSource::Global,
        )
        .expect("YAML-special characters must round-trip");
        assert_eq!(skill.description, tricky);
    }

    #[test]
    fn github_blob_url_converts_to_raw_url() {
        let source_url = "https://github.com/cursor/plugins/blob/3347cbab5b54136f6fba0994c3a01a56f7fb7fca/cursor-team-kit/agents/thermo-nuclear-code-quality-review.md";
        let raw_url = github_raw_url(source_url).expect("GitHub blob URLs should be importable");

        assert_eq!(
            raw_url,
            "https://raw.githubusercontent.com/cursor/plugins/3347cbab5b54136f6fba0994c3a01a56f7fb7fca/cursor-team-kit/agents/thermo-nuclear-code-quality-review.md"
        );
        assert!(is_supported_skill_url(source_url));
        assert!(!is_supported_skill_url(
            "https://example.com/not-a-skill.md"
        ));
    }

    #[test]
    fn derived_skill_name_strips_markdown_extension_case_insensitively() {
        let name = derived_skill_name_from_url(
            "https://raw.githubusercontent.com/owner/repo/main/README.MD",
        )
        .expect("name should be derived from Markdown URL");

        assert_eq!(name, "readme");
    }

    #[test]
    fn parse_imported_skill_reads_frontmatter_and_body() {
        let imported = parse_imported_skill(
            "---\nname: imported-skill\ndescription: Imported from GitHub.\ndisable-model-invocation: true\n---\n\n# Instructions\n\nDo the thing.\n",
            "https://raw.githubusercontent.com/owner/repo/main/imported-skill.md",
        )
        .expect("valid skill frontmatter should parse");

        assert_eq!(imported.name, "imported-skill");
        assert_eq!(imported.description, "Imported from GitHub.");
        assert_eq!(imported.body, "# Instructions\n\nDo the thing.");
        assert!(imported.disable_model_invocation);
    }

    #[test]
    fn parse_imported_skill_falls_back_to_markdown_when_frontmatter_is_missing() {
        let imported = parse_imported_skill(
            "# Code Review\n\nReview code for maintainability.",
            "https://raw.githubusercontent.com/owner/repo/main/code-review.md",
        )
        .expect("plain markdown should still import");

        assert_eq!(imported.name, "code-review");
        assert_eq!(imported.description, "Code Review");
        assert_eq!(
            imported.body,
            "# Code Review\n\nReview code for maintainability."
        );
        assert!(!imported.disable_model_invocation);
    }

    #[test]
    fn parse_imported_skill_reuses_skill_metadata_validation() {
        let error = parse_imported_skill(
            "---\nname: Imported Skill\ndescription: Imported from GitHub.\n---\n\n# Instructions\n",
            "https://raw.githubusercontent.com/owner/repo/main/imported-skill.md",
        )
        .expect_err("invalid skill metadata should be rejected instead of imported");
        let message = error.to_string();

        assert!(
            message.contains("Skill name must contain only lowercase letters"),
            "error should come from shared skill metadata validation, got: {message}"
        );
    }

    #[gpui::test]
    async fn fetch_imported_skill_retries_404_with_github_token(_cx: &mut gpui::TestAppContext) {
        let client = TestHttpClient::new_sequence(vec![
            (404, AsyncBody::from("Not Found")),
            (200, AsyncBody::from("# Imported Skill\n\nDo the thing.")),
        ]);

        let imported = fetch_imported_skill_from_url_with_github_token(
            client.clone(),
            "https://github.com/owner/repo/blob/main/skill.md".to_string(),
            Some("secret-token".to_string()),
        )
        .await
        .expect("private repo fallback should retry with the GitHub token");

        assert_eq!(imported.name, "skill");
        assert_eq!(imported.description, "Imported Skill");
        assert_eq!(
            client.authorization_headers(),
            vec![None, Some("Bearer secret-token".to_string())]
        );
        assert_eq!(
            client.redirect_policies(),
            vec![
                http_client::RedirectPolicy::FollowAll,
                http_client::RedirectPolicy::NoFollow,
            ],
            "the authenticated retry must not follow redirects, so the token \
             can never be forwarded to another host"
        );
    }

    #[gpui::test]
    async fn fetch_imported_skill_rejects_redirect_on_authenticated_request(
        _cx: &mut gpui::TestAppContext,
    ) {
        let client = TestHttpClient::new_sequence(vec![
            (404, AsyncBody::from("Not Found")),
            (302, AsyncBody::from("")),
        ]);

        let error = fetch_imported_skill_from_url_with_github_token(
            client.clone(),
            "https://github.com/owner/repo/blob/main/skill.md".to_string(),
            Some("secret-token".to_string()),
        )
        .await
        .expect_err("a redirect on the authenticated request should be an error");
        let message = error.to_string();

        assert!(
            message.contains("unexpected redirect (302)"),
            "error should report the redirect, got: {message}"
        );
    }

    #[gpui::test]
    async fn fetch_imported_skill_reports_private_or_missing_for_404(
        _cx: &mut gpui::TestAppContext,
    ) {
        let client = TestHttpClient::new_sequence(vec![(404, AsyncBody::from("Not Found"))]);

        let error = fetch_imported_skill_from_url_with_github_token(
            client.clone(),
            "https://github.com/owner/repo/blob/main/skill.md".to_string(),
            None,
        )
        .await
        .expect_err("404 without a GitHub token should fail");
        let message = error.to_string();

        assert!(
            message.contains("no repository exists at this URL, or it is private"),
            "404 error should mention private repositories, got: {message}"
        );
        assert_eq!(client.authorization_headers(), vec![None]);
    }

    #[gpui::test]
    async fn fetch_imported_skill_stops_reading_after_size_limit(_cx: &mut gpui::TestAppContext) {
        let client = TestHttpClient::new(
            200,
            AsyncBody::from_reader(FailsAfterLimitReader {
                bytes_read: 0,
                limit: MAX_SKILL_FILE_SIZE + 1,
            }),
        );

        let error = fetch_imported_skill_from_url(
            client,
            "https://github.com/owner/repo/blob/main/skill.md".to_string(),
        )
        .await
        .expect_err("oversized responses should be rejected");
        let message = error.to_string();

        assert!(
            message.contains("exceeds maximum size"),
            "error should report the skill size limit, got: {message}"
        );
        assert!(
            !message.contains("failed to read response body"),
            "reader should not be polled past the limit, got: {message}"
        );
    }

    #[gpui::test]
    async fn fetch_imported_skill_truncates_error_response_body(_cx: &mut gpui::TestAppContext) {
        let body = format!(
            "{}tail-that-should-not-appear",
            "x".repeat(URL_IMPORT_ERROR_BODY_MAX_LEN + 20)
        );
        let client = TestHttpClient::new(500, AsyncBody::from(body));

        let error = fetch_imported_skill_from_url(
            client,
            "https://github.com/owner/repo/blob/main/skill.md".to_string(),
        )
        .await
        .expect_err("non-success responses should be rejected");
        let message = error.to_string();

        assert!(message.contains("GitHub returned 500"));
        assert!(
            message.ends_with('…'),
            "error body should be visibly truncated, got: {message}"
        );
        assert!(
            !message.contains("tail-that-should-not-appear"),
            "error body should not include the unbounded tail, got: {message}"
        );
    }

    #[gpui::test]
    async fn write_skill_to_disk_creates_directory_and_file(cx: &mut gpui::TestAppContext) {
        let fs = FakeFs::new(cx.executor());
        fs.insert_tree("/skills", serde_json::json!({})).await;

        let path = write_skill_to_disk(
            fs.as_ref(),
            Path::new("/skills"),
            "draft-pr",
            "Push a draft PR",
            "Body of the skill.",
            false,
        )
        .await
        .expect("write should succeed");

        assert_eq!(path, Path::new("/skills/draft-pr/SKILL.md"));
        let content = fs.load(&path).await.expect("file should exist");
        let skill = parse_skill_frontmatter(&path, &content, SkillSource::Global)
            .expect("written file should be parseable");
        assert_eq!(skill.name, "draft-pr");
        assert_eq!(skill.description, "Push a draft PR");
    }

    #[gpui::test]
    async fn write_skill_to_disk_refuses_to_overwrite(cx: &mut gpui::TestAppContext) {
        let fs = FakeFs::new(cx.executor());
        fs.insert_tree(
            "/skills",
            serde_json::json!({
                "draft-pr": {
                    "SKILL.md": "---\nname: draft-pr\ndescription: existing\n---\nbody\n"
                }
            }),
        )
        .await;

        let err = write_skill_to_disk(
            fs.as_ref(),
            Path::new("/skills"),
            "draft-pr",
            "Push a draft PR",
            "Body of the skill.",
            false,
        )
        .await
        .expect_err("writing over an existing skill must fail");
        assert!(
            err.to_string().contains("already exists"),
            "error message should mention the conflict, got: {err}"
        );
    }

    #[gpui::test]
    async fn write_skill_to_disk_rejects_non_directory_at_skill_path(
        cx: &mut gpui::TestAppContext,
    ) {
        let fs = FakeFs::new(cx.executor());
        // A *file* (not a directory) sitting at `/skills/draft-pr`. With the
        // old `is_dir` check this slipped through and we ended up surfacing
        // the underlying "File exists" OS error.
        fs.insert_tree(
            "/skills",
            serde_json::json!({ "draft-pr": "i am a stray file" }),
        )
        .await;

        let err = write_skill_to_disk(
            fs.as_ref(),
            Path::new("/skills"),
            "draft-pr",
            "Push a draft PR",
            "Body of the skill.",
            false,
        )
        .await
        .expect_err("writing where a file already lives must fail");
        let message = err.to_string();
        assert!(
            message.contains("not a skill directory"),
            "error should explain the conflict is a non-directory, got: {message}"
        );
        // Path separator differs between platforms
        let expected_path = Path::new("/skills").join("draft-pr");
        let expected_path = expected_path.display().to_string();
        assert!(
            message.contains(&expected_path),
            "error should include the conflicting path {expected_path:?}, got: {message}"
        );
    }
}
