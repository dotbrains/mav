use super::*;

/// Modifies .git/info/exclude temporarily
pub struct GitExcludeOverride {
    git_exclude_path: PathBuf,
    original_excludes: Option<String>,
    added_excludes: Option<String>,
}

impl GitExcludeOverride {
    const START_BLOCK_MARKER: &str = "\n\n#  ====== Auto-added by Mav: =======\n";
    const END_BLOCK_MARKER: &str = "\n#  ====== End of auto-added by Mav =======\n";

    pub async fn new(git_exclude_path: PathBuf) -> Result<Self> {
        let original_excludes =
            smol::fs::read_to_string(&git_exclude_path)
                .await
                .ok()
                .map(|content| {
                    // Auto-generated lines are normally cleaned up in
                    // `restore_original()` or `drop()`, but may stuck in rare cases.
                    // Make sure to remove them.
                    Self::remove_auto_generated_block(&content)
                });

        Ok(GitExcludeOverride {
            git_exclude_path,
            original_excludes,
            added_excludes: None,
        })
    }

    pub async fn add_excludes(&mut self, excludes: &str) -> Result<()> {
        self.added_excludes = Some(if let Some(ref already_added) = self.added_excludes {
            format!("{already_added}\n{excludes}")
        } else {
            excludes.to_string()
        });

        let mut content = self.original_excludes.clone().unwrap_or_default();

        content.push_str(Self::START_BLOCK_MARKER);
        content.push_str(self.added_excludes.as_ref().unwrap());
        content.push_str(Self::END_BLOCK_MARKER);

        smol::fs::write(&self.git_exclude_path, content).await?;
        Ok(())
    }

    pub async fn restore_original(&mut self) -> Result<()> {
        if let Some(ref original) = self.original_excludes {
            smol::fs::write(&self.git_exclude_path, original).await?;
        } else if self.git_exclude_path.exists() {
            smol::fs::remove_file(&self.git_exclude_path).await?;
        }

        self.added_excludes = None;

        Ok(())
    }

    fn remove_auto_generated_block(content: &str) -> String {
        let start_marker = Self::START_BLOCK_MARKER;
        let end_marker = Self::END_BLOCK_MARKER;
        let mut content = content.to_string();

        let start_index = content.find(start_marker);
        let end_index = content.rfind(end_marker);

        if let (Some(start), Some(end)) = (start_index, end_index) {
            if end > start {
                content.replace_range(start..end + end_marker.len(), "");
            }
        }

        // Older versions of Mav didn't have end-of-block markers,
        // so it's impossible to determine auto-generated lines.
        // Conservatively remove the standard list of excludes
        let standard_excludes = format!(
            "{}{}",
            Self::START_BLOCK_MARKER,
            include_str!("../checkpoint.gitignore")
        );
        content = content.replace(&standard_excludes, "");

        content
    }
}

impl Drop for GitExcludeOverride {
    fn drop(&mut self) {
        if self.added_excludes.is_some() {
            let git_exclude_path = self.git_exclude_path.clone();
            let original_excludes = self.original_excludes.clone();
            smol::spawn(async move {
                if let Some(original) = original_excludes {
                    smol::fs::write(&git_exclude_path, original).await
                } else {
                    smol::fs::remove_file(&git_exclude_path).await
                }
            })
            .detach();
        }
    }
}
