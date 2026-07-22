use super::*;

pub(crate) async fn read_oauth_token(
    fs: &Arc<dyn Fs>,
    config_paths: &HashSet<PathBuf>,
    oauth_domain: &str,
    auth_db_path: &std::path::Path,
    cx: &AsyncApp,
) -> Option<String> {
    if let Some(token) = oauth_token_from_env() {
        return Some(token);
    }

    let token_from_db = cx
        .background_spawn({
            let auth_db_path = auth_db_path.to_path_buf();
            let oauth_domain = oauth_domain.to_string();
            async move { extract_oauth_token_from_db(&auth_db_path, &oauth_domain) }
        })
        .await;

    if let Some(token) = token_from_db {
        return Some(token);
    }

    for file_path in config_paths {
        if let Ok(contents) = fs.load(file_path).await {
            if let Some(token) = extract_oauth_token(contents, oauth_domain) {
                return Some(token);
            }
        }
    }

    None
}

fn extract_oauth_token(contents: String, domain: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(&contents)
        .map(|v| {
            v.as_object().and_then(|obj| {
                obj.iter().find_map(|(key, value)| {
                    if key.starts_with(domain) {
                        value["oauth_token"].as_str().map(|v| v.to_string())
                    } else {
                        None
                    }
                })
            })
        })
        .ok()
        .flatten()
}

pub(crate) fn extract_oauth_token_from_db(db_path: &Path, auth_authority: &str) -> Option<String> {
    if !db_path.exists() {
        return None;
    }

    let db = sqlez::connection::Connection::open_file(db_path.to_str()?);

    let token_bytes: Option<Vec<u8>> = db
    .select_row_bound::<&str, Vec<u8>>(
        "SELECT token_ciphertext FROM oauth_tokens WHERE auth_authority = ? ORDER BY last_used_at DESC, token_id DESC LIMIT 1",
    )
    .ok()
    .and_then(|mut select| select(auth_authority).ok().flatten());

    let token = token_bytes.and_then(|bytes| String::from_utf8(bytes).ok())?;

    if token.starts_with("ghu_")
        && token.len() >= 36
        && token.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        log::debug!("Copilot OAuth token loaded from auth.db");
        Some(token)
    } else {
        log::warn!(
            "Copilot auth.db: token does not match expected GitHub OAuth format (ghu_<alphanumeric>)"
        );
        None
    }
}
