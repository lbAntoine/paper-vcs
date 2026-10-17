//! Small filesystem helpers shared by everything that writes into `.git/paper/`.

use std::io::{self, Write};
use std::path::Path;

use tempfile::NamedTempFile;

/// Writes `bytes` to `path` atomically: into a temporary file in the same directory,
/// flushed to disk, then renamed over `path`. A crash leaves the old file or the new one,
/// never a mix.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let mut file = NamedTempFile::new_in(dir)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|err| err.error)?;
    Ok(())
}

/// Writes `bytes` to a new file at `path`, flushed to disk before it appears there. Fails if
/// `path` already exists: a file written this way is never replaced.
pub(crate) fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let mut file = NamedTempFile::new_in(dir)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    match file.persist_noclobber(path) {
        Ok(_) => Ok(()),
        Err(err) if noclobber_unsupported(&err.error) => {
            // No hard links or no-replace rename here (FAT, some network mounts). Ids are
            // 128 random bits and the project lock is held, so a check then a rename is safe.
            if path.exists() {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("{} already exists", path.display()),
                ));
            }
            err.file.persist(path).map_err(|err| err.error)?;
            Ok(())
        }
        Err(err) => Err(err.error),
    }
}

/// Whether a failed no-clobber persist means "this filesystem can't do that" rather than
/// "the file exists" or a real failure.
fn noclobber_unsupported(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::Unsupported | io::ErrorKind::PermissionDenied
    )
}

/// Flushes the directory entry itself, so a file renamed into `dir` survives a power cut.
/// Does nothing where a directory can't be opened for it (Windows).
pub(crate) fn sync_dir(dir: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        match std::fs::File::open(dir)?.sync_all() {
            // FUSE and some network shares don't support fsync on directories.
            Err(err) if err.kind() == io::ErrorKind::InvalidInput => Ok(()),
            other => other,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_the_file_and_leaves_nothing_else_behind() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        write_atomic(&path, b"old").unwrap();
        write_atomic(&path, b"new").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn only_unsupported_and_permission_failures_trigger_the_rename_fallback() {
        let kind = |k| noclobber_unsupported(&io::Error::from(k));
        assert!(kind(io::ErrorKind::Unsupported));
        assert!(kind(io::ErrorKind::PermissionDenied));
        assert!(!kind(io::ErrorKind::AlreadyExists));
        assert!(!kind(io::ErrorKind::NotFound));
    }

    #[test]
    fn write_new_refuses_to_replace_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("op.json");
        write_new(&path, b"first").unwrap();
        assert!(write_new(&path, b"second").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"first");
        sync_dir(dir.path()).unwrap();
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
