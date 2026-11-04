//! Explicitly opted-in, loopback-only disposable SSH tests; never a production target.
use remotedock::{
    domain::{ErrorCode, Server},
    infrastructure::{
        discovery,
        ssh::{
            CancellationToken, ConnectFailure, RealSshTransport, RemoteCommand, RemoteSession,
            SshTransport,
        },
    },
};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
fn server() -> Server {
    Server {
        id: "00000000-0000-4000-8000-000000000000".into(),
        name: "Disposable".into(),
        host: "127.0.0.1".into(),
        port: 22222,
        username: "tester".into(),
        notes: None,
        created_at: String::new(),
        updated_at: String::new(),
    }
}
fn key(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../.ssh-test")
        .join(name)
}
fn observed() -> Vec<u8> {
    match RealSshTransport.connect(
        &server(),
        &key("ed25519"),
        None,
        None,
        &CancellationToken::default(),
    ) {
        Err(ConnectFailure::HostKey(key, false)) => key.bytes,
        _ => panic!("Expected an unknown host key from the disposable loopback fixture"),
    }
}
fn connect(
    name: &str,
    passphrase: Option<&str>,
    trusted: &[u8],
) -> Result<Box<dyn RemoteSession>, ConnectFailure> {
    RealSshTransport.connect(
        &server(),
        &key(name),
        passphrase,
        Some(trusted),
        &CancellationToken::default(),
    )
}
#[test]
#[ignore = "Requires explicitly started disposable SSH fixture on loopback port 22222"]
fn real_ssh_authentication_trust_discovery_and_failure_modes() {
    let trusted = observed();
    let mut session = connect("ed25519", None, &trusted)
        .unwrap_or_else(|_| panic!("Ed25519 authentication failed"));
    let output = session
        .execute(
            RemoteCommand::Probe,
            &CancellationToken::default(),
            Instant::now() + Duration::from_secs(5),
        )
        .unwrap();
    assert_eq!(output.stdout, "remotedock");
    assert_eq!(output.exit_code, 0);
    let info = discovery::info(
        session.as_mut(),
        &CancellationToken::default(),
        Instant::now() + Duration::from_secs(45),
    )
    .unwrap();
    assert!(info.provisioning_supported);
    assert!(info.memory_total_bytes > 0.0);
    let caps = discovery::docker(
        session.as_mut(),
        &CancellationToken::default(),
        Instant::now() + Duration::from_secs(45),
    )
    .unwrap();
    assert!(!caps.cli_installed);
    assert!(connect("rsa", Some("fixture-passphrase"), &trusted).is_ok());
    assert!(
        matches!(connect("rsa",Some("wrong"),&trusted),Err(ConnectFailure::Error(e)) if e.code == ErrorCode::PassphraseRequired)
    );
    assert!(
        matches!(connect("rsa",None,&trusted),Err(ConnectFailure::Error(e)) if e.code == ErrorCode::PassphraseRequired)
    );
    assert!(matches!(
        connect("ed25519", None, b"changed key"),
        Err(ConnectFailure::HostKey(_, true))
    ));
    let bad = tempfile::NamedTempFile::new().unwrap();
    assert!(
        matches!(RealSshTransport.connect(&server(),bad.path(),None,Some(&trusted),&CancellationToken::default()),Err(ConnectFailure::Error(e)) if e.code == ErrorCode::InvalidPrivateKey)
    );
    let mut slow = connect("slow", None, &trusted)
        .unwrap_or_else(|_| panic!("Slow fixture authentication failed"));
    let result = slow.execute(
        RemoteCommand::Probe,
        &CancellationToken::default(),
        Instant::now() + Duration::from_millis(300),
    );
    assert!(matches!(result,Err(e) if e.code == ErrorCode::Timeout));
    let mut dropped = connect("drop", None, &trusted)
        .unwrap_or_else(|_| panic!("Drop fixture authentication failed"));
    assert!(dropped
        .execute(
            RemoteCommand::Probe,
            &CancellationToken::default(),
            Instant::now() + Duration::from_secs(3)
        )
        .is_err());
    let mut target = server();
    target.port = 1;
    assert!(
        matches!(RealSshTransport.connect(&target,&key("ed25519"),None,Some(&trusted),&CancellationToken::default()),Err(ConnectFailure::Error(e)) if e.code == ErrorCode::Unreachable)
    );
}

#[test]
#[ignore = "Requires disposable loopback SSH fixture"]
fn generated_and_imported_keys_authenticate() {
    let trusted = observed();
    for (name, secret) in [
        ("generated-ed25519", None),
        ("generated-rsa", Some("fixture-passphrase")),
    ] {
        let bytes = zeroize::Zeroizing::new(std::fs::read_to_string(key(name)).unwrap());
        let imported = remotedock::infrastructure::ssh_keys::import(&bytes, secret).unwrap();
        assert!(connect(name, secret, &trusted).is_ok());
        let mut file = tempfile::NamedTempFile::new().unwrap();
        use std::io::Write;
        file.write_all(imported.private.as_bytes()).unwrap();
        assert!(RealSshTransport
            .connect(
                &server(),
                file.path(),
                secret,
                Some(&trusted),
                &CancellationToken::default()
            )
            .is_ok());
    }
}
