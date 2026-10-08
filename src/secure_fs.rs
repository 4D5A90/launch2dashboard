//! File access that never follows symlinks out of the L2D directories.
use crate::domain::{AppError, ErrorKind};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom},
    os::unix::fs::OpenOptionsExt,
    path::Path,
};
pub(crate) fn safe_directory(path: &Path) -> Result<(), AppError> {
    for ancestor in path.ancestors() {
        if let Ok(m) = fs::symlink_metadata(ancestor)
            && m.file_type().is_symlink()
        {
            return Err(AppError::new(
                ErrorKind::Validation,
                format!(
                    "Symlink directories are not supported: {}",
                    ancestor.display()
                ),
            ));
        }
    }
    fs::create_dir_all(path)?;
    if !path.is_dir() {
        return Err(AppError::new(ErrorKind::Io, "Expected a directory"));
    }
    Ok(())
}
pub(crate) fn safe_regular(path: &Path, missing_ok: bool) -> Result<(), AppError> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_file() && !m.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(AppError::new(
            ErrorKind::Validation,
            format!("Refusing non-regular file: {}", path.display()),
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && missing_ok => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Err(AppError::new(ErrorKind::NotFound, "Service not found"))
        }
        Err(e) => Err(e.into()),
    }
}
/// O_NOFOLLOW, whose value differs between Darwin and Linux.
fn no_follow() -> i32 {
    #[cfg(target_os = "macos")]
    {
        0x100
    }
    #[cfg(not(target_os = "macos"))]
    {
        0x20000
    }
}
pub(crate) fn open_read(path: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .read(true)
        .custom_flags(no_follow())
        .open(path)
}
/// Creates `path` as a private (0600) regular file if missing.
pub(crate) fn touch_private(path: &Path) -> Result<(), AppError> {
    safe_regular(path, true)?;
    OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .custom_flags(no_follow())
        .open(path)?;
    Ok(())
}
/// Atomically replaces `path` with a private file filled by `fill`, via `temp` in the same directory.
pub(crate) fn replace_private(
    path: &Path,
    temp: &Path,
    fill: impl FnOnce(&mut File) -> Result<(), AppError>,
) -> Result<(), AppError> {
    safe_regular(path, true)?;
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(temp)?;
        fill(&mut file)?;
        file.sync_all()?;
        fs::rename(temp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}
/// Returns at most the last `max_bytes` of `path`, or an empty string if it does not exist.
pub(crate) fn tail(path: &Path, max_bytes: u64) -> Result<String, AppError> {
    safe_regular(path, true)?;
    let mut f = match open_read(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
        Err(e) => return Err(e.into()),
    };
    let size = f.metadata()?.len();
    f.seek(SeekFrom::Start(size.saturating_sub(max_bytes)))?;
    let mut bytes = Vec::new();
    f.take(max_bytes).read_to_end(&mut bytes)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}
