use super::*;

// todo(windows)
// can we get file id not open the file twice?
// https://github.com/rust-lang/rust/issues/63010
#[cfg(target_os = "windows")]
async fn file_id(path: impl AsRef<Path>) -> Result<u64> {
    use std::os::windows::io::AsRawHandle;

    use smol::fs::windows::OpenOptionsExt;
    use windows::Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS, GetFileInformationByHandle,
        },
    };

    let file = smol::fs::OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0)
        .open(path)
        .await?;

    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    // https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getfileinformationbyhandle
    // This function supports Windows XP+
    smol::unblock(move || {
        unsafe { GetFileInformationByHandle(HANDLE(file.as_raw_handle() as _), &mut info)? };

        Ok(((info.nFileIndexHigh as u64) << 32) | (info.nFileIndexLow as u64))
    })
    .await
}

#[cfg(target_os = "windows")]
fn atomic_replace<P: AsRef<Path>>(
    replaced_file: P,
    replacement_file: P,
) -> windows::core::Result<()> {
    use windows::{
        Win32::Storage::FileSystem::{REPLACE_FILE_FLAGS, ReplaceFileW},
        core::HSTRING,
    };

    // If the file does not exist, create it.
    let _ = std::fs::File::create_new(replaced_file.as_ref());

    unsafe {
        ReplaceFileW(
            &HSTRING::from(replaced_file.as_ref().to_string_lossy().into_owned()),
            &HSTRING::from(replacement_file.as_ref().to_string_lossy().into_owned()),
            None,
            REPLACE_FILE_FLAGS::default(),
            None,
            None,
        )
    }
}
