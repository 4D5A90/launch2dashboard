use crate::domain::{AppError, Icon, IconFormat, IconStore, MAX_ICON_BYTES, validate_id};
use crate::secure_fs::{self, safe_directory};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};
/// Stores one icon per service as `<id>.png` or `<id>.svg`, outside the launchd plists.
pub struct FsIconStore {
    dir: PathBuf,
}
impl FsIconStore {
    pub fn from_environment() -> Result<Self, AppError> {
        Self::new(
            secure_fs::home_directory()?.join("Library/Application Support/launch2dashboard/icons"),
        )
    }
    pub fn new(dir: PathBuf) -> Result<Self, AppError> {
        safe_directory(&dir)?;
        Ok(Self { dir })
    }
}
impl FsIconStore {
    fn path(&self, id: &str, format: IconFormat) -> Result<PathBuf, AppError> {
        validate_id(id)?;
        Ok(self.dir.join(format!("{id}.{}", format.extension())))
    }
    /// The stored file for `id`, if any; symlinks and other non-regular files are refused.
    fn existing(&self, id: &str) -> Result<Option<PathBuf>, AppError> {
        for format in FORMATS {
            let path = self.path(id, format)?;
            secure_fs::safe_regular(&path, true)?;
            if path.exists() {
                return Ok(Some(path));
            }
        }
        Ok(None)
    }
}
const FORMATS: [IconFormat; 2] = [IconFormat::Png, IconFormat::Svg];
impl IconStore for FsIconStore {
    fn get(&self, id: &str) -> Result<Option<Icon>, AppError> {
        let Some(path) = self.existing(id)? else {
            return Ok(None);
        };
        let mut bytes = Vec::new();
        match secure_fs::open_read(&path) {
            Ok(file) => file
                .take(MAX_ICON_BYTES as u64 + 1)
                .read_to_end(&mut bytes)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        Icon::parse(bytes).map(Some)
    }
    fn version(&self, id: &str) -> Result<Option<String>, AppError> {
        let Some(path) = self.existing(id)? else {
            return Ok(None);
        };
        let metadata = fs::symlink_metadata(&path)?;
        let modified = metadata
            .modified()?
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        Ok(Some(format!("{modified:x}-{:x}", metadata.len())))
    }
    fn put(&self, id: &str, icon: &Icon) -> Result<(), AppError> {
        let path = self.path(id, icon.format())?;
        let temp = self.dir.join(format!(".{id}.{}.tmp", std::process::id()));
        secure_fs::replace_private(&path, &temp, |file| Ok(file.write_all(icon.bytes())?))?;
        for format in FORMATS.into_iter().filter(|f| *f != icon.format()) {
            remove_if_present(&self.path(id, format)?)?;
        }
        Ok(())
    }
    fn delete(&self, id: &str) -> Result<(), AppError> {
        for format in FORMATS {
            remove_if_present(&self.path(id, format)?)?;
        }
        Ok(())
    }
}
fn remove_if_present(path: &Path) -> Result<(), AppError> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ErrorKind;
    use std::fs;
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\npixels";
    const SVG: &[u8] = b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>";
    fn store() -> (tempfile::TempDir, FsIconStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = FsIconStore::new(dir.path().canonicalize().unwrap().join("icons")).unwrap();
        (dir, store)
    }
    fn icon(bytes: &[u8]) -> Icon {
        Icon::parse(bytes.to_vec()).unwrap()
    }
    #[test]
    fn put_get_roundtrip_and_replacing_format_drops_the_old_file() {
        let (_dir, store) = store();
        assert_eq!(store.get("demo").unwrap(), None);
        store.put("demo", &icon(PNG)).unwrap();
        assert_eq!(store.get("demo").unwrap(), Some(icon(PNG)));
        store.put("demo", &icon(SVG)).unwrap();
        assert_eq!(store.get("demo").unwrap(), Some(icon(SVG)));
        assert!(!store.dir.join("demo.png").exists());
        assert_eq!(fs::read_dir(&store.dir).unwrap().count(), 1);
    }
    #[test]
    fn version_tracks_presence_and_replacement() {
        let (_dir, store) = store();
        assert_eq!(store.version("demo").unwrap(), None);
        store.put("demo", &icon(PNG)).unwrap();
        let first = store.version("demo").unwrap().unwrap();
        store.put("demo", &icon(SVG)).unwrap();
        assert_ne!(store.version("demo").unwrap().unwrap(), first);
        store.delete("demo").unwrap();
        assert_eq!(store.version("demo").unwrap(), None);
    }
    #[test]
    fn delete_is_idempotent() {
        let (_dir, store) = store();
        store.put("demo", &icon(PNG)).unwrap();
        store.delete("demo").unwrap();
        store.delete("demo").unwrap();
        assert_eq!(store.get("demo").unwrap(), None);
    }
    #[test]
    fn icons_are_private_files() {
        use std::os::unix::fs::PermissionsExt;
        let (_dir, store) = store();
        store.put("demo", &icon(PNG)).unwrap();
        let mode = fs::metadata(store.dir.join("demo.png"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    #[test]
    fn rejects_invalid_ids_and_symlinks() {
        use std::os::unix::fs::symlink;
        let (dir, store) = store();
        assert_eq!(
            store.put("../x", &icon(PNG)).unwrap_err().kind,
            ErrorKind::Validation
        );
        assert_eq!(store.get("../x").unwrap_err().kind, ErrorKind::Validation);
        let secret = dir.path().join("secret.png");
        fs::write(&secret, PNG).unwrap();
        symlink(&secret, store.dir.join("demo.png")).unwrap();
        assert_eq!(store.get("demo").unwrap_err().kind, ErrorKind::Validation);
        assert_eq!(
            store.put("demo", &icon(PNG)).unwrap_err().kind,
            ErrorKind::Validation
        );
        assert_eq!(fs::read(&secret).unwrap(), PNG);
    }
}
