use mav::settings::LspSettings;
use mav_extension_api::{self as mav, LanguageServerId, Result, serde_json};
use std::fs;

struct GlslExtension {
    cached_binary_path: Option<String>,
}

impl GlslExtension {
    fn language_server_binary_path(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &mav::Worktree,
    ) -> Result<String> {
        if let Some(path) = worktree.which("glsl_analyzer") {
            return Ok(path);
        }

        if let Some(path) = &self.cached_binary_path
            && fs::metadata(path).is_ok_and(|stat| stat.is_file())
        {
            return Ok(path.clone());
        }

        mav::set_language_server_installation_status(
            language_server_id,
            &mav::LanguageServerInstallationStatus::CheckingForUpdate,
        );
        let release = mav::latest_github_release(
            "nolanderc/glsl_analyzer",
            mav::GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        )?;

        let (platform, arch) = mav::current_platform();
        let asset_name = format!(
            "{arch}-{os}.zip",
            arch = match arch {
                mav::Architecture::Aarch64 => "aarch64",
                mav::Architecture::X86 => "x86",
                mav::Architecture::X8664 => "x86_64",
            },
            os = match platform {
                mav::Os::Mac => "macos",
                mav::Os::Linux => "linux-musl",
                mav::Os::Windows => "windows",
            }
        );

        let asset = release
            .assets
            .iter()
            .find(|asset| asset.name == asset_name)
            .ok_or_else(|| format!("no asset found matching {:?}", asset_name))?;

        let version_dir = format!("glsl_analyzer-{}", release.version);
        fs::create_dir_all(&version_dir)
            .map_err(|err| format!("failed to create directory '{version_dir}': {err}"))?;
        let binary_path = format!("{version_dir}/bin/glsl_analyzer");

        if !fs::metadata(&binary_path).is_ok_and(|stat| stat.is_file()) {
            mav::set_language_server_installation_status(
                language_server_id,
                &mav::LanguageServerInstallationStatus::Downloading,
            );

            mav::download_file(
                &asset.download_url,
                &version_dir,
                match platform {
                    mav::Os::Mac | mav::Os::Linux => mav::DownloadedFileType::Zip,
                    mav::Os::Windows => mav::DownloadedFileType::Zip,
                },
            )
            .map_err(|e| format!("failed to download file: {e}"))?;

            mav::make_file_executable(&binary_path)?;

            let entries =
                fs::read_dir(".").map_err(|e| format!("failed to list working directory {e}"))?;
            for entry in entries {
                let entry = entry.map_err(|e| format!("failed to load directory entry {e}"))?;
                if entry.file_name().to_str() != Some(&version_dir) {
                    fs::remove_dir_all(entry.path()).ok();
                }
            }
        }

        self.cached_binary_path = Some(binary_path.clone());
        Ok(binary_path)
    }
}

impl mav::Extension for GlslExtension {
    fn new() -> Self {
        Self {
            cached_binary_path: None,
        }
    }

    fn language_server_command(
        &mut self,
        language_server_id: &mav::LanguageServerId,
        worktree: &mav::Worktree,
    ) -> Result<mav::Command> {
        Ok(mav::Command {
            command: self.language_server_binary_path(language_server_id, worktree)?,
            args: vec![],
            env: Default::default(),
        })
    }

    fn language_server_workspace_configuration(
        &mut self,
        _language_server_id: &mav::LanguageServerId,
        worktree: &mav::Worktree,
    ) -> Result<Option<serde_json::Value>> {
        let settings = LspSettings::for_worktree("glsl_analyzer", worktree)
            .ok()
            .and_then(|lsp_settings| lsp_settings.settings)
            .unwrap_or_default();

        Ok(Some(serde_json::json!({
            "glsl_analyzer": settings
        })))
    }
}

mav::register_extension!(GlslExtension);
