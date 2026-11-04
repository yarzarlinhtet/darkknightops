use crate::domain::{AppError, ErrorCode};
use zeroize::Zeroizing;
pub trait CredentialStore: Send + Sync {
    fn get(&self, reference: &str) -> Result<Option<Zeroizing<String>>, AppError>;
    fn set(&self, reference: &str, secret: &str) -> Result<(), AppError>;
    fn remove(&self, reference: &str) -> Result<(), AppError>;
}
pub struct OsCredentialStore;
fn unavailable() -> AppError {
    AppError::new(
        ErrorCode::CredentialStoreUnavailable,
        "The OS credential store is unavailable or access was denied. Unlock Keychain and retry.",
    )
}
#[cfg(target_os = "macos")]
impl CredentialStore for OsCredentialStore {
    fn get(&self, reference: &str) -> Result<Option<Zeroizing<String>>, AppError> {
        match keyring::Entry::new("com.remotedock.desktop.ssh", reference)
            .map_err(|_| unavailable())?
            .get_password()
        {
            Ok(secret) => Ok(Some(Zeroizing::new(secret))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(unavailable()),
        }
    }
    fn set(&self, reference: &str, secret: &str) -> Result<(), AppError> {
        keyring::Entry::new("com.remotedock.desktop.ssh", reference)
            .map_err(|_| unavailable())?
            .set_password(secret)
            .map_err(|_| unavailable())
    }
    fn remove(&self, reference: &str) -> Result<(), AppError> {
        match keyring::Entry::new("com.remotedock.desktop.ssh", reference)
            .map_err(|_| unavailable())?
            .delete_credential()
        {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(unavailable()),
        }
    }
}
#[cfg(not(target_os = "macos"))]
impl CredentialStore for OsCredentialStore {
    fn get(&self, _: &str) -> Result<Option<Zeroizing<String>>, AppError> {
        Err(unavailable())
    }
    fn set(&self, _: &str, _: &str) -> Result<(), AppError> {
        Err(unavailable())
    }
    fn remove(&self, _: &str) -> Result<(), AppError> {
        Err(unavailable())
    }
}
pub fn validate_key_path(path: &str) -> Result<std::path::PathBuf, AppError> {
    let fail = || {
        AppError::new(
            ErrorCode::KeyFileUnavailable,
            "Select an accessible SSH private-key file.",
        )
    };
    if path.len() > 4096
        || path.chars().any(char::is_control)
        || !std::path::Path::new(path).is_absolute()
    {
        return Err(fail());
    }
    let path = std::fs::canonicalize(path).map_err(|_| fail())?;
    let file = std::fs::File::open(&path).map_err(|_| fail())?;
    let metadata = file.metadata().map_err(|_| fail())?;
    if !metadata.is_file() || metadata.len() > 1024 * 1024 {
        return Err(fail());
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paths_must_be_absolute_readable_bounded_files() {
        assert!(validate_key_path("../key").is_err());
        assert!(validate_key_path("/tmp/key\n").is_err());
        let dir = tempfile::tempdir().unwrap();
        assert!(validate_key_path(&dir.path().to_string_lossy()).is_err());
        let file = tempfile::NamedTempFile::new().unwrap();
        assert!(validate_key_path(&file.path().to_string_lossy()).is_ok());
        file.as_file().set_len(1024 * 1024 + 1).unwrap();
        assert!(validate_key_path(&file.path().to_string_lossy()).is_err());
    }
}
