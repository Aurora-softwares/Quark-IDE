//! Atomic persistence and optimistic external-change protection.
use std::{
    fs,
    io::{self, Write},
    path::Path,
};

#[derive(Debug)]
pub enum SaveError {
    Conflict,
    Io(io::Error),
}
impl From<io::Error> for SaveError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

/// Write beside the destination, flush, then replace it atomically. Following
/// symlinks preserves the link rather than replacing it with a regular file.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let target = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let parent = target
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    if let Ok(metadata) = fs::metadata(&target) {
        if metadata.permissions().readonly() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "File is read-only",
            ));
        }
        temp.as_file().set_permissions(metadata.permissions())?;
    }
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(&target).map_err(|e| e.error)?;
    #[cfg(unix)]
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

/// A missing/deleted file also counts as a conflict with a previously saved file.
/// The check is optimistic: another writer can still race the final rename.
pub fn save_checked(path: &Path, text: &str, expected: &str) -> Result<(), SaveError> {
    match fs::read_to_string(path) {
        Ok(disk) if disk == expected => atomic_write(path, text.as_bytes()).map_err(Into::into),
        Ok(_) => Err(SaveError::Conflict),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Err(SaveError::Conflict),
        Err(e) => Err(SaveError::Io(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn external_changes_and_deletion_are_never_silently_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("main.hy");
        atomic_write(&p, b"original").unwrap();
        fs::write(&p, "external").unwrap();
        assert!(matches!(
            save_checked(&p, "mine", "original"),
            Err(SaveError::Conflict)
        ));
        assert_eq!(fs::read_to_string(&p).unwrap(), "external");
        fs::remove_file(&p).unwrap();
        assert!(matches!(
            save_checked(&p, "mine", "original"),
            Err(SaveError::Conflict)
        ));
    }
    #[test]
    fn successful_save_replaces_complete_utf8_and_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("main.hy");
        atomic_write(&p, b"old").unwrap();
        save_checked(&p, "π new", "old").unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "π new");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
    #[cfg(unix)]
    #[test]
    fn save_preserves_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("real.hy");
        let link = dir.path().join("link.hy");
        fs::write(&p, "old").unwrap();
        std::os::unix::fs::symlink(&p, &link).unwrap();
        save_checked(&link, "new", "old").unwrap();
        assert!(link.is_symlink());
        assert_eq!(fs::read_to_string(p).unwrap(), "new");
    }

    #[test]
    fn atomic_write_creates_parents_and_empty_files() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("nested/new.hy");
        atomic_write(&p, b"").unwrap();
        assert!(p.is_file());
        assert_eq!(fs::metadata(p).unwrap().len(), 0);
    }
    #[test]
    fn readonly_file_failure_preserves_original_and_cleans_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("readonly.hy");
        fs::write(&p, "old").unwrap();
        let mut permissions = fs::metadata(&p).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&p, permissions).unwrap();
        assert!(atomic_write(&p, b"new").is_err());
        assert_eq!(fs::read_to_string(&p).unwrap(), "old");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        #[cfg(windows)]
        {
            let mut permissions = fs::metadata(&p).unwrap().permissions();
            permissions.set_readonly(false);
            fs::set_permissions(p, permissions).unwrap();
        }
    }
    #[test]
    fn destination_directory_is_not_replaced_on_write_failure() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("folder");
        fs::create_dir(&p).unwrap();
        fs::write(p.join("keep"), "original").unwrap();
        assert!(atomic_write(&p, b"new").is_err());
        assert_eq!(fs::read_to_string(p.join("keep")).unwrap(), "original");
    }
    #[cfg(unix)]
    #[test]
    fn atomic_replace_retains_permissions_through_symlink() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("real.hy");
        let link = dir.path().join("link.hy");
        fs::write(&p, "old").unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o640)).unwrap();
        std::os::unix::fs::symlink(&p, &link).unwrap();
        atomic_write(&link, b"new").unwrap();
        assert!(link.is_symlink());
        assert_eq!(fs::metadata(p).unwrap().permissions().mode() & 0o777, 0o640);
    }
}
