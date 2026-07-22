use super::*;

pub struct RealFs {
    pub(super) bundled_git_binary_path: Option<PathBuf>,
    pub(super) executor: BackgroundExecutor,
    pub(super) next_job_id: Arc<AtomicUsize>,
    pub(super) job_event_subscribers: Arc<Mutex<Vec<JobEventSender>>>,
    pub(super) is_case_sensitive: AtomicU8,
}

pub trait FileHandle: Send + Sync + std::fmt::Debug {
    fn current_path(&self, fs: &Arc<dyn Fs>) -> Result<PathBuf>;
}

impl FileHandle for std::fs::File {
    #[cfg(target_os = "macos")]
    fn current_path(&self, _: &Arc<dyn Fs>) -> Result<PathBuf> {
        use std::{
            ffi::{CStr, OsStr},
            os::unix::ffi::OsStrExt,
        };

        let fd = self.as_fd();
        let mut path_buf = MaybeUninit::<[u8; libc::PATH_MAX as usize]>::uninit();

        let result = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETPATH, path_buf.as_mut_ptr()) };
        anyhow::ensure!(result != -1, "fcntl returned -1");

        // SAFETY: `fcntl` will initialize the path buffer.
        let c_str = unsafe { CStr::from_ptr(path_buf.as_ptr().cast()) };
        anyhow::ensure!(!c_str.is_empty(), "Could find a path for the file handle");
        let path = PathBuf::from(OsStr::from_bytes(c_str.to_bytes()));
        Ok(path)
    }

    #[cfg(target_os = "linux")]
    fn current_path(&self, _: &Arc<dyn Fs>) -> Result<PathBuf> {
        let fd = self.as_fd();
        let fd_path = format!("/proc/self/fd/{}", fd.as_raw_fd());
        let new_path = std::fs::read_link(fd_path)?;
        if new_path
            .file_name()
            .is_some_and(|f| f.to_string_lossy().ends_with(" (deleted)"))
        {
            anyhow::bail!("file was deleted")
        };

        Ok(new_path)
    }

    #[cfg(target_os = "freebsd")]
    fn current_path(&self, _: &Arc<dyn Fs>) -> Result<PathBuf> {
        use std::{
            ffi::{CStr, OsStr},
            os::unix::ffi::OsStrExt,
        };

        let fd = self.as_fd();
        let mut kif = MaybeUninit::<libc::kinfo_file>::uninit();
        kif.kf_structsize = libc::KINFO_FILE_SIZE;

        let result = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_KINFO, kif.as_mut_ptr()) };
        anyhow::ensure!(result != -1, "fcntl returned -1");

        // SAFETY: `fcntl` will initialize the kif.
        let c_str = unsafe { CStr::from_ptr(kif.assume_init().kf_path.as_ptr()) };
        anyhow::ensure!(!c_str.is_empty(), "Could find a path for the file handle");
        let path = PathBuf::from(OsStr::from_bytes(c_str.to_bytes()));
        Ok(path)
    }

    #[cfg(target_os = "windows")]
    fn current_path(&self, _: &Arc<dyn Fs>) -> Result<PathBuf> {
        use std::ffi::OsString;
        use std::os::windows::ffi::OsStringExt;
        use std::os::windows::io::AsRawHandle;

        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::Storage::FileSystem::{
            FILE_NAME_NORMALIMAV, GetFinalPathNameByHandleW,
        };

        let handle = HANDLE(self.as_raw_handle() as _);

        // Query required buffer size (in wide chars)
        let required_len =
            unsafe { GetFinalPathNameByHandleW(handle, &mut [], FILE_NAME_NORMALIMAV) };
        anyhow::ensure!(
            required_len != 0,
            "GetFinalPathNameByHandleW returned 0 length"
        );

        // Allocate buffer and retrieve the path
        let mut buf: Vec<u16> = vec![0u16; required_len as usize + 1];
        let written = unsafe { GetFinalPathNameByHandleW(handle, &mut buf, FILE_NAME_NORMALIMAV) };
        anyhow::ensure!(
            written != 0,
            "GetFinalPathNameByHandleW failed to write path"
        );

        let os_str: OsString = OsString::from_wide(&buf[..written as usize]);
        anyhow::ensure!(!os_str.is_empty(), "Could find a path for the file handle");
        Ok(PathBuf::from(os_str))
    }
}

pub struct RealWatcher {}

impl RealFs {
    pub fn new(git_binary_path: Option<PathBuf>, executor: BackgroundExecutor) -> Self {
        Self {
            bundled_git_binary_path: git_binary_path,
            executor,
            next_job_id: Arc::new(AtomicUsize::new(0)),
            job_event_subscribers: Arc::new(Mutex::new(Vec::new())),
            is_case_sensitive: Default::default(),
        }
    }

    #[cfg(target_os = "windows")]
    fn canonicalize(path: &Path) -> Result<PathBuf> {
        use std::ffi::OsString;
        use std::os::windows::ffi::OsStringExt;
        use windows::Win32::Storage::FileSystem::GetVolumePathNameW;
        use windows::core::HSTRING;

        // std::fs::canonicalize resolves mapped network paths to UNC paths, which can
        // confuse some software. To mitigate this, we canonicalize the input, then rebase
        // the result onto the input's original volume root if both paths are on the same
        // volume. This keeps the same drive letter or mount point the caller used.

        let abs_path = if path.is_relative() {
            std::env::current_dir()?.join(path)
        } else {
            path.to_path_buf()
        };

        let path_hstring = HSTRING::from(abs_path.as_os_str());
        let mut vol_buf = vec![0u16; abs_path.as_os_str().len() + 2];
        unsafe { GetVolumePathNameW(&path_hstring, &mut vol_buf)? };
        let volume_root = {
            let len = vol_buf
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(vol_buf.len());
            PathBuf::from(OsString::from_wide(&vol_buf[..len]))
        };

        let resolved_path = dunce::canonicalize(&abs_path)?;
        let resolved_root = dunce::canonicalize(&volume_root)?;

        if let Ok(relative) = resolved_path.strip_prefix(&resolved_root) {
            let mut result = volume_root;
            result.push(relative);
            Ok(result)
        } else {
            Ok(resolved_path)
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(super) fn rename_without_replace(source: &Path, target: &Path) -> io::Result<()> {
    let source = path_to_c_string(source)?;
    let target = path_to_c_string(target)?;

    #[cfg(target_os = "macos")]
    let result = unsafe { libc::renamex_np(source.as_ptr(), target.as_ptr(), libc::RENAME_EXCL) };

    #[cfg(target_os = "linux")]
    let result = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            target.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };

    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(target_os = "windows")]
pub(super) fn rename_without_replace(source: &Path, target: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    use windows::Win32::Storage::FileSystem::{MOVE_FILE_FLAGS, MoveFileExW};
    use windows::core::PCWSTR;

    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let target: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();

    unsafe {
        MoveFileExW(
            PCWSTR(source.as_ptr()),
            PCWSTR(target.as_ptr()),
            MOVE_FILE_FLAGS::default(),
        )
    }
    .map_err(|_| io::Error::last_os_error())
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(super) fn path_to_c_string(path: &Path) -> io::Result<CString> {
    CString::new(path.as_os_str().as_bytes()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("path contains interior NUL: {}", path.display()),
        )
    })
}

#[async_trait::async_trait]
impl Fs for RealFs {
    async fn create_dir(&self, path: &Path) -> Result<()> {
        RealFs::create_dir(self, path).await
    }
    async fn create_symlink(&self, path: &Path, target: PathBuf) -> Result<()> {
        RealFs::create_symlink(self, path, target).await
    }
    async fn create_file(&self, path: &Path, options: CreateOptions) -> Result<()> {
        RealFs::create_file(self, path, options).await
    }
    async fn create_file_with(
        &self,
        path: &Path,
        content: Pin<&mut (dyn AsyncRead + Send)>,
    ) -> Result<()> {
        RealFs::create_file_with(self, path, content).await
    }
    async fn extract_tar_file(
        &self,
        path: &Path,
        content: Archive<Pin<&mut (dyn AsyncRead + Send)>>,
    ) -> Result<()> {
        RealFs::extract_tar_file(self, path, content).await
    }
    async fn copy_file(&self, source: &Path, target: &Path, options: CopyOptions) -> Result<()> {
        RealFs::copy_file(self, source, target, options).await
    }
    async fn rename(&self, source: &Path, target: &Path, options: RenameOptions) -> Result<()> {
        RealFs::rename(self, source, target, options).await
    }
    async fn remove_dir(&self, path: &Path, options: RemoveOptions) -> Result<()> {
        RealFs::remove_dir(self, path, options).await
    }
    async fn trash(&self, path: &Path, options: RemoveOptions) -> Result<TrashedEntry> {
        RealFs::trash(self, path, options).await
    }
    async fn remove_file(&self, path: &Path, options: RemoveOptions) -> Result<()> {
        RealFs::remove_file(self, path, options).await
    }
    async fn open_handle(&self, path: &Path) -> Result<Arc<dyn FileHandle>> {
        RealFs::open_handle(self, path).await
    }
    async fn open_sync(&self, path: &Path) -> Result<Box<dyn io::Read + Send + Sync>> {
        RealFs::open_sync(self, path).await
    }
    async fn load(&self, path: &Path) -> Result<String> {
        RealFs::load(self, path).await
    }
    async fn load_bytes(&self, path: &Path) -> Result<Vec<u8>> {
        RealFs::load_bytes(self, path).await
    }
    async fn atomic_write(&self, path: PathBuf, text: String) -> Result<()> {
        RealFs::atomic_write(self, path, text).await
    }
    async fn save(&self, path: &Path, text: &Rope, line_ending: LineEnding) -> Result<()> {
        RealFs::save(self, path, text, line_ending).await
    }
    async fn write(&self, path: &Path, content: &[u8]) -> Result<()> {
        RealFs::write(self, path, content).await
    }
    async fn canonicalize(&self, path: &Path) -> Result<PathBuf> {
        RealFs::canonicalize(self, path).await
    }
    async fn is_file(&self, path: &Path) -> bool {
        RealFs::is_file(self, path).await
    }
    async fn is_dir(&self, path: &Path) -> bool {
        RealFs::is_dir(self, path).await
    }
    async fn metadata(&self, path: &Path) -> Result<Option<Metadata>> {
        RealFs::metadata(self, path).await
    }
    async fn read_link(&self, path: &Path) -> Result<PathBuf> {
        RealFs::read_link(self, path).await
    }
    async fn read_dir(
        &self,
        path: &Path,
    ) -> Result<Pin<Box<dyn Send + Stream<Item = Result<PathBuf>>>>> {
        RealFs::read_dir(self, path).await
    }
    async fn watch(
        &self,
        path: &Path,
        latency: Duration,
    ) -> (
        Pin<Box<dyn Send + Stream<Item = Vec<PathEvent>>>>,
        Arc<dyn Watcher>,
    ) {
        RealFs::watch(self, path, latency).await
    }
    fn open_repo(
        &self,
        abs_dot_git: &Path,
        system_git_binary_path: Option<&Path>,
    ) -> Result<Arc<dyn GitRepository>> {
        RealFs::open_repo(self, abs_dot_git, system_git_binary_path)
    }
    async fn git_init(
        &self,
        abs_work_directory: &Path,
        fallback_branch_name: String,
    ) -> Result<()> {
        RealFs::git_init(self, abs_work_directory, fallback_branch_name).await
    }
    async fn git_clone(&self, abs_work_directory: &Path, repo_url: &str) -> Result<()> {
        RealFs::git_clone(self, abs_work_directory, repo_url).await
    }
    async fn git_config(&self, abs_work_directory: &Path, args: Vec<String>) -> Result<String> {
        RealFs::git_config(self, abs_work_directory, args).await
    }
    fn is_fake(&self) -> bool {
        false
    }
    async fn is_case_sensitive(&self) -> bool {
        RealFs::is_case_sensitive(self).await
    }
    fn subscribe_to_jobs(&self) -> JobEventReceiver {
        RealFs::subscribe_to_jobs(self)
    }
    async fn restore(
        &self,
        trashed_entry: TrashedEntry,
    ) -> std::result::Result<PathBuf, TrashRestoreError> {
        RealFs::restore(self, trashed_entry).await
    }
}

#[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
impl Watcher for RealWatcher {
    fn add(&self, _: &Path) -> Result<()> {
        Ok(())
    }

    fn remove(&self, _: &Path) -> Result<()> {
        Ok(())
    }
}
