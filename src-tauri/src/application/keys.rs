use crate::{
    domain::{AppError, ErrorCode},
    infrastructure::{
        credentials::{validate_key_path, CredentialStore},
        ssh_keys,
    },
};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use zeroize::Zeroizing;

pub use crate::domain::keys::SshKey;
pub struct NewKey {
    pub id: String,
    pub name: String,
    pub algorithm: String,
    pub fingerprint: String,
    pub public_key: String,
    pub path: String,
    pub storage_type: String,
    pub reference: Option<String>,
    pub now: String,
}
pub struct Cleanup {
    pub id: String,
    pub path: Option<String>,
    pub reference: Option<String>,
}
pub trait KeyFiles: Send + Sync {
    fn read(&self, path: &Path) -> Result<Zeroizing<String>, AppError>;
    fn write(&self, directory: &Path, id: &str, bytes: &[u8]) -> Result<PathBuf, AppError>;
    fn remove(&self, directory: &Path, path: &Path) -> Result<(), AppError>;
}
pub trait KeyRepository: super::ServerRepository {
    fn list_keys(&self) -> Result<Vec<SshKey>, AppError>;
    fn get_key(&self, id: &str) -> Result<SshKey, AppError>;
    fn resolve(&self, id: &str) -> Result<(PathBuf, Option<String>, String), AppError>;
    fn assigned(&self, id: &str) -> Result<Option<String>, AppError>;
    fn servers(&self, id: &str) -> Result<Vec<String>, AppError>;
    fn rename(&self, id: &str, name: &str) -> Result<(), AppError>;
    fn contains_fingerprint(&self, fingerprint: &str) -> Result<bool, AppError>;
    fn queue_cleanup(&self, entry: &Cleanup) -> Result<(), AppError>;
    fn insert(&self, key: &NewKey) -> Result<(), AppError>;
    fn delete_queued(&self, entry: &Cleanup) -> Result<(), AppError>;
    fn cleanup_entries(&self) -> Result<Vec<Cleanup>, AppError>;
    fn finish_cleanup(&self, id: &str) -> Result<(), AppError>;
    fn save_server(
        &self,
        id: Option<&str>,
        input: &crate::domain::ServerInput,
        key: &str,
    ) -> Result<crate::domain::Server, AppError>;
    fn assign(&self, server: &str, key: Option<&str>) -> Result<(), AppError>;
}
pub struct KeyService {
    pub repository: Arc<dyn KeyRepository>,
    pub credentials: Arc<dyn CredentialStore>,
    directory: PathBuf,
    files: Arc<dyn KeyFiles>,
    operation: Mutex<()>,
}
fn fail() -> AppError {
    AppError::new(
        ErrorCode::DatabaseError,
        "SSH key operation failed. Retry or restart RemoteDock.",
    )
}
fn file_error() -> AppError {
    AppError::new(
        ErrorCode::KeyFileUnavailable,
        "The SSH key file is unavailable or has unsafe permissions.",
    )
}
impl KeyService {
    pub fn new(
        repository: Arc<dyn KeyRepository>,
        credentials: Arc<dyn CredentialStore>,
        directory: PathBuf,
    ) -> Self {
        Self {
            repository,
            credentials,
            directory,
            files: Arc::new(crate::infrastructure::key_files::LocalKeyFiles),
            operation: Mutex::new(()),
        }
    }
    pub fn list(&self) -> Result<Vec<SshKey>, AppError> {
        self.repository.list_keys()
    }
    pub fn get(&self, id: &str) -> Result<SshKey, AppError> {
        self.repository.get_key(id)
    }
    pub fn resolve(&self, id: &str) -> Result<(PathBuf, Option<String>, String), AppError> {
        self.repository.resolve(id)
    }
    pub fn assigned(&self, id: &str) -> Result<Option<String>, AppError> {
        self.repository.assigned(id)
    }
    pub fn servers(&self, id: &str) -> Result<Vec<String>, AppError> {
        self.repository.servers(id)
    }
    fn name(name: &str) -> Result<&str, AppError> {
        let name = name.trim();
        if name.is_empty() || name.len() > 128 || name.chars().any(char::is_control) {
            return Err(AppError::validation(
                "name",
                "Enter a key name of 1–128 characters.",
            ));
        }
        Ok(name)
    }
    pub fn rename(&self, id: &str, name: &str) -> Result<SshKey, AppError> {
        self.get(id)?;
        let name = Self::name(name)?;
        self.repository.rename(id, name)?;
        self.get(id)
    }
    pub fn create(
        &self,
        name: &str,
        rsa: bool,
        path: Option<&str>,
        managed: bool,
        secret: Option<&str>,
        save: bool,
    ) -> Result<SshKey, AppError> {
        let _guard = self.operation.lock().map_err(|_| fail())?;
        let name = Self::name(name)?;
        if secret.is_some_and(|s| s.len() > 4096) || save && secret.is_none_or(str::is_empty) {
            return Err(AppError::validation(
                "passphrase",
                "Enter a passphrase before saving it.",
            ));
        }
        let external = path.map(validate_key_path).transpose()?;
        let encoded = match &external {
            Some(p) => {
                let contents = self.files.read(p)?;
                ssh_keys::import(&contents, secret)?
            }
            None => ssh_keys::generate(rsa, secret)?,
        };
        if self.repository.contains_fingerprint(&encoded.fingerprint)? {
            return Err(AppError::validation(
                "file",
                "This public key is already registered.",
            ));
        }
        let id = uuid::Uuid::new_v4().to_string();
        let managed = managed || external.is_none();
        let target = if managed {
            self.directory.join(&id)
        } else {
            external.ok_or_else(file_error)?
        };
        if managed && target.try_exists().map_err(|_| file_error())? {
            return Err(AppError::validation(
                "file",
                "A managed key already exists at this location.",
            ));
        }
        let reference = save.then(|| format!("ssh-key-{id}"));
        self.repository.queue_cleanup(&Cleanup {
            id: id.clone(),
            path: managed.then(|| target.to_string_lossy().into_owned()),
            reference: reference.clone(),
        })?;
        let result = (|| {
            if managed {
                self.files
                    .write(&self.directory, &id, encoded.private.as_bytes())?;
            }
            if let Some(r) = &reference {
                self.credentials.set(r, secret.unwrap_or_default())?;
            }
            let now = chrono::Utc::now().to_rfc3339();
            self.repository.insert(&NewKey {
                id: id.clone(),
                name: name.into(),
                algorithm: encoded.algorithm,
                fingerprint: encoded.fingerprint,
                public_key: encoded.public,
                path: target.to_string_lossy().into_owned(),
                storage_type: if managed {
                    "MANAGED".into()
                } else {
                    "EXTERNAL".into()
                },
                reference: reference.clone(),
                now,
            })?;
            Ok::<(), AppError>(())
        })();
        if let Err(error) = result {
            self.cleanup()?;
            return Err(error);
        }
        self.get(&id)
    }
    pub fn delete(&self, id: &str, confirmed: bool, delete_file: bool) -> Result<(), AppError> {
        let _guard = self.operation.lock().map_err(|_| fail())?;
        let key = self.get(id)?;
        if !confirmed {
            return Err(AppError::validation("confirmation", "Confirm key removal."));
        }
        if key.server_count > 0 {
            return Err(AppError::validation(
                "key",
                "Unassign this key from every server before removing it.",
            ));
        }
        let (path, reference, _) = self.resolve(id)?;
        let cleanup_path = if delete_file && key.storage_type == "MANAGED" {
            if path.parent() != Some(self.directory.as_path()) {
                return Err(file_error());
            }
            Some(path.to_string_lossy().into_owned())
        } else {
            None
        };
        self.repository.delete_queued(&Cleanup {
            id: id.into(),
            path: cleanup_path,
            reference,
        })?;
        self.cleanup()
    }
    pub fn cleanup(&self) -> Result<(), AppError> {
        let entries = self.repository.cleanup_entries()?;
        for Cleanup {
            id,
            path,
            reference,
        } in entries
        {
            if let Some(r) = reference {
                self.credentials.remove(&r)?;
            }
            if let Some(p) = path {
                let p = PathBuf::from(p);
                if p.parent() != Some(self.directory.as_path()) {
                    return Err(file_error());
                }
                self.files.remove(&self.directory, &p)?;
            }
            self.repository.finish_cleanup(&id)?;
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::key_files::{check_permissions, write_managed};
    use crate::infrastructure::persistence::SqliteServerRepository;
    use crate::{
        application::connection::ConnectionService, domain::ServerInput,
        infrastructure::ssh::RealSshTransport,
    };
    use std::collections::HashMap;
    #[derive(Default)]
    struct MemoryCredentials(Mutex<HashMap<String, String>>);
    impl CredentialStore for MemoryCredentials {
        fn get(&self, r: &str) -> Result<Option<Zeroizing<String>>, AppError> {
            Ok(self.0.lock().unwrap().get(r).cloned().map(Zeroizing::new))
        }
        fn set(&self, r: &str, s: &str) -> Result<(), AppError> {
            self.0.lock().unwrap().insert(r.into(), s.into());
            Ok(())
        }
        fn remove(&self, r: &str) -> Result<(), AppError> {
            self.0.lock().unwrap().remove(r);
            Ok(())
        }
    }
    fn fixture() -> (tempfile::TempDir, Arc<KeyService>) {
        let dir = tempfile::tempdir().unwrap();
        let repo = Arc::new(SqliteServerRepository::open(&dir.path().join("test.sqlite")).unwrap());
        let keys = Arc::new(KeyService::new(
            repo,
            Arc::new(MemoryCredentials::default()),
            dir.path().join("keys"),
        ));
        (dir, keys)
    }
    #[test]
    fn managed_permissions_identity_duplicates_and_removal() {
        let (_dir, keys) = fixture();
        let key = keys
            .create("First", false, None, true, None, false)
            .unwrap();
        let (path, _, _) = keys.resolve(&key.id).unwrap();
        assert!(path.exists());
        check_permissions(&path).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                std::fs::metadata(path.parent().unwrap())
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
        }
        assert!(keys
            .create(
                "Duplicate",
                false,
                Some(path.to_str().unwrap()),
                false,
                None,
                false
            )
            .is_err());
        assert!(keys.delete(&key.id, false, true).is_err());
        assert!(path.exists());
        keys.rename(&key.id, "Renamed").unwrap();
        assert_eq!(keys.get(&key.id).unwrap().name, "Renamed");
        keys.delete(&key.id, true, true).unwrap();
        assert!(!path.exists());
        assert!(keys.list().unwrap().is_empty());
    }
    #[test]
    fn external_files_are_retained_and_missing_files_fail() {
        let (dir, keys) = fixture();
        let raw = ssh_keys::generate(false, None).unwrap();
        let path = write_managed(dir.path(), "external", raw.private.as_bytes()).unwrap();
        let key = keys
            .create(
                "External",
                false,
                Some(path.to_str().unwrap()),
                false,
                None,
                false,
            )
            .unwrap();
        assert_eq!(key.storage_type, "EXTERNAL");
        keys.delete(&key.id, true, true).unwrap();
        assert!(path.exists());
        assert!(keys
            .create(
                "Missing",
                false,
                Some(dir.path().join("missing").to_str().unwrap()),
                false,
                None,
                false
            )
            .is_err());
    }
    #[test]
    fn reusable_assignments_block_deletion_and_preserve_trust() {
        let (_dir, keys) = fixture();
        let key = keys
            .create("Shared", false, None, true, None, false)
            .unwrap();
        let connections = ConnectionService::new(
            keys.repository.clone(),
            Arc::new(RealSshTransport),
            keys.credentials.clone(),
        )
        .with_keys(keys.clone());
        let input = ServerInput {
            name: "Server".into(),
            host: "example.com".into(),
            port: 22,
            username: "ubuntu".into(),
            notes: None,
        };
        let a = keys.repository.create(&input).unwrap();
        let b = keys.repository.create(&input).unwrap();
        let mut record = keys.repository.connection(&a.id).unwrap();
        record.trusted_key = Some(vec![1, 2, 3]);
        record.key_path = Some("/legacy/key".into());
        keys.repository.save_connection(&a.id, &record).unwrap();
        connections.assign_key(&a.id, Some(&key.id)).unwrap();
        connections.assign_key(&b.id, Some(&key.id)).unwrap();
        assert_eq!(keys.get(&key.id).unwrap().server_count, 2);
        assert!(keys.delete(&key.id, true, true).is_err());
        assert_eq!(
            keys.repository.connection(&a.id).unwrap().trusted_key,
            record.trusted_key
        );
        connections.assign_key(&a.id, None).unwrap();
        assert_eq!(
            keys.repository.connection(&a.id).unwrap().key_path,
            record.key_path
        );
        keys.repository.delete(&b.id).unwrap();
        keys.delete(&key.id, true, false).unwrap();
    }
    #[test]
    fn encrypted_import_and_opt_in_keychain() {
        let (dir, keys) = fixture();
        let raw = ssh_keys::generate(false, Some("secret")).unwrap();
        let path = write_managed(dir.path(), "encrypted", raw.private.as_bytes()).unwrap();
        assert!(keys
            .create(
                "No password",
                false,
                Some(path.to_str().unwrap()),
                false,
                None,
                false
            )
            .is_err());
        assert!(keys
            .create(
                "Wrong password",
                false,
                Some(path.to_str().unwrap()),
                false,
                Some("wrong"),
                false
            )
            .is_err());
        let k = keys
            .create(
                "Encrypted",
                false,
                Some(path.to_str().unwrap()),
                true,
                Some("secret"),
                true,
            )
            .unwrap();
        assert!(k.passphrase_saved);
        let (_, reference, _) = keys.resolve(&k.id).unwrap();
        assert_eq!(
            keys.credentials
                .get(reference.as_deref().unwrap())
                .unwrap()
                .unwrap()
                .as_str(),
            "secret"
        );
        keys.delete(&k.id, true, true).unwrap();
        assert!(keys
            .credentials
            .get(reference.as_deref().unwrap())
            .unwrap()
            .is_none());
        assert!(path.exists());
    }
    #[test]
    fn failed_file_cleanup_is_durable_and_retryable() {
        use std::sync::atomic::{AtomicBool, Ordering};
        struct Files(Arc<AtomicBool>);
        impl KeyFiles for Files {
            fn read(&self, p: &Path) -> Result<Zeroizing<String>, AppError> {
                crate::infrastructure::key_files::LocalKeyFiles.read(p)
            }
            fn write(&self, d: &Path, id: &str, b: &[u8]) -> Result<PathBuf, AppError> {
                write_managed(d, id, b)
            }
            fn remove(&self, d: &Path, p: &Path) -> Result<(), AppError> {
                if self.0.load(Ordering::SeqCst) {
                    return Err(file_error());
                }
                crate::infrastructure::key_files::LocalKeyFiles.remove(d, p)
            }
        }
        let (_dir, mut keys) = fixture();
        let failed = Arc::new(AtomicBool::new(true));
        Arc::get_mut(&mut keys).unwrap().files = Arc::new(Files(failed.clone()));
        let key = keys
            .create("Pending cleanup", false, None, true, None, false)
            .unwrap();
        let (path, _, _) = keys.resolve(&key.id).unwrap();
        assert!(keys.delete(&key.id, true, true).is_err());
        assert!(keys.list().unwrap().is_empty());
        assert!(path.exists());
        assert_eq!(keys.repository.cleanup_entries().unwrap().len(), 1);
        failed.store(false, Ordering::SeqCst);
        keys.cleanup().unwrap();
        assert!(!path.exists());
        assert!(keys.repository.cleanup_entries().unwrap().is_empty());
    }
    #[test]
    fn server_metadata_and_assignment_are_atomic() {
        let (_dir, keys) = fixture();
        let input = ServerInput {
            name: "Atomic".into(),
            host: "example.com".into(),
            port: 22,
            username: "ubuntu".into(),
            notes: None,
        };
        assert!(keys
            .repository
            .save_server(None, &input, "missing-key")
            .is_err());
        assert!(keys.repository.list().unwrap().is_empty());
        let key = keys.create("Key", false, None, true, None, false).unwrap();
        let server = keys.repository.save_server(None, &input, &key.id).unwrap();
        assert_eq!(
            keys.assigned(&server.id).unwrap().as_deref(),
            Some(key.id.as_str())
        );
        let mut changed = input;
        changed.name = "Changed".into();
        assert!(keys
            .repository
            .save_server(Some(&server.id), &changed, "missing-key")
            .is_err());
        assert_eq!(keys.repository.get(&server.id).unwrap().name, "Atomic");
    }
    #[cfg(unix)]
    #[test]
    fn unsafe_external_permissions_are_rejected_without_modifying_source() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, keys) = fixture();
        let raw = ssh_keys::generate(false, None).unwrap();
        let path = write_managed(dir.path(), "unsafe", raw.private.as_bytes()).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(keys
            .create(
                "Unsafe",
                false,
                Some(path.to_str().unwrap()),
                false,
                None,
                false
            )
            .is_err());
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o644
        );
    }
}
