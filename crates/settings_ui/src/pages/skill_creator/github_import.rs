use super::*;

async fn fetch_imported_skill_from_url(
    http_client: Arc<dyn HttpClient>,
    url: String,
) -> Result<ImportedSkill> {
    let github_token = std::env::var("GITHUB_TOKEN").ok().and_then(|token| {
        let token = token.trim().to_string();
        (!token.is_empty()).then_some(token)
    });
    fetch_imported_skill_from_url_with_github_token(http_client, url, github_token).await
}

async fn fetch_imported_skill_from_url_with_github_token(
    http_client: Arc<dyn HttpClient>,
    url: String,
    github_token: Option<String>,
) -> Result<ImportedSkill> {
    let raw_url = github_raw_url(&url)?;
    let (mut status, mut body) =
        fetch_skill_url(http_client.clone(), raw_url.as_str(), None).await?;

    if status == StatusCode::NOT_FOUND
        && let Some(github_token) = github_token.as_deref()
    {
        (status, body) = fetch_skill_url(http_client, raw_url.as_str(), Some(github_token)).await?;
    }

    if !status.is_success() {
        return Err(github_fetch_error(status, &body));
    }

    if body.len() > MAX_SKILL_FILE_SIZE {
        anyhow::bail!(
            "SKILL.md file exceeds maximum size of {}KB",
            MAX_SKILL_FILE_SIZE / 1024
        );
    }

    let content = String::from_utf8(body).context("GitHub response was not valid UTF-8")?;
    parse_imported_skill(&content, raw_url.as_str())
}

async fn fetch_skill_url(
    http_client: Arc<dyn HttpClient>,
    raw_url: &str,
    github_token: Option<&str>,
) -> Result<(StatusCode, Vec<u8>)> {
    // When sending the GitHub token, don't follow redirects: whether an
    // `Authorization` header survives a cross-origin redirect depends on the
    // underlying `HttpClient` implementation, and a redirect away from
    // raw.githubusercontent.com must never carry the user's token with it.
    // Authenticated raw.githubusercontent.com responses are served directly,
    // so a redirect on that path is unexpected anyway.
    let redirect_policy = if github_token.is_some() {
        http_client::RedirectPolicy::NoFollow
    } else {
        http_client::RedirectPolicy::FollowAll
    };
    let request = Request::get(raw_url)
        .follow_redirects(redirect_policy)
        .when_some(github_token, |builder, token| {
            builder.header("Authorization", format!("Bearer {token}"))
        })
        .body(AsyncBody::default())?;

    let mut response = http_client
        .send(request)
        .await
        .with_context(|| format!("failed to fetch {raw_url}"))?;

    let status = response.status();
    if github_token.is_some() && status.is_redirection() {
        anyhow::bail!(
            "GitHub returned an unexpected redirect ({}) for the authenticated request to {raw_url}",
            status.as_u16()
        );
    }
    let mut body = Vec::new();
    response
        .body_mut()
        .take(MAX_SKILL_FILE_SIZE as u64 + 1)
        .read_to_end(&mut body)
        .await
        .context("failed to read response body")?;

    Ok((status, body))
}

fn github_fetch_error(status: StatusCode, body: &[u8]) -> anyhow::Error {
    let mut message = if status == StatusCode::NOT_FOUND {
        "GitHub returned 404 while fetching the skill; no repository exists at this URL, or it is private"
            .to_string()
    } else {
        format!(
            "GitHub returned {} while fetching the skill",
            status.as_u16()
        )
    };

    let response_text = truncated_response_body_for_error(body);
    if !response_text.is_empty() {
        message.push_str(": ");
        message.push_str(&response_text);
    }

    anyhow!(message)
}

pub(crate) fn is_supported_skill_url(input: &str) -> bool {
    github_raw_url(input).is_ok()
}

fn github_raw_url(input: &str) -> Result<String> {
    let url = Url::parse(input.trim()).context("Enter a valid GitHub URL")?;
    if url.scheme() != "https" {
        anyhow::bail!("GitHub skill URLs must use https://");
    }

    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("Enter a valid GitHub URL"))?;
    let path_segments = url
        .path_segments()
        .ok_or_else(|| anyhow!("Enter a valid GitHub URL"))?
        .collect::<Vec<_>>();

    match host {
        "github.com" => github_blob_raw_url(&path_segments),
        "raw.githubusercontent.com" => {
            ensure_markdown_path(&path_segments)?;
            Ok(url.into())
        }
        _ => anyhow::bail!("Paste a GitHub .md URL"),
    }
}

fn github_blob_raw_url(path_segments: &[&str]) -> Result<String> {
    let [owner, repo, kind, reference, file_path @ ..] = path_segments else {
        anyhow::bail!("Paste a GitHub blob URL that points to a .md file");
    };

    if *kind != "blob" {
        anyhow::bail!("Paste a GitHub blob URL that points to a .md file");
    }

    ensure_markdown_path(file_path)?;
    Ok(format!(
        "https://raw.githubusercontent.com/{owner}/{repo}/{reference}/{}",
        file_path.join("/")
    ))
}

fn ensure_markdown_path(path_segments: &[&str]) -> Result<()> {
    let Some(file_name) = path_segments.last() else {
        anyhow::bail!("Paste a GitHub .md URL");
    };

    if !file_name.to_ascii_lowercase().ends_with(".md") {
        anyhow::bail!("Paste a GitHub URL that points to a .md file");
    }

    Ok(())
}

fn parse_imported_skill(content: &str, source_url: &str) -> Result<ImportedSkill> {
    if content.trim_start().starts_with("---") {
        let (metadata, body) = parse_skill_file_content(content)?;
        return Ok(ImportedSkill {
            name: metadata.name,
            description: metadata.description,
            body: body.trim().to_string(),
            disable_model_invocation: metadata.disable_model_invocation,
        });
    }

    Ok(ImportedSkill {
        name: derived_skill_name_from_url(source_url).unwrap_or_else(|| "imported-skill".into()),
        description: derived_description_from_markdown(content).unwrap_or_default(),
        body: content.trim().to_string(),
        disable_model_invocation: false,
    })
}

fn derived_skill_name_from_url(source_url: &str) -> Option<String> {
    let url = Url::parse(source_url).ok()?;
    let file_name = url.path_segments()?.next_back()?;
    let stem = file_name
        .rsplit_once('.')
        .and_then(|(stem, extension)| extension.eq_ignore_ascii_case("md").then_some(stem))
        .unwrap_or(file_name);
    slugify_skill_name(stem)
}

fn truncated_response_body_for_error(body: &[u8]) -> String {
    let text = String::from_utf8_lossy(body);
    let text = text.trim();
    if text.len() <= URL_IMPORT_ERROR_BODY_MAX_LEN {
        return text.to_string();
    }

    let mut end = URL_IMPORT_ERROR_BODY_MAX_LEN;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", text[..end].trim_end())
}

fn derived_description_from_markdown(content: &str) -> Option<String> {
    content.lines().find_map(|line| {
        let line = line.trim();
        if line.is_empty() || line == "---" {
            return None;
        }

        let text = line.trim_start_matches('#').trim();
        if text.is_empty() {
            None
        } else {
            Some(truncate_description(text))
        }
    })
}

fn truncate_description(description: &str) -> String {
    if description.len() <= MAX_SKILL_DESCRIPTION_LEN {
        return description.to_string();
    }

    let mut end = MAX_SKILL_DESCRIPTION_LEN;
    while !description.is_char_boundary(end) {
        end -= 1;
    }
    description[..end].trim().to_string()
}
