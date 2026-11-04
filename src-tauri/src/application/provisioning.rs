use super::connection::{ConnectionService, ProvisioningSession};
use crate::{
    domain::{provisioning::*, validate_id, AppError, ErrorCode},
    infrastructure::{
        discovery::now,
        provisioning::{self, Command, Operation},
        ssh::{CancellationToken, RemoteCommand},
    },
};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

pub trait ProvisioningRepository: Send + Sync {
    fn save_run(&self, run: &ProvisioningRun) -> Result<(), AppError>;
    fn get_run(&self, server: &str, id: &str) -> Result<ProvisioningRun, AppError>;
    fn list_runs(&self, server: &str) -> Result<Vec<ProvisioningRun>, AppError>;
    fn recover(&self) -> Result<(), AppError>;
}
struct Pending {
    plan: ProvisioningPlan,
    identity: String,
    expires: Instant,
}
pub type Progress = Arc<dyn Fn(&ProvisioningRun) + Send + Sync>;
pub struct ProvisioningService {
    connections: Arc<ConnectionService>,
    repository: Arc<dyn ProvisioningRepository>,
    plans: Mutex<HashMap<String, Pending>>,
    active: Mutex<HashMap<String, Arc<AtomicBool>>>,
}
fn internal() -> AppError {
    AppError::new(ErrorCode::Internal, "Provisioning state unavailable.")
}
fn blocked(message: &str) -> AppError {
    AppError::new(ErrorCode::ProvisioningBlocked, message)
}
fn stale() -> AppError {
    AppError::new(
        ErrorCode::PlanStale,
        "Approval expired or server state changed. Generate and review a fresh plan.",
    )
}
fn material(preflight: &Preflight, identity: &str, grant: bool) -> Result<String, AppError> {
    let mut facts = preflight.facts.clone();
    facts.available_bytes = u64::from(facts.available_bytes >= MINIMUM_INSTALL_BYTES);
    let bytes = serde_json::to_vec(&(
        1,
        identity,
        grant,
        facts,
        &preflight.privilege,
        &preflight.username,
        &preflight.docker_access,
        preflight.docker_group,
        preflight.pending_login,
        &preflight.client_version,
        &preflight.server_version,
        &preflight.compose_version,
        preflight.workloads,
        &preflight.checks,
    ))
    .map_err(|_| internal())?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
impl ProvisioningService {
    pub fn new(
        connections: Arc<ConnectionService>,
        repository: Arc<dyn ProvisioningRepository>,
    ) -> Result<Self, AppError> {
        repository.recover()?;
        Ok(Self {
            connections,
            repository,
            plans: Mutex::new(HashMap::new()),
            active: Mutex::new(HashMap::new()),
        })
    }
    pub fn preflight(&self, id: &str, lease: &str) -> Result<Preflight, AppError> {
        let _guard = self.connections.begin(id)?;
        let mut worker = self.connections.provisioning_session(id, lease)?;
        provisioning::preflight::inspect(worker.session()?)
    }
    pub fn plan(&self, id: &str, lease: &str, grant: bool) -> Result<ProvisioningPlan, AppError> {
        let _guard = self.connections.begin(id)?;
        let mut worker = self.connections.provisioning_session(id, lease)?;
        let preflight = provisioning::preflight::inspect(worker.session()?)?;
        let identity = self.connections.provisioning_identity(id)?;
        let plan_hash = material(&preflight, &identity, grant)?;
        let mut steps = preflight.checks.clone();
        if grant {
            steps.retain(|s| {
                matches!(
                    s.key.as_str(),
                    "compatibility" | "system" | "privileges" | "docker_detection"
                )
            });
            steps.push(PlanStep {
                key: "grant".into(),
                status: if !preflight.facts.engine_installed
                    || !preflight.facts.administrative_access
                {
                    StepStatus::Blocked
                } else if preflight.docker_access == "direct" {
                    StepStatus::Satisfied
                } else {
                    StepStatus::Required
                },
                description:
                    "Grant root-equivalent Docker group access to the authenticated SSH user."
                        .into(),
            });
        } else {
            let index = steps
                .iter()
                .position(|s| s.key == "repository")
                .unwrap_or(steps.len());
            let needs_packages = !preflight.facts.engine_installed
                || !preflight.facts.compose_installed
                || preflight.facts.additional_packages_required;
            steps.insert(
                index,
                PlanStep {
                    key: "bootstrap".into(),
                    status: if !needs_packages {
                        StepStatus::Satisfied
                    } else if steps
                        .iter()
                        .any(|s| matches!(s.status, StepStatus::Blocked | StepStatus::Unsupported))
                    {
                        StepStatus::Blocked
                    } else {
                        StepStatus::Required
                    },
                    description:
                        "Install missing CA certificates, curl and GnuPG; refresh apt metadata."
                            .into(),
                },
            );
        }
        steps.push(PlanStep {
            key: "verify".into(),
            status: StepStatus::Required,
            description: "Verify actual Docker Engine, Compose, service and access state.".into(),
        });
        let plan=ProvisioningPlan {id:uuid::Uuid::new_v4().to_string(), server_id:id.into(), plan_hash,expires_at:(chrono::Utc::now()+chrono::Duration::minutes(5)).to_rfc3339(),preflight,steps,grant_access:grant,warnings:vec!["Docker daemon and Docker group access are root-equivalent.".into(),"Package installation may start Docker and alter host networking; published ports may bypass host firewall rules.".into(),"Cancellation waits for an active package operation. Package changes are not rolled back.".into(),"Conflicts, custom repositories and upgrades require manual remediation.".into()]};
        let mut plans = self.plans.lock().map_err(|_| internal())?;
        plans.retain(|_, p| p.expires > Instant::now());
        if plans.len() >= 128 {
            return Err(blocked(
                "Too many pending plans. Wait five minutes and retry.",
            ));
        }
        plans.insert(
            plan.id.clone(),
            Pending {
                plan: plan.clone(),
                identity,
                expires: Instant::now() + Duration::from_secs(300),
            },
        );
        Ok(plan)
    }
    pub fn get(&self, id: &str, run: &str) -> Result<ProvisioningRun, AppError> {
        validate_id(id)?;
        validate_id(run)?;
        self.repository.get_run(id, run)
    }
    pub fn list(&self, id: &str) -> Result<Vec<ProvisioningRun>, AppError> {
        validate_id(id)?;
        self.repository.list_runs(id)
    }
    pub fn cancel(&self, id: &str, run_id: &str) -> Result<(), AppError> {
        self.get(id, run_id)?;
        let active = self.active.lock().map_err(|_| internal())?;
        if let Some(token) = active.get(run_id) {
            token.store(true, Ordering::SeqCst);
            // Only the worker writes snapshots, preventing stale cancellation
            // requests from overwriting a completed step.
        }
        Ok(())
    }
    pub fn start(
        self: &Arc<Self>,
        id: String,
        lease: String,
        plan_id: String,
        confirmed: bool,
        grant: bool,
        progress: Progress,
    ) -> Result<ProvisioningRun, AppError> {
        validate_id(&id)?;
        validate_id(&lease)?;
        validate_id(&plan_id)?;
        if !confirmed {
            return Err(blocked("Explicit plan approval is required."));
        }
        let service = self.clone();
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let result = (|| {
                let _guard = service.connections.begin(&id)?;
                let pending = service
                    .plans
                    .lock()
                    .map_err(|_| internal())?
                    .remove(&plan_id)
                    .ok_or_else(stale)?;
                if pending.plan.server_id != id
                    || pending.expires <= Instant::now()
                    || pending.plan.grant_access != grant
                {
                    return Err(stale());
                }
                if pending
                    .plan
                    .steps
                    .iter()
                    .any(|s| matches!(s.status, StepStatus::Blocked | StepStatus::Unsupported))
                {
                    return Err(blocked(
                        "Resolve all blocked preflight checks before provisioning.",
                    ));
                }
                let mut worker = service.connections.provisioning_session(&id, &lease)?;
                let fresh = provisioning::preflight::inspect(worker.session()?)?;
                let identity = service.connections.provisioning_identity(&id)?;
                if identity != pending.identity
                    || material(&fresh, &identity, grant)? != pending.plan.plan_hash
                {
                    return Err(stale());
                }
                let mut run = ProvisioningRun {
                    approved_plan: Some(pending.plan.clone()),
                    logs: Vec::new(),
                    id: uuid::Uuid::new_v4().to_string(),
                    server_id: id.clone(),
                    plan_hash: pending.plan.plan_hash,
                    status: RunStatus::Running,
                    started_at: now(),
                    finished_at: None,
                    error_code: None,
                    error_message: None,
                    cancellation_requested: false,
                    sequence: 0,
                    steps: pending
                        .plan
                        .steps
                        .iter()
                        .map(|s| RunStep {
                            key: s.key.clone(),
                            description: s.description.clone(),
                            status: if s.status == StepStatus::Satisfied {
                                "SKIPPED"
                            } else {
                                "PENDING"
                            }
                            .into(),
                            started_at: None,
                            finished_at: None,
                            error_code: None,
                            sanitized_output: String::new(),
                        })
                        .collect(),
                    verification: None,
                };
                let cancel = Arc::new(AtomicBool::new(false));
                service.repository.save_run(&run)?;
                service
                    .active
                    .lock()
                    .map_err(|_| internal())?
                    .insert(run.id.clone(), cancel.clone());
                let _ = tx.send(Ok(run.clone()));
                progress(&run);
                let outcome = service.execute(
                    &mut run,
                    &mut worker,
                    &cancel,
                    &progress,
                    grant,
                    fresh.privilege == "root",
                );
                run.cancellation_requested = cancel.load(Ordering::SeqCst);
                if let Err(error) = outcome {
                    run.status = if matches!(
                        error.code,
                        ErrorCode::Timeout
                            | ErrorCode::ProvisioningUnknown
                            | ErrorCode::Unreachable
                            | ErrorCode::Internal
                            | ErrorCode::OutputLimit
                    ) {
                        RunStatus::Interrupted
                    } else if run.steps.iter().any(|s| s.status == "SUCCEEDED") {
                        RunStatus::PartiallyCompleted
                    } else {
                        RunStatus::Failed
                    };
                    run.error_code = serde_json::to_value(&error.code)
                        .ok()
                        .and_then(|value| value.as_str().map(str::to_owned));
                    run.error_message = Some(error.message);
                    if let Some(step) = run.steps.iter_mut().find(|s| s.status == "RUNNING") {
                        step.status = if run.status == RunStatus::Interrupted {
                            "UNKNOWN"
                        } else {
                            "FAILED"
                        }
                        .into();
                        step.finished_at = Some(now());
                        step.error_code = run.error_code.clone();
                    }
                }
                run.finished_at = Some(now());
                for step in &run.steps {
                    for line in step
                        .sanitized_output
                        .lines()
                        .filter(|line| !line.is_empty())
                    {
                        if !run
                            .logs
                            .iter()
                            .any(|entry| entry.step_key == step.key && entry.message == line)
                        {
                            run.logs.push(ProvisioningLog {
                                id: uuid::Uuid::new_v4().to_string(),
                                step_key: step.key.clone(),
                                observed_at: now(),
                                level: "INFO".into(),
                                message: line.into(),
                            });
                        }
                    }
                }
                run.sequence += 1;
                // Never emit success until its durable snapshot has been saved.
                if service.repository.save_run(&run).is_err() {
                    run.status = RunStatus::Interrupted;
                    run.error_code = Some("DATABASE_ERROR".into());
                    run.error_message=Some("Local progress could not be saved. Remote state is unknown; restart RemoteDock and run fresh preflight before retrying.".into());
                }
                progress(&run);
                if let Ok(mut active) = service.active.lock() {
                    active.remove(&run.id);
                }
                Ok(())
            })();
            if let Err(error) = result {
                let _ = tx.send(Err(error));
            }
        });
        rx.recv().map_err(|_| internal())?
    }
    fn persist(&self, run: &mut ProvisioningRun, progress: &Progress) -> Result<(), AppError> {
        for step in &run.steps {
            for line in step
                .sanitized_output
                .lines()
                .filter(|line| !line.is_empty())
            {
                if !run
                    .logs
                    .iter()
                    .any(|entry| entry.step_key == step.key && entry.message == line)
                {
                    run.logs.push(ProvisioningLog {
                        id: uuid::Uuid::new_v4().to_string(),
                        step_key: step.key.clone(),
                        observed_at: now(),
                        level: "INFO".into(),
                        message: line.into(),
                    });
                }
            }
        }
        run.sequence += 1;
        self.repository.save_run(run)?;
        progress(run);
        Ok(())
    }
    fn execute(
        &self,
        run: &mut ProvisioningRun,
        worker: &mut ProvisioningSession,
        cancel: &AtomicBool,
        progress: &Progress,
        grant: bool,
        root: bool,
    ) -> Result<(), AppError> {
        let run_uuid = uuid::Uuid::parse_str(&run.id).map_err(|_| internal())?;
        for index in 0..run.steps.len() {
            if run.steps[index].status == "SKIPPED" {
                continue;
            }
            if cancel.load(Ordering::SeqCst) {
                run.status = if run.steps.iter().any(|s| s.status == "SUCCEEDED") {
                    RunStatus::PartiallyCompleted
                } else {
                    RunStatus::Cancelled
                };
                for step in &mut run.steps[index..] {
                    if step.status == "PENDING" {
                        step.status = "CANCELLED".into();
                    }
                }
                return Ok(());
            }
            run.steps[index].status = "RUNNING".into();
            run.steps[index].started_at = Some(now());
            self.persist(run, progress)?;
            let key = run.steps[index].key.as_str();
            if key == "verify" {
                if grant {
                    self.connections
                        .reconnect_provisioning(&run.server_id, worker)?;
                }
                let verification = provisioning::preflight::inspect(worker.session()?)?;
                let good = if grant {
                    verification.docker_access == "direct"
                } else {
                    verification.facts.engine_installed
                        && verification.facts.compose_installed
                        && !verification.facts.additional_packages_required
                        && verification.facts.service_running
                        && verification.facts.service_enabled
                        && matches!(
                            verification.docker_access.as_str(),
                            "direct" | "administrative"
                        )
                };
                run.verification = Some(verification);
                if !good {
                    return Err(provisioning::failed(if grant {
                        "Group membership was requested but Docker access is not yet verified. Reconnect or start a new login, then verify again."
                    } else {
                        "Installation verification failed. Inspect service, daemon access and Compose before retrying."
                    }));
                }
            } else {
                let operation = match key {
                    "bootstrap" => Operation::Bootstrap,
                    "repository" => Operation::Repository,
                    "engine" | "compose" | "buildx" => Operation::Packages,
                    "enable" => Operation::Enable,
                    "start" => Operation::Start,
                    "grant" => Operation::Grant,
                    _ => return Err(internal()),
                };
                // Recheck remote facts before every new mutation; completed local steps alone are never authoritative.
                let fresh = provisioning::preflight::inspect(worker.session()?)?;
                if !compatible(&fresh.facts)
                    || !fresh.facts.administrative_access
                    || !fresh.facts.docker_state_known
                {
                    return Err(blocked(
                        "Prerequisites changed while provisioning. Run fresh preflight.",
                    ));
                }
                let satisfied = match operation {
                    Operation::Packages => {
                        fresh.facts.engine_installed
                            && fresh.facts.compose_installed
                            && !fresh.facts.additional_packages_required
                    }
                    Operation::Repository => fresh.facts.repository_valid,
                    Operation::Enable => fresh.facts.service_enabled,
                    Operation::Start => fresh.facts.service_running,
                    Operation::Grant => fresh.docker_access == "direct",
                    _ => false,
                };
                if !satisfied {
                    if matches!(operation, Operation::Packages | Operation::Repository)
                        && (fresh.facts.conflicting_packages || fresh.facts.custom_repository)
                    {
                        return Err(blocked(
                            "Repository or package conflict detected. Resolve manually.",
                        ));
                    }
                    let command = Command {
                        operation,
                        run: run_uuid,
                        step: index as u8,
                        root,
                    };
                    let output = worker.session()?.execute(
                        RemoteCommand::Provisioning(command),
                        &CancellationToken::default(),
                        Instant::now() + Duration::from_secs(30),
                    )?;
                    if output.exit_code != 0 {
                        return Err(provisioning::failed("Remote step could not be launched. Verify passwordless permission for systemd-run and inspect the remote journal."));
                    }
                    let deadline = Instant::now() + Duration::from_secs(1800);
                    let mut tick = Instant::now();
                    loop {
                        let output = worker.session()?.execute(
                            RemoteCommand::Provisioning(Command {
                                operation: Operation::Inspect,
                                ..command
                            }),
                            &CancellationToken::default(),
                            Instant::now() + Duration::from_secs(30),
                        )?;
                        let fields: HashMap<_, _> = output
                            .stdout
                            .lines()
                            .filter_map(|l| l.split_once('='))
                            .collect();
                        if output.exit_code != 0 || fields.get("LoadState") != Some(&"loaded") {
                            return Err(AppError::new(ErrorCode::ProvisioningUnknown,"Remote operation state is unknown. Reconcile systemd and package state before retrying."));
                        }
                        if tick.elapsed() >= Duration::from_secs(5)
                            || fields.get("ActiveState") == Some(&"failed")
                            || fields.get("SubState") == Some(&"exited")
                        {
                            let logs = worker.session()?.execute(
                                RemoteCommand::Provisioning(Command {
                                    operation: Operation::Logs,
                                    ..command
                                }),
                                &CancellationToken::default(),
                                Instant::now() + Duration::from_secs(30),
                            )?;
                            let safe = provisioning::sanitized_log(&logs.stdout);
                            if !safe.is_empty() {
                                run.steps[index].sanitized_output = safe;
                            }
                        }
                        if fields.get("ActiveState") == Some(&"failed") {
                            return Err(provisioning::failed("Remote step failed. Inspect its systemd journal; no package rollback was attempted."));
                        }
                        if fields.get("ActiveState") == Some(&"active")
                            && fields.get("SubState") == Some(&"exited")
                        {
                            if fields.get("ExecMainStatus") != Some(&"0")
                                || fields.get("Result") != Some(&"success")
                            {
                                return Err(provisioning::failed(
                                    "Remote operation reported an unsuccessful exit.",
                                ));
                            }
                            break;
                        }
                        if Instant::now() >= deadline {
                            return Err(AppError::new(ErrorCode::Timeout,"Monitoring timed out. The remote package operation may still be running; cancellation did not terminate it."));
                        }
                        if tick.elapsed() >= Duration::from_secs(5) {
                            run.cancellation_requested = cancel.load(Ordering::SeqCst);
                            let phases = run.steps[index]
                                .sanitized_output
                                .lines()
                                .filter(|line| {
                                    line.starts_with("Setting up ")
                                        || line.starts_with("Unpacking ")
                                })
                                .collect::<Vec<_>>()
                                .join("\n");
                            let status = if run.cancellation_requested {
                                "Cancellation requested; waiting for the remote operation to finish safely."
                            } else {
                                "Remote operation is running."
                            };
                            run.steps[index].sanitized_output = format!("{phases}\n{status}");
                            self.persist(run, progress)?;
                            tick = Instant::now();
                        }
                        std::thread::sleep(Duration::from_millis(500));
                    }
                    run.steps[index].sanitized_output = format!(
                        "{}\n{} completed with exit status 0. Unit: {}",
                        run.steps[index].sanitized_output,
                        run.steps[index].description,
                        command.unit()
                    );
                }
            }
            run.steps[index].status = "SUCCEEDED".into();
            run.steps[index].finished_at = Some(now());
            self.persist(run, progress)?;
        }
        run.status = RunStatus::Succeeded;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::ServerRepository,
        domain::{Server, ServerInput},
        infrastructure::{
            credentials::CredentialStore,
            persistence::SqliteServerRepository,
            ssh::{ConnectFailure, ExecuteOutput, RemoteSession, SshTransport},
        },
    };
    use std::path::Path;
    use zeroize::Zeroizing;
    #[derive(Default)]
    struct Remote {
        installed: bool,
        compose: bool,
        running: bool,
        enabled: bool,
        repository: bool,
        inaccessible: bool,
        disconnect: bool,
        fail_start: bool,
        interactive: bool,
        calls: Vec<String>,
    }
    struct Transport(Arc<Mutex<Remote>>);
    struct Session(Arc<Mutex<Remote>>);
    struct Credentials;
    impl CredentialStore for Credentials {
        fn get(&self, _: &str) -> Result<Option<Zeroizing<String>>, AppError> {
            Ok(None)
        }
        fn set(&self, _: &str, _: &str) -> Result<(), AppError> {
            panic!("no secrets")
        }
        fn remove(&self, _: &str) -> Result<(), AppError> {
            Ok(())
        }
    }
    impl SshTransport for Transport {
        fn connect(
            &self,
            _: &Server,
            _: &Path,
            _: Option<&str>,
            trusted: Option<&[u8]>,
            _: &CancellationToken,
        ) -> Result<Box<dyn RemoteSession>, ConnectFailure> {
            assert_eq!(trusted, Some(&b"trusted"[..]));
            Ok(Box::new(Session(self.0.clone())))
        }
    }
    impl RemoteSession for Session {
        fn execute(
            &mut self,
            command: RemoteCommand,
            _: &CancellationToken,
            _: Instant,
        ) -> Result<ExecuteOutput, AppError> {
            let mut state = self.0.lock().unwrap();
            let stdout = match command {
                RemoteCommand::OsRelease => {
                    "ID=ubuntu\nVERSION_ID=24.04\nVERSION_CODENAME=noble\n".into()
                }
                RemoteCommand::Provisioning(command) => match command.operation {
                    Operation::Preflight => {
                        let mut result=format!("username=tester\nuid=1000\narchitecture=amd64\nsystemd=yes\napt=yes\nprivilege={}\ndns=yes\nnetwork=yes\nubuntu_network=yes\ndisk=4000000\naudit=0\nactive_units=0\n",if state.interactive {"interactive"} else {"passwordless"});
                        if state.installed {
                            result.push_str("cli=yes\nengine=yes\nknown_packages=yes\nsource=official-package\n");
                        }
                        if state.compose {
                            result.push_str("compose=2.40.0\nbuildx=yes\n");
                        }
                        if state.running {
                            result.push_str("running=active\n");
                        }
                        if state.enabled {
                            result.push_str("enabled=enabled\n");
                        }
                        result.push_str(if state.running && !state.inaccessible {
                            "access=direct\nserver_version=29.0\nworkloads=0\n"
                        } else {
                            "access=unavailable\n"
                        });
                        if state.repository {
                            result.push_str("repository_file=/etc/apt/sources.list.d/docker.sources\nrepository_line=Types: deb\nrepository_line=URIs: https://download.docker.com/linux/ubuntu\nrepository_line=Suites: noble\nrepository_line=Components: stable\nrepository_line=Architectures: amd64\nrepository_line=Signed-By: /etc/apt/keyrings/docker.asc\nkey_fingerprint=9DC858229FC7DD38854AE2D88D81803C0EBFCD88\nkey_count=1\n");
                        }
                        result
                    }
                    Operation::Inspect => {
                        if state.disconnect {
                            return Err(AppError::new(ErrorCode::Unreachable, "SSH disconnected."));
                        }
                        if state.fail_start {
                            "LoadState=loaded\nActiveState=failed\nResult=exit-code\nExecMainStatus=1\n".into()
                        } else {
                            "LoadState=loaded\nActiveState=active\nSubState=exited\nResult=success\nExecMainStatus=0\n".into()
                        }
                    }
                    Operation::Logs => "Setting up docker-ce (secret)\n".into(),
                    action => {
                        state.calls.push(format!("{action:?}"));
                        match action {
                            Operation::Repository => state.repository = true,
                            Operation::Packages => {
                                state.installed = true;
                                state.compose = true;
                            }
                            Operation::Start => state.running = true,
                            Operation::Enable => state.enabled = true,
                            Operation::Grant => state.inaccessible = false,
                            _ => (),
                        };
                        String::new()
                    }
                },
                _ => panic!("unexpected command"),
            };
            Ok(ExecuteOutput {
                stdout,
                stderr: String::new(),
                exit_code: 0,
            })
        }
    }
    struct Fixture {
        service: Arc<ProvisioningService>,
        remote: Arc<Mutex<Remote>>,
        repository: Arc<SqliteServerRepository>,
        id: String,
        lease: String,
        _key: tempfile::NamedTempFile,
        _directory: tempfile::TempDir,
    }
    fn fixture(remote: Remote) -> Fixture {
        let directory = tempfile::tempdir().unwrap();
        let repository =
            Arc::new(SqliteServerRepository::open(&directory.path().join("test.sqlite")).unwrap());
        let server = repository
            .create(&ServerInput {
                name: "test".into(),
                host: "127.0.0.1".into(),
                port: 22,
                username: "tester".into(),
                notes: None,
            })
            .unwrap();
        let key = tempfile::NamedTempFile::new().unwrap();
        let mut record = repository.connection(&server.id).unwrap();
        record.key_path = Some(key.path().to_string_lossy().into_owned());
        record.trusted_key = Some(b"trusted".to_vec());
        repository.save_connection(&server.id, &record).unwrap();
        let remote = Arc::new(Mutex::new(remote));
        let connections = Arc::new(ConnectionService::new(
            repository.clone(),
            Arc::new(Transport(remote.clone())),
            Arc::new(Credentials),
        ));
        let lease = connections.open(&server.id).unwrap();
        let service = Arc::new(ProvisioningService::new(connections, repository.clone()).unwrap());
        Fixture {
            service,
            remote,
            repository,
            id: server.id,
            lease,
            _key: key,
            _directory: directory,
        }
    }
    fn wait(f: &Fixture, id: &str) -> ProvisioningRun {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let run = f.service.get(&f.id, id).unwrap();
            if run.status != RunStatus::Running {
                return run;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    fn start(f: &Fixture) -> ProvisioningRun {
        let plan = f.service.plan(&f.id, &f.lease, false).unwrap();
        let run = f
            .service
            .start(
                f.id.clone(),
                f.lease.clone(),
                plan.id,
                true,
                false,
                Arc::new(|_| {}),
            )
            .unwrap();
        wait(f, &run.id)
    }
    #[test]
    fn fresh_install_is_verified_persisted_and_repeat_is_read_only() {
        let f = fixture(Remote::default());
        let run = start(&f);
        assert_eq!(run.status, RunStatus::Succeeded);
        assert!(run.verification.unwrap().facts.service_enabled);
        let calls = f.remote.lock().unwrap().calls.clone();
        assert!(calls.contains(&"Packages".into()));
        assert_eq!(start(&f).status, RunStatus::Succeeded);
        assert_eq!(f.remote.lock().unwrap().calls, calls);
        let reopened =
            SqliteServerRepository::open(&f._directory.path().join("test.sqlite")).unwrap();
        assert_eq!(reopened.list_runs(&f.id).unwrap().len(), 2);
    }
    #[test]
    fn approvals_require_confirmation_and_are_single_use() {
        let f = fixture(Remote::default());
        let plan = f.service.plan(&f.id, &f.lease, false).unwrap();
        assert_eq!(
            f.service
                .start(
                    f.id.clone(),
                    f.lease.clone(),
                    plan.id.clone(),
                    false,
                    false,
                    Arc::new(|_| {})
                )
                .unwrap_err()
                .code,
            ErrorCode::ProvisioningBlocked
        );
        let run = f
            .service
            .start(
                f.id.clone(),
                f.lease.clone(),
                plan.id.clone(),
                true,
                false,
                Arc::new(|_| {}),
            )
            .unwrap();
        wait(&f, &run.id);
        assert_eq!(
            f.service
                .start(
                    f.id.clone(),
                    f.lease.clone(),
                    plan.id,
                    true,
                    false,
                    Arc::new(|_| {})
                )
                .unwrap_err()
                .code,
            ErrorCode::PlanStale
        );
    }
    #[test]
    fn expired_changed_and_cross_server_approvals_are_rejected() {
        let f = fixture(Remote::default());
        let plan = f.service.plan(&f.id, &f.lease, false).unwrap();
        f.service
            .plans
            .lock()
            .unwrap()
            .get_mut(&plan.id)
            .unwrap()
            .expires = Instant::now();
        assert_eq!(
            f.service
                .start(
                    f.id.clone(),
                    f.lease.clone(),
                    plan.id,
                    true,
                    false,
                    Arc::new(|_| {})
                )
                .unwrap_err()
                .code,
            ErrorCode::PlanStale
        );
        let plan = f.service.plan(&f.id, &f.lease, false).unwrap();
        f.remote.lock().unwrap().interactive = true;
        assert_eq!(
            f.service
                .start(
                    f.id.clone(),
                    f.lease.clone(),
                    plan.id,
                    true,
                    false,
                    Arc::new(|_| {})
                )
                .unwrap_err()
                .code,
            ErrorCode::PlanStale
        );
        assert!(f.remote.lock().unwrap().calls.is_empty());
    }
    #[test]
    fn compose_missing_installs_without_restarting_engine() {
        let f = fixture(Remote {
            installed: true,
            running: true,
            enabled: true,
            ..Default::default()
        });
        assert_eq!(start(&f).status, RunStatus::Succeeded);
        let calls = &f.remote.lock().unwrap().calls;
        assert!(calls.contains(&"Packages".into()));
        assert!(!calls.contains(&"Start".into()));
    }
    #[test]
    fn inaccessible_daemon_is_never_reported_ready() {
        let f = fixture(Remote {
            installed: true,
            compose: true,
            running: true,
            enabled: true,
            inaccessible: true,
            ..Default::default()
        });
        assert_eq!(start(&f).status, RunStatus::Failed);
        assert!(f.remote.lock().unwrap().calls.is_empty());
    }
    #[test]
    fn ssh_disconnect_is_unknown_and_preserves_history() {
        let f = fixture(Remote {
            disconnect: true,
            ..Default::default()
        });
        let run = start(&f);
        assert_eq!(run.status, RunStatus::Interrupted);
        assert!(run.steps.iter().any(|s| s.status == "UNKNOWN"));
    }
    #[test]
    fn cancellation_stops_at_a_step_boundary_without_rollback() {
        let f = fixture(Remote::default());
        let service = f.service.clone();
        let server = f.id.clone();
        let callback: Progress = Arc::new(move |run| {
            if run.steps.iter().any(|s| s.status == "RUNNING") {
                service.cancel(&server, &run.id).unwrap();
            }
        });
        let plan = f.service.plan(&f.id, &f.lease, false).unwrap();
        let run = f
            .service
            .start(
                f.id.clone(),
                f.lease.clone(),
                plan.id,
                true,
                false,
                callback,
            )
            .unwrap();
        let run = wait(&f, &run.id);
        assert_eq!(run.status, RunStatus::PartiallyCompleted);
        assert!(run.cancellation_requested);
        assert_eq!(f.remote.lock().unwrap().calls, vec!["Bootstrap"]);
    }
    #[test]
    fn startup_recovery_marks_running_steps_unknown() {
        let f = fixture(Remote {
            installed: true,
            compose: true,
            running: true,
            enabled: true,
            ..Default::default()
        });
        let mut run = start(&f);
        run.status = RunStatus::Running;
        run.steps[0].status = "RUNNING".into();
        f.repository.save_run(&run).unwrap();
        f.repository.recover().unwrap();
        let recovered = f.repository.get_run(&f.id, &run.id).unwrap();
        assert_eq!(recovered.status, RunStatus::Interrupted);
        assert_eq!(recovered.steps[0].status, "UNKNOWN");
    }
    #[test]
    fn concurrent_connection_operations_block_planning() {
        let f = fixture(Remote::default());
        let _guard = f.service.connections.begin(&f.id).unwrap();
        assert_eq!(
            f.service.plan(&f.id, &f.lease, false).unwrap_err().code,
            ErrorCode::Busy
        );
    }
    #[test]
    fn group_access_requires_a_separate_approval() {
        let f = fixture(Remote {
            installed: true,
            compose: true,
            running: true,
            enabled: true,
            inaccessible: true,
            ..Default::default()
        });
        let plan = f.service.plan(&f.id, &f.lease, true).unwrap();
        assert!(plan
            .warnings
            .iter()
            .any(|warning| warning.contains("root-equivalent")));
        assert_eq!(
            f.service
                .start(
                    f.id.clone(),
                    f.lease.clone(),
                    plan.id.clone(),
                    true,
                    false,
                    Arc::new(|_| {})
                )
                .unwrap_err()
                .code,
            ErrorCode::PlanStale
        );
        let plan = f.service.plan(&f.id, &f.lease, true).unwrap();
        let run = f
            .service
            .start(
                f.id.clone(),
                f.lease.clone(),
                plan.id,
                true,
                true,
                Arc::new(|_| {}),
            )
            .unwrap();
        assert_eq!(wait(&f, &run.id).status, RunStatus::Succeeded);
        assert_eq!(f.remote.lock().unwrap().calls, vec!["Grant"]);
    }
    #[test]
    fn interrupted_attempt_retries_using_actual_remote_state() {
        let f = fixture(Remote {
            disconnect: true,
            ..Default::default()
        });
        assert_eq!(start(&f).status, RunStatus::Interrupted);
        f.remote.lock().unwrap().disconnect = false;
        assert_eq!(start(&f).status, RunStatus::Succeeded);
        assert_eq!(f.service.list(&f.id).unwrap().len(), 2);
    }
    #[test]
    fn remote_service_failure_cannot_be_reported_as_success() {
        let f = fixture(Remote {
            installed: true,
            compose: true,
            enabled: true,
            fail_start: true,
            ..Default::default()
        });
        let run = start(&f);
        assert_ne!(run.status, RunStatus::Succeeded);
        assert!(run.error_message.is_some());
    }
}
