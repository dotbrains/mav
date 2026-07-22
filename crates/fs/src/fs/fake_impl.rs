use super::*;

#[cfg(feature = "test-support")]
#[async_trait::async_trait]
impl Fs for FakeFs {
    async fn create_dir(&self, path: &Path) -> Result<()> {
        FakeFs::create_dir(self, path).await
    }
    async fn create_file(&self, path: &Path, options: CreateOptions) -> Result<()> {
        FakeFs::create_file(self, path, options).await
    }
    async fn create_symlink(&self, path: &Path, target: PathBuf) -> Result<()> {
        FakeFs::create_symlink(self, path, target).await
    }
    async fn create_file_with(
        &self,
        path: &Path,
        content: Pin<&mut (dyn AsyncRead + Send)>,
    ) -> Result<()> {
        FakeFs::create_file_with(self, path, content).await
    }
    async fn extract_tar_file(
        &self,
        path: &Path,
        content: Archive<Pin<&mut (dyn AsyncRead + Send)>>,
    ) -> Result<()> {
        FakeFs::extract_tar_file(self, path, content).await
    }
    async fn rename(&self, old_path: &Path, new_path: &Path, options: RenameOptions) -> Result<()> {
        FakeFs::rename(self, old_path, new_path, options).await
    }
    async fn copy_file(&self, source: &Path, target: &Path, options: CopyOptions) -> Result<()> {
        FakeFs::copy_file(self, source, target, options).await
    }
    async fn remove_dir(&self, path: &Path, options: RemoveOptions) -> Result<()> {
        FakeFs::remove_dir(self, path, options).await
    }
    async fn trash(&self, path: &Path, options: RemoveOptions) -> Result<TrashedEntry> {
        FakeFs::trash(self, path, options).await
    }
    async fn remove_file(&self, path: &Path, options: RemoveOptions) -> Result<()> {
        FakeFs::remove_file(self, path, options).await
    }
    async fn open_sync(&self, path: &Path) -> Result<Box<dyn io::Read + Send + Sync>> {
        FakeFs::open_sync(self, path).await
    }
    async fn open_handle(&self, path: &Path) -> Result<Arc<dyn FileHandle>> {
        FakeFs::open_handle(self, path).await
    }
    async fn load(&self, path: &Path) -> Result<String> {
        FakeFs::load(self, path).await
    }
    async fn load_bytes(&self, path: &Path) -> Result<Vec<u8>> {
        FakeFs::load_bytes(self, path).await
    }
    async fn atomic_write(&self, path: PathBuf, data: String) -> Result<()> {
        FakeFs::atomic_write(self, path, data).await
    }
    async fn save(&self, path: &Path, text: &Rope, line_ending: LineEnding) -> Result<()> {
        FakeFs::save(self, path, text, line_ending).await
    }
    async fn write(&self, path: &Path, content: &[u8]) -> Result<()> {
        FakeFs::write(self, path, content).await
    }
    async fn canonicalize(&self, path: &Path) -> Result<PathBuf> {
        FakeFs::canonicalize(self, path).await
    }
    async fn is_file(&self, path: &Path) -> bool {
        FakeFs::is_file(self, path).await
    }
    async fn is_dir(&self, path: &Path) -> bool {
        FakeFs::is_dir(self, path).await
    }
    async fn metadata(&self, path: &Path) -> Result<Option<Metadata>> {
        FakeFs::metadata(self, path).await
    }
    async fn read_link(&self, path: &Path) -> Result<PathBuf> {
        FakeFs::read_link(self, path).await
    }
    async fn read_dir(
        &self,
        path: &Path,
    ) -> Result<Pin<Box<dyn Send + Stream<Item = Result<PathBuf>>>>> {
        FakeFs::read_dir(self, path).await
    }
    async fn watch(
        &self,
        path: &Path,
        latency: Duration,
    ) -> (
        Pin<Box<dyn Send + Stream<Item = Vec<PathEvent>>>>,
        Arc<dyn Watcher>,
    ) {
        FakeFs::watch(self, path, latency).await
    }
    fn open_repo(
        &self,
        abs_dot_git: &Path,
        system_git_binary: Option<&Path>,
    ) -> Result<Arc<dyn GitRepository>> {
        FakeFs::open_repo(self, abs_dot_git, system_git_binary)
    }
    async fn git_init(
        &self,
        abs_work_directory_path: &Path,
        fallback_branch_name: String,
    ) -> Result<()> {
        FakeFs::git_init(self, abs_work_directory_path, fallback_branch_name).await
    }
    async fn git_clone(&self, abs_work_directory: &Path, repo_url: &str) -> Result<()> {
        FakeFs::git_clone(self, abs_work_directory, repo_url).await
    }
    async fn git_config(&self, abs_work_directory: &Path, args: Vec<String>) -> Result<String> {
        FakeFs::git_config(self, abs_work_directory, args).await
    }
    fn is_fake(&self) -> bool {
        true
    }
    async fn is_case_sensitive(&self) -> bool {
        true
    }
    fn subscribe_to_jobs(&self) -> JobEventReceiver {
        FakeFs::subscribe_to_jobs(self)
    }
    async fn restore(&self, trashed_entry: TrashedEntry) -> Result<PathBuf, TrashRestoreError> {
        FakeFs::restore(self, trashed_entry).await
    }
    fn as_fake(&self) -> Arc<FakeFs> {
        FakeFs::as_fake(self)
    }
}
