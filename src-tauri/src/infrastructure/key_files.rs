use crate::{
    application::keys::KeyFiles,
    domain::{AppError, ErrorCode},
};
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;
fn file_error() -> AppError {
    AppError::new(
        ErrorCode::KeyFileUnavailable,
        "The SSH key file is unavailable or has unsafe permissions.",
    )
}
pub struct LocalKeyFiles;
impl KeyFiles for LocalKeyFiles {
    fn read(&self, path: &Path) -> Result<Zeroizing<String>, AppError> {
        check_permissions(path)?;
        use std::io::Read;
        let file = std::fs::File::open(path).map_err(|_| file_error())?;
        let mut contents = Zeroizing::new(String::new());
        file.take(1024 * 1024 + 1)
            .read_to_string(&mut contents)
            .map_err(|_| file_error())?;
        if contents.len() > 1024 * 1024 {
            return Err(file_error());
        }
        Ok(contents)
    }
    fn write(&self, directory: &Path, id: &str, bytes: &[u8]) -> Result<PathBuf, AppError> {
        write_managed(directory, id, bytes)
    }
    fn remove(&self, directory: &Path, path: &Path) -> Result<(), AppError> {
        if path.parent() != Some(directory) {
            return Err(file_error());
        }
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(file_error()),
        }
    }
}
pub fn check_permissions(path: &Path) -> Result<(), AppError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if std::fs::metadata(path)
            .map_err(|_| file_error())?
            .permissions()
            .mode()
            & 0o077
            != 0
        {
            return Err(file_error());
        }
    }
    Ok(())
}
pub(crate) fn write_managed(directory: &Path, id: &str, bytes: &[u8]) -> Result<PathBuf, AppError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true).mode(0o700);
        builder.create(directory).map_err(|_| file_error())?;
        std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| file_error())?;
    }
    let target = directory.join(id);
    use std::io::Write;
    let parsed = ssh_key::PrivateKey::from_openssh(bytes).map_err(|_| file_error())?;
    if parsed.is_encrypted() {
        let mut temp = tempfile::NamedTempFile::new_in(directory).map_err(|_| file_error())?;
        temp.write_all(bytes).map_err(|_| file_error())?;
        temp.as_file().sync_all().map_err(|_| file_error())?;
        temp.persist_noclobber(&target).map_err(|_| file_error())?;
    } else {
        // No plaintext staging file: publication to app readers is the later
        // SQLite transaction, after this final UUID file has been synced.
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&target).map_err(|_| file_error())?;
        file.write_all(bytes).map_err(|_| file_error())?;
        file.sync_all().map_err(|_| file_error())?;
    }
    std::fs::File::open(directory)
        .and_then(|d| d.sync_all())
        .map_err(|_| file_error())?;
    Ok(target)
}
