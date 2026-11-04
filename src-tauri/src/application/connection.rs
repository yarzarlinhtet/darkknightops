use super::ServerRepository;
use crate::{
    domain::{connection::*, validate_id, AppError, ErrorCode, Server},
    infrastructure::{
        credentials::{validate_key_path, CredentialStore},
        discovery,
        ssh::{
            self, CancellationToken, ConnectFailure, RemoteCommand, RemoteSession, SshTransport,
        },
    },
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};
use zeroize::Zeroizing;
fn lock<T>(mutex: &Mutex<T>) -> Result<MutexGuard<'_, T>, AppError> {
    mutex.lock().map_err(|_| {
        AppError::new(
            ErrorCode::Internal,
            "Connection state unavailable. Restart RemoteDock.",
        )
    })
}
fn busy() -> AppError {
    AppError::new(
        ErrorCode::Busy,
        "A connection operation is already running or the connection limit was reached.",
    )
}
struct Runtime {
    server_id: String,
    lease: String,
    touched: Instant,
    session: Option<Box<dyn RemoteSession>>,
    passphrase: Option<Zeroizing<String>>,
}
struct Observation {
    dto: HostKeyObservation,
    key: Vec<u8>,
    server: Server,
    revision: u64,
    expires: Instant,
}
pub struct ConnectionService {
    keys: Option<Arc<super::keys::KeyService>>,
    repository: Arc<dyn ServerRepository>,
    transport: Arc<dyn SshTransport>,
    credentials: Arc<dyn CredentialStore>,
    active: Mutex<HashMap<String, CancellationToken>>,
    runtimes: Mutex<HashMap<String, Arc<Mutex<Runtime>>>>,
    observations: Mutex<HashMap<String, Observation>>,
    leases: Mutex<HashMap<String, String>>,
}
/// Worker-owned state survives detail-page closure; secrets remain zeroized.
pub struct ProvisioningSession(Runtime);
impl ProvisioningSession {
    pub fn session(&mut self) -> Result<&mut dyn RemoteSession, AppError> {
        match self.0.session.as_mut() {
            Some(session) => Ok(session.as_mut()),
            None => Err(ssh::error(ErrorCode::Unreachable)),
        }
    }
}
pub struct OperationGuard<'a> {
    service: &'a ConnectionService,
    id: String,
    pub token: CancellationToken,
}
impl Drop for OperationGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut active) = self.service.active.lock() {
            active.remove(&self.id);
        }
    }
}
impl ConnectionService {
    pub fn new(
        repository: Arc<dyn ServerRepository>,
        transport: Arc<dyn SshTransport>,
        credentials: Arc<dyn CredentialStore>,
    ) -> Self {
        Self {
            keys: None,
            repository,
            transport,
            credentials,
            active: Mutex::new(HashMap::new()),
            runtimes: Mutex::new(HashMap::new()),
            observations: Mutex::new(HashMap::new()),
            leases: Mutex::new(HashMap::new()),
        }
    }
    pub fn with_keys(mut self, keys: Arc<super::keys::KeyService>) -> Self {
        self.keys = Some(keys);
        self
    }
    pub fn assign_key(&self, id: &str, key_id: Option<&str>) -> Result<(), AppError> {
        let _guard = self.begin(id)?;
        let keys = self
            .keys
            .as_ref()
            .ok_or_else(|| AppError::new(ErrorCode::Internal, "Key service unavailable."))?;
        self.repository.get(id)?;
        if let Some(k) = key_id {
            keys.get(k)?;
        }
        self.invalidate_runtime(id)?;
        keys.repository.assign(id, key_id)?;
        Ok(())
    }
    pub fn begin(&self, id: &str) -> Result<OperationGuard<'_>, AppError> {
        validate_id(id)?;
        let mut active = lock(&self.active)?;
        let runtimes = lock(&self.runtimes)?;
        let occupied = runtimes
            .keys()
            .chain(active.keys())
            .collect::<std::collections::HashSet<_>>();
        if active.contains_key(id)
            || active.len() >= 4
            || (!occupied.contains(&id.to_string()) && occupied.len() >= 4)
        {
            return Err(busy());
        }
        let token = CancellationToken::default();
        active.insert(id.into(), token.clone());
        Ok(OperationGuard {
            service: self,
            id: id.into(),
            token,
        })
    }
    pub fn cancel(&self, id: &str) -> Result<(), AppError> {
        validate_id(id)?;
        self.repository.get(id)?;
        if let Some(token) = lock(&self.active)?.get(id) {
            token.cancel();
        }
        Ok(())
    }
    pub fn expire_idle(&self) {
        // Match begin/open lock ordering. Long provisioning retains its detail
        // lease, while the worker itself owns a separate SSH session.
        if let (Ok(active), Ok(mut runtimes)) = (self.active.lock(), self.runtimes.lock()) {
            runtimes.retain(|id, runtime| {
                runtime
                    .try_lock()
                    .map(|mut value| {
                        if active.contains_key(id) {
                            value.touched = Instant::now();
                        }
                        value.touched.elapsed() < Duration::from_secs(60)
                    })
                    .unwrap_or(true)
            });
        }
        if let (Ok(runtimes), Ok(mut leases)) = (self.runtimes.lock(), self.leases.lock()) {
            leases.retain(|id, _| runtimes.contains_key(id));
        }
        if let Ok(mut observations) = self.observations.lock() {
            observations.retain(|_, o| Instant::now() < o.expires);
        }
    }
    pub fn open(&self, id: &str) -> Result<String, AppError> {
        validate_id(id)?;
        self.repository.get(id)?;
        self.expire_idle();
        let active = lock(&self.active)?;
        let mut runtimes = lock(&self.runtimes)?;
        if let Some(runtime) = runtimes.get(id).cloned() {
            drop(runtimes);
            drop(active);
            let mut runtime = lock(&runtime)?;
            runtime.touched = Instant::now();
            return Ok(runtime.lease.clone());
        }
        let occupied = runtimes
            .keys()
            .chain(active.keys())
            .collect::<std::collections::HashSet<_>>();
        if !occupied.contains(&id.to_string()) && occupied.len() >= 4 {
            return Err(busy());
        }
        let lease = uuid::Uuid::new_v4().to_string();
        runtimes.insert(
            id.into(),
            Arc::new(Mutex::new(Runtime {
                server_id: id.into(),
                lease: lease.clone(),
                touched: Instant::now(),
                session: None,
                passphrase: None,
            })),
        );
        lock(&self.leases)?.insert(id.into(), lease.clone());
        Ok(lease)
    }
    fn runtime(&self, id: &str, lease: &str) -> Result<Arc<Mutex<Runtime>>, AppError> {
        validate_id(id)?;
        validate_id(lease)?;
        let runtime = lock(&self.runtimes)?.get(id).cloned().ok_or_else(|| {
            AppError::new(
                ErrorCode::NotFound,
                "The detail session expired. Reopen this server.",
            )
        })?;
        {
            let mut value = lock(&runtime)?;
            if value.lease != lease || value.server_id != id {
                return Err(AppError::not_found());
            }
            value.touched = Instant::now();
        }
        Ok(runtime)
    }
    pub fn close(&self, id: &str, lease: &str) -> Result<(), AppError> {
        validate_id(id)?;
        validate_id(lease)?;
        // Same lock order as open/begin. An old lease cannot close or cancel its replacement.
        let active = lock(&self.active)?;
        let mut runtimes = lock(&self.runtimes)?;
        let mut leases = lock(&self.leases)?;
        if leases.get(id).map(String::as_str) != Some(lease) {
            return Ok(());
        }
        if let Some(token) = active.get(id) {
            token.cancel();
        }
        leases.remove(id);
        runtimes.remove(id);
        Ok(())
    }
    pub fn provisioning_session(
        &self,
        id: &str,
        lease: &str,
    ) -> Result<ProvisioningSession, AppError> {
        let runtime = self.runtime(id, lease)?;
        let runtime = lock(&runtime)?;
        let mut worker = ProvisioningSession(Runtime {
            server_id: id.into(),
            lease: uuid::Uuid::new_v4().to_string(),
            touched: Instant::now(),
            session: None,
            passphrase: runtime
                .passphrase
                .as_ref()
                .map(|v| Zeroizing::new(v.to_string())),
        });
        drop(runtime);
        self.reconnect_provisioning(id, &mut worker)?;
        Ok(worker)
    }
    pub fn reconnect_provisioning(
        &self,
        id: &str,
        worker: &mut ProvisioningSession,
    ) -> Result<(), AppError> {
        worker.0.session = None;
        let server = self.repository.get(id)?;
        let record = self.repository.connection(id)?;
        if let Some(observation) = self.connect(
            &server,
            &record,
            &mut worker.0,
            &CancellationToken::default(),
        )? {
            return Err(ssh::error(if observation.previous_fingerprint.is_some() {
                ErrorCode::HostKeyMismatch
            } else {
                ErrorCode::HostKeyUntrusted
            }));
        }
        Ok(())
    }
    pub fn provisioning_identity(&self, id: &str) -> Result<String, AppError> {
        use sha2::{Digest, Sha256};
        let server = self.repository.get(id)?;
        let record = self.repository.connection(id)?;
        let key_id = self
            .keys
            .as_ref()
            .map(|k| k.assigned(id))
            .transpose()?
            .flatten();
        let data = serde_json::to_vec(&(
            server.host,
            server.port,
            server.username,
            record.revision,
            record.trusted_key,
            record.key_path,
            key_id,
        ))
        .map_err(|_| ssh::error(ErrorCode::Internal))?;
        Ok(format!("{:x}", Sha256::digest(data)))
    }
    pub fn metadata(&self, id: &str) -> Result<SshMetadata, AppError> {
        validate_id(id)?;
        let record = self.repository.connection(id)?;
        let assigned = self
            .keys
            .as_ref()
            .map(|keys| keys.assigned(id))
            .transpose()?
            .flatten();
        let passphrase_saved = if let (Some(keys), Some(key_id)) = (&self.keys, assigned) {
            keys.get(&key_id)?.passphrase_saved
        } else {
            record.credential_ref.is_some()
        };
        Ok(SshMetadata {
            key_path: record.key_path,
            passphrase_saved,
            trusted_fingerprint: record.trusted_key.as_deref().map(ssh::fingerprint),
            trusted_algorithm: record.trusted_algorithm,
            last_test: record.last_test,
            last_success_at: record.last_success_at,
            info: record.info,
            docker: record.docker,
        })
    }
    pub fn configure(
        &self,
        id: &str,
        path: Option<String>,
        passphrase: Option<Zeroizing<String>>,
        save: bool,
    ) -> Result<(), AppError> {
        let _guard = self.begin(id)?;
        let mut record = self.repository.connection(id)?;
        let path = path
            .map(|p| validate_key_path(&p).map(|p| p.to_string_lossy().into_owned()))
            .transpose()?;
        if save && (path.is_none() || passphrase.as_ref().is_none_or(|v| v.is_empty())) {
            return Err(AppError::validation(
                "passphrase",
                "Enter a passphrase to save in Keychain.",
            ));
        }
        let original = record.clone();
        let previous = record.credential_ref.clone();
        let reference = if save {
            let reference = uuid::Uuid::new_v4().to_string();
            self.credentials.set(
                &reference,
                passphrase
                    .as_deref()
                    .map(|v| v.as_str())
                    .unwrap_or_default(),
            )?;
            Some(reference)
        } else {
            None
        };
        record.key_path = path;
        record.credential_ref = reference.clone();
        record.revision += 1;
        if let Err(error) = self.repository.save_connection(id, &record) {
            if let Some(reference) = reference {
                self.credentials.remove(&reference)?;
            }
            return Err(error);
        }
        self.invalidate_runtime(id)?;
        if let Some(previous) = previous {
            if let Err(error) = self.credentials.remove(&previous) {
                self.repository.save_connection(id, &original)?;
                if let Some(reference) = &reference {
                    self.credentials.remove(reference)?;
                }
                return Err(error);
            }
        }
        Ok(())
    }
    pub fn unlock(
        &self,
        id: &str,
        lease: &str,
        passphrase: Zeroizing<String>,
    ) -> Result<(), AppError> {
        let _guard = self.begin(id)?;
        if passphrase.len() > 4096 {
            return Err(AppError::validation(
                "passphrase",
                "Passphrase is too long.",
            ));
        }
        let runtime = self.runtime(id, lease)?;
        lock(&runtime)?.passphrase = Some(passphrase);
        Ok(())
    }
    pub fn invalidate_runtime(&self, id: &str) -> Result<(), AppError> {
        let runtime = lock(&self.runtimes)?.get(id).cloned();
        if let Some(runtime) = runtime {
            let mut runtime = lock(&runtime)?;
            runtime.session = None;
            runtime.passphrase = None;
        }
        lock(&self.observations)?.retain(|_, observation| observation.server.id != id);
        Ok(())
    }
    pub fn prepare_edit(&self, id: &str, target_changed: bool) -> Result<(), AppError> {
        self.invalidate_runtime(id)?;
        let mut record = self.repository.connection(id)?;
        record.revision += 1;
        if target_changed {
            record.trusted_key = None;
            record.trusted_algorithm = None;
            record.last_test = None;
            record.last_success_at = None;
            record.info = None;
            record.docker = None;
        }
        self.repository.save_connection(id, &record)
    }
    pub fn prepare_delete(&self, id: &str) -> Result<(), AppError> {
        let record = self.repository.connection(id)?;
        if let Some(reference) = record.credential_ref {
            self.credentials.remove(&reference)?;
        }
        self.invalidate_runtime(id)?;
        lock(&self.runtimes)?.remove(id);
        Ok(())
    }
    fn observe(
        &self,
        server: &Server,
        record: &ConnectionRecord,
        key: ssh::ObservedKey,
    ) -> Result<HostKeyObservation, AppError> {
        let dto = HostKeyObservation {
            id: uuid::Uuid::new_v4().to_string(),
            host: server.host.clone(),
            port: server.port,
            algorithm: discovery::clean(&key.algorithm),
            fingerprint: ssh::fingerprint(&key.bytes),
            previous_fingerprint: record.trusted_key.as_deref().map(ssh::fingerprint),
        };
        let mut observations = lock(&self.observations)?;
        observations.retain(|_, o| o.server.id != server.id && o.expires > Instant::now());
        if observations.len() >= 128 {
            return Err(busy());
        }
        observations.insert(
            dto.id.clone(),
            Observation {
                dto: dto.clone(),
                key: key.bytes,
                server: server.clone(),
                revision: record.revision,
                expires: Instant::now() + Duration::from_secs(300),
            },
        );
        Ok(dto)
    }
    pub fn trust(&self, id: &str, observation_id: &str, replace: bool) -> Result<(), AppError> {
        let _guard = self.begin(id)?;
        validate_id(observation_id)?;
        let observation = lock(&self.observations)?
            .remove(observation_id)
            .ok_or_else(|| {
                AppError::new(
                    ErrorCode::ObservationExpired,
                    "Host-key observation expired. Test the connection again.",
                )
            })?;
        let server = self.repository.get(id)?;
        let mut record = self.repository.connection(id)?;
        if observation.expires <= Instant::now()
            || observation.server.id != id
            || observation.server.host != server.host
            || observation.server.port != server.port
            || observation.server.username != server.username
            || record.revision != observation.revision
        {
            return Err(AppError::new(
                ErrorCode::ObservationExpired,
                "Server configuration or observed key changed. Test the connection again.",
            ));
        }
        if record.trusted_key.is_some() && !replace {
            return Err(ssh::error(ErrorCode::HostKeyMismatch));
        }
        if record.trusted_key.is_none() && replace {
            return Err(AppError::validation(
                "replace",
                "Use first-use trust for this server.",
            ));
        }
        record.trusted_key = Some(observation.key);
        record.trusted_algorithm = Some(observation.dto.algorithm);
        record.revision += 1;
        self.repository.save_connection(id, &record)?;
        self.invalidate_runtime(id)
    }
    fn connect(
        &self,
        server: &Server,
        record: &ConnectionRecord,
        runtime: &mut Runtime,
        token: &CancellationToken,
    ) -> Result<Option<HostKeyObservation>, AppError> {
        crate::domain::ServerInput {
            name: server.name.clone(),
            host: server.host.clone(),
            port: server.port,
            username: server.username.clone(),
            notes: server.notes.clone(),
        }
        .validate()?;
        if runtime.session.is_some() {
            return Ok(None);
        }
        let assigned = self
            .keys
            .as_ref()
            .map(|keys| keys.assigned(&server.id))
            .transpose()?
            .flatten();
        let resolved = assigned
            .as_deref()
            .map(|id| {
                self.keys
                    .as_ref()
                    .ok_or_else(|| AppError::new(ErrorCode::Internal, "Key service unavailable."))?
                    .resolve(id)
            })
            .transpose()?;
        let path = if let Some((path, _, _)) = &resolved {
            validate_key_path(&path.to_string_lossy())?
        } else {
            validate_key_path(
                record
                    .key_path
                    .as_deref()
                    .ok_or_else(|| AppError::validation("keyPath", "Select an SSH key first."))?,
            )?
        };
        let credential_ref = resolved
            .as_ref()
            .and_then(|(_, r, _)| r.as_ref())
            .or_else(|| {
                if assigned.is_none() {
                    record.credential_ref.as_ref()
                } else {
                    None
                }
            });
        let saved = credential_ref
            .map(|r| r.as_str())
            .map(|r| self.credentials.get(r))
            .transpose()?
            .flatten();
        if credential_ref.is_some() && saved.is_none() && runtime.passphrase.is_none() {
            return Err(AppError::new(
                ErrorCode::PassphraseRequired,
                "The saved passphrase is missing from Keychain. Enter it again.",
            ));
        }
        let passphrase = runtime
            .passphrase
            .as_deref()
            .map(|v| v.as_str())
            .or(saved.as_deref().map(|v| v.as_str()));
        if let Some((_, _, expected)) = &resolved {
            crate::infrastructure::key_files::check_permissions(&path)?;
            let contents = Zeroizing::new(
                std::fs::read_to_string(&path)
                    .map_err(|_| ssh::error(ErrorCode::KeyFileUnavailable))?,
            );
            let parsed = ssh_key::PrivateKey::from_openssh(contents.as_str())
                .map_err(|_| ssh::error(ErrorCode::InvalidPrivateKey))?;
            if parsed
                .public_key()
                .fingerprint(ssh_key::HashAlg::Sha256)
                .to_string()
                != *expected
            {
                return Err(ssh::error(ErrorCode::InvalidPrivateKey));
            }
        }
        match self.transport.connect(
            server,
            &path,
            passphrase,
            record.trusted_key.as_deref(),
            token,
        ) {
            Ok(session) => {
                runtime.session = Some(session);
                Ok(None)
            }
            Err(ConnectFailure::HostKey(key, _mismatch)) => {
                self.observe(server, record, key).map(Some)
            }
            Err(ConnectFailure::Error(error)) => Err(error),
        }
    }
    pub fn test(&self, id: &str, lease: Option<&str>) -> Result<ConnectionTestResult, AppError> {
        let guard = self.begin(id)?;
        let server = self.repository.get(id)?;
        let mut record = self.repository.connection(id)?;
        let temporary = Arc::new(Mutex::new(Runtime {
            server_id: id.into(),
            lease: String::new(),
            touched: Instant::now(),
            session: None,
            passphrase: None,
        }));
        let runtime = if let Some(lease) = lease {
            self.runtime(id, lease)?
        } else {
            lock(&self.runtimes)?.get(id).cloned().unwrap_or(temporary)
        };
        let mut runtime = lock(&runtime)?;
        // A connection test measures a new handshake and authentication, not a cached session.
        runtime.session = None;
        let started = Instant::now();
        let outcome = self
            .connect(&server, &record, &mut runtime, &guard.token)
            .and_then(|observation| {
                if let Some(observation) = observation {
                    return Ok(Some(observation));
                }
                let output = runtime
                    .session
                    .as_mut()
                    .ok_or_else(|| ssh::error(ErrorCode::Internal))?
                    .execute(
                        RemoteCommand::Probe,
                        &guard.token,
                        Instant::now() + Duration::from_secs(10),
                    )?;
                if output.exit_code != 0 || output.stdout != "remotedock" {
                    return Err(ssh::error(ErrorCode::Internal));
                }
                Ok(None)
            });
        let (status, message, error_code, observation, latency_ms) = match outcome {
            Ok(None) => (
                ConnectionStatus::Connected,
                "SSH authentication and read-only probe succeeded.".into(),
                None,
                None,
                Some(started.elapsed().as_secs_f64() * 1000.0),
            ),
            Ok(Some(observation)) => {
                let mismatch = observation.previous_fingerprint.is_some();
                let code = if mismatch {
                    ErrorCode::HostKeyMismatch
                } else {
                    ErrorCode::HostKeyUntrusted
                };
                let err = ssh::error(code);
                (
                    if mismatch {
                        ConnectionStatus::HostKeyMismatch
                    } else {
                        ConnectionStatus::HostKeyUntrusted
                    },
                    err.message,
                    Some(
                        if mismatch {
                            "HOST_KEY_MISMATCH"
                        } else {
                            "HOST_KEY_UNTRUSTED"
                        }
                        .into(),
                    ),
                    Some(observation),
                    None,
                )
            }
            Err(error) => {
                runtime.session = None;
                let status = match error.code {
                    ErrorCode::AuthenticationFailed
                    | ErrorCode::PassphraseRequired
                    | ErrorCode::InvalidPrivateKey => ConnectionStatus::AuthenticationFailed,
                    ErrorCode::Timeout => ConnectionStatus::Timeout,
                    ErrorCode::Unreachable => ConnectionStatus::Unreachable,
                    ErrorCode::Cancelled => ConnectionStatus::Disconnected,
                    _ => ConnectionStatus::Error,
                };
                let code = serde_json::to_value(&error.code)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned));
                (status, error.message, code, None, None)
            }
        };
        let result = ConnectionTestResult {
            status,
            message,
            error_code,
            observation,
            latency_ms,
            tested_at: discovery::now(),
        };
        if result.status == ConnectionStatus::Connected {
            record.last_success_at = Some(result.tested_at.clone());
        }
        let mut historical = result.clone();
        historical.observation = None;
        record.last_test = Some(historical);
        self.repository.save_connection(id, &record)?;
        Ok(result)
    }
    fn with_session<T>(
        &self,
        id: &str,
        lease: &str,
        action: impl FnOnce(&mut dyn RemoteSession, &CancellationToken, Instant) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let server = self.repository.get(id)?;
        let record = self.repository.connection(id)?;
        let runtime = self.runtime(id, lease)?;
        let mut runtime = lock(&runtime)?;
        let token = lock(&self.active)?.get(id).cloned().ok_or_else(busy)?;
        if let Some(observation) = self.connect(&server, &record, &mut runtime, &token)? {
            return Err(ssh::error(if observation.previous_fingerprint.is_some() {
                ErrorCode::HostKeyMismatch
            } else {
                ErrorCode::HostKeyUntrusted
            }));
        }
        let result = action(
            runtime
                .session
                .as_mut()
                .ok_or_else(|| ssh::error(ErrorCode::Internal))?
                .as_mut(),
            &token,
            Instant::now() + Duration::from_secs(45),
        );
        if result.is_err() {
            runtime.session = None;
        }
        runtime.touched = Instant::now();
        result
    }
    pub fn info(&self, id: &str, lease: &str) -> Result<ServerInfo, AppError> {
        let _guard = self.begin(id)?;
        let info = self.with_session(id, lease, discovery::info)?;
        let mut record = self.repository.connection(id)?;
        record.info = Some(info.clone());
        self.repository.save_connection(id, &record)?;
        Ok(info)
    }
    pub fn docker(&self, id: &str, lease: &str) -> Result<DockerCapabilities, AppError> {
        let _guard = self.begin(id)?;
        let docker = self.with_session(id, lease, discovery::docker)?;
        let mut record = self.repository.connection(id)?;
        record.docker = Some(docker.clone());
        self.repository.save_connection(id, &record)?;
        Ok(docker)
    }
    pub fn metrics(&self, id: &str, lease: &str) -> Result<ServerMetrics, AppError> {
        let _guard = self.begin(id)?;
        self.with_session(id, lease, discovery::metrics)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::ServerInput,
        infrastructure::{
            persistence::SqliteServerRepository,
            ssh::{ExecuteOutput, ObservedKey},
        },
    };
    struct NoSecrets;
    impl CredentialStore for NoSecrets {
        fn get(&self, _: &str) -> Result<Option<Zeroizing<String>>, AppError> {
            Ok(None)
        }
        fn set(&self, _: &str, _: &str) -> Result<(), AppError> {
            Err(AppError::new(
                ErrorCode::CredentialStoreUnavailable,
                "Unavailable",
            ))
        }
        fn remove(&self, _: &str) -> Result<(), AppError> {
            Ok(())
        }
    }
    struct Transport {
        key: Vec<u8>,
    }
    struct Session;
    impl RemoteSession for Session {
        fn execute(
            &mut self,
            _: RemoteCommand,
            token: &CancellationToken,
            deadline: Instant,
        ) -> Result<ExecuteOutput, AppError> {
            token.check(deadline)?;
            Ok(ExecuteOutput {
                stdout: "remotedock".into(),
                stderr: String::new(),
                exit_code: 0,
            })
        }
    }
    impl SshTransport for Transport {
        fn connect(
            &self,
            _: &Server,
            _: &std::path::Path,
            _: Option<&str>,
            trusted: Option<&[u8]>,
            _: &CancellationToken,
        ) -> Result<Box<dyn RemoteSession>, ConnectFailure> {
            if trusted == Some(&self.key) {
                Ok(Box::new(Session))
            } else {
                Err(ConnectFailure::HostKey(
                    ObservedKey {
                        bytes: self.key.clone(),
                        algorithm: "ssh-ed25519".into(),
                    },
                    trusted.is_some(),
                ))
            }
        }
    }
    fn fixture() -> (
        ConnectionService,
        Arc<SqliteServerRepository>,
        String,
        tempfile::NamedTempFile,
    ) {
        let repository =
            Arc::new(SqliteServerRepository::open(std::path::Path::new(":memory:")).unwrap());
        let server = repository
            .create(&ServerInput {
                name: "Test".into(),
                host: "localhost".into(),
                port: 22,
                username: "ubuntu".into(),
                notes: None,
            })
            .unwrap();
        let file = tempfile::NamedTempFile::new().unwrap();
        let service = ConnectionService::new(
            repository.clone(),
            Arc::new(Transport {
                key: b"exact key".to_vec(),
            }),
            Arc::new(NoSecrets),
        );
        service
            .configure(
                &server.id,
                Some(file.path().to_string_lossy().into()),
                None,
                false,
            )
            .unwrap();
        (service, repository, server.id, file)
    }
    #[test]
    fn unknown_keys_require_exact_single_use_observation_before_success() {
        let (service, _, id, _file) = fixture();
        let result = service.test(&id, None).unwrap();
        assert_eq!(result.status, ConnectionStatus::HostKeyUntrusted);
        assert!(result.latency_ms.is_none());
        let observation = result.observation.unwrap();
        service.trust(&id, &observation.id, false).unwrap();
        assert!(service.trust(&id, &observation.id, false).is_err());
        let result = service.test(&id, None).unwrap();
        assert_eq!(result.status, ConnectionStatus::Connected);
        assert!(result.latency_ms.is_some());
        assert!(service.metadata(&id).unwrap().last_success_at.is_some());
    }
    #[test]
    fn expired_and_changed_configuration_observations_are_rejected() {
        let (service, repo, id, _file) = fixture();
        let observation = service.test(&id, None).unwrap().observation.unwrap();
        service
            .observations
            .lock()
            .unwrap()
            .get_mut(&observation.id)
            .unwrap()
            .expires = Instant::now();
        assert_eq!(
            service.trust(&id, &observation.id, false).unwrap_err().code,
            ErrorCode::ObservationExpired
        );
        let observation = service.test(&id, None).unwrap().observation.unwrap();
        let mut record = repo.connection(&id).unwrap();
        record.revision += 1;
        repo.save_connection(&id, &record).unwrap();
        assert!(service.trust(&id, &observation.id, false).is_err());
    }
    #[test]
    fn changed_key_requires_explicit_replacement() {
        let (service, repo, id, _file) = fixture();
        let mut record = repo.connection(&id).unwrap();
        record.trusted_key = Some(b"old".to_vec());
        repo.save_connection(&id, &record).unwrap();
        let result = service.test(&id, None).unwrap();
        assert_eq!(result.status, ConnectionStatus::HostKeyMismatch);
        assert!(service
            .trust(&id, &result.observation.unwrap().id, false)
            .is_err());
        let observation = service.test(&id, None).unwrap().observation.unwrap();
        service.trust(&id, &observation.id, true).unwrap();
        assert_eq!(
            service.test(&id, None).unwrap().status,
            ConnectionStatus::Connected
        );
    }
    #[test]
    fn operations_are_serialized_cancelled_and_leases_are_released() {
        let (service, _, id, _file) = fixture();
        let guard = service.begin(&id).unwrap();
        assert_eq!(service.begin(&id).err().unwrap().code, ErrorCode::Busy);
        service.cancel(&id).unwrap();
        assert_eq!(
            guard
                .token
                .check(Instant::now() + Duration::from_secs(1))
                .unwrap_err()
                .code,
            ErrorCode::Cancelled
        );
        drop(guard);
        let lease = service.open(&id).unwrap();
        service
            .unlock(&id, &lease, Zeroizing::new("temporary".into()))
            .unwrap();
        service.close(&id, &lease).unwrap();
        assert!(service.runtime(&id, &lease).is_err());
    }
    #[test]
    fn unavailable_keychain_never_saves_plaintext() {
        let (service, repo, id, file) = fixture();
        let error = service
            .configure(
                &id,
                Some(file.path().to_string_lossy().into()),
                Some(Zeroizing::new("sensitive".into())),
                true,
            )
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::CredentialStoreUnavailable);
        let record = repo.connection(&id).unwrap();
        assert!(record.credential_ref.is_none());
        assert!(!serde_json::to_string(&record)
            .unwrap()
            .contains("sensitive"));
    }
    #[test]
    fn assigning_a_key_invalidates_sessions_and_keeps_host_trust() {
        let (mut service, repo, id, _legacy) = fixture();
        let directory = tempfile::tempdir().unwrap();
        let keys = Arc::new(crate::application::keys::KeyService::new(
            repo.clone(),
            Arc::new(NoSecrets),
            directory.path().join("keys"),
        ));
        let key = keys
            .create("Reusable", false, None, true, None, false)
            .unwrap();
        service = service.with_keys(keys);
        let mut record = repo.connection(&id).unwrap();
        record.trusted_key = Some(b"exact key".to_vec());
        repo.save_connection(&id, &record).unwrap();
        let lease = service.open(&id).unwrap();
        let runtime = service.runtime(&id, &lease).unwrap();
        {
            let mut r = runtime.lock().unwrap();
            r.session = Some(Box::new(Session));
            r.passphrase = Some(Zeroizing::new("transient".into()));
        }
        service.assign_key(&id, Some(&key.id)).unwrap();
        {
            let r = runtime.lock().unwrap();
            assert!(r.session.is_none());
            assert!(r.passphrase.is_none());
        }
        assert_eq!(
            repo.connection(&id).unwrap().trusted_key,
            record.trusted_key
        );
    }
}
