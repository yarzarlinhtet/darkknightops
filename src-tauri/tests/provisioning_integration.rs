//! Mutating integration tests: explicitly opted in, pinned disposable loopback VM only.
use remotedock::{
    application::{
        connection::ConnectionService, provisioning::ProvisioningService, ServerRepository,
    },
    domain::{
        provisioning::{RunStatus, StepStatus},
        ServerInput,
    },
    infrastructure::{
        credentials::OsCredentialStore,
        persistence::SqliteServerRepository,
        provisioning::{Command, Operation},
        ssh::{CancellationToken, RealSshTransport, RemoteCommand, SshTransport},
    },
};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

#[test]
#[ignore = "Mutates an explicitly prepared disposable Ubuntu 24.04 systemd VM"]
fn disposable_ubuntu_install_verify_and_repeat() {
    assert_eq!(
        std::env::var("REMOTEDOCK_PROVISIONING_FIXTURE").as_deref(),
        Ok("disposable-ubuntu-24.04"),
        "Explicit fixture opt-in is required"
    );
    let directory = tempfile::tempdir().unwrap();
    let repository =
        Arc::new(SqliteServerRepository::open(&directory.path().join("fixture.sqlite")).unwrap());
    let server = repository
        .create(&ServerInput {
            name: "Disposable provisioning fixture".into(),
            host: "127.0.0.1".into(),
            port: 22223,
            username: "tester".into(),
            notes: None,
        })
        .unwrap();
    let key = Path::new(env!("CARGO_MANIFEST_DIR")).join("../.provisioning-test/id_ed25519");
    let host_public = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../.provisioning-test/ssh_host_ed25519_key.pub"),
    )
    .unwrap();
    let trusted = ssh_key::PublicKey::from_openssh(&host_public)
        .unwrap()
        .to_bytes()
        .unwrap();
    let mut record = repository.connection(&server.id).unwrap();
    record.key_path = Some(key.to_string_lossy().into_owned());
    record.trusted_key = Some(trusted.clone());
    repository.save_connection(&server.id, &record).unwrap();
    let mut probe = RealSshTransport
        .connect(
            &server,
            &key,
            None,
            Some(&trusted),
            &CancellationToken::default(),
        )
        .unwrap_or_else(|_| panic!("Pinned disposable fixture connection failed"));
    let marker = probe
        .execute(
            RemoteCommand::Provisioning(Command {
                operation: Operation::FixtureProbe,
                run: uuid::Uuid::nil(),
                step: 0,
                root: false,
            }),
            &CancellationToken::default(),
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
    assert_eq!(marker.exit_code, 0);
    assert_eq!(marker.stdout.trim(), "remotedock-milestone-3");
    let connections = Arc::new(ConnectionService::new(
        repository.clone(),
        Arc::new(RealSshTransport),
        Arc::new(OsCredentialStore),
    ));
    let lease = connections.open(&server.id).unwrap();
    let service = Arc::new(ProvisioningService::new(connections, repository).unwrap());
    for iteration in 0..2 {
        let plan = service.plan(&server.id, &lease, false).unwrap();
        assert!(
            !plan
                .steps
                .iter()
                .any(|s| matches!(s.status, StepStatus::Blocked | StepStatus::Unsupported)),
            "Blocked fixture preflight: {:?}",
            plan.steps
        );
        if iteration == 1 {
            assert!(plan
                .steps
                .iter()
                .filter(|s| s.key != "verify")
                .all(|s| s.status == StepStatus::Satisfied));
        }
        let run = service
            .start(
                server.id.clone(),
                lease.clone(),
                plan.id,
                true,
                false,
                Arc::new(|_| {}),
            )
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2400);
        loop {
            let snapshot = service.get(&server.id, &run.id).unwrap();
            if snapshot.status != RunStatus::Running {
                assert_eq!(
                    snapshot.status,
                    RunStatus::Succeeded,
                    "{:?}",
                    snapshot.error_message
                );
                let verified = snapshot.verification.unwrap();
                assert!(verified.facts.service_enabled && verified.facts.service_running);
                assert!(!verified.server_version.is_empty());
                assert!(!verified.compose_version.is_empty());
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Fixture monitoring deadline exceeded"
            );
            std::thread::sleep(Duration::from_secs(1));
        }
    }
}
