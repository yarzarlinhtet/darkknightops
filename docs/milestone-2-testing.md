# Milestone 2 verification

## Automated checks

Run the checks in README. The default Rust suite uses injected transports and credential stores and requires no VPS, Docker daemon, or Keychain access. The frontend suite mocks typed IPC. These checks cannot prove desktop Keychain permissions or remote SSH compatibility.

## Disposable OpenSSH integration fixture

Start Docker Desktop yourself. From the repository root:

```sh
sh src-tauri/tests/fixtures/ssh/start.sh
docker logs remotedock-ssh-test
cargo test --manifest-path src-tauri/Cargo.toml --test ssh_integration -- --ignored --nocapture
docker stop remotedock-ssh-test
```

Wait for sshd to report listening before running tests. The container binds only loopback port 22222, runs Ubuntu 24.04, disables password authentication, and has no Docker socket mount. Keys are generated locally under ignored `.ssh-test/`, mounted read-only into the container, and are exclusively disposable test credentials. The RSA fixture passphrase is public test data. Stop the container when finished.

The opt-in integration test verifies real Ed25519 and encrypted RSA authentication, missing/wrong passphrases, invalid key contents, unknown/exact/mismatched host identities, actual Linux discovery, missing Docker, a forced slow command, a dropped connection, and a closed loopback port. Timeout/drop behaviors are implemented by forced commands on dedicated public keys. Docker permission-denied, daemon-unavailable and malformed-version classifications have deterministic parser coverage; this fixture intentionally does not run a nested Docker daemon.

For an actual host-key rotation check, record/trust the fixture's fingerprint in the desktop app, stop the fixture, run the start script again (a new container generates a new host identity), and test again. Authentication must be blocked with a changed-key warning. Independently inspect the disposable identity:

```sh
docker exec remotedock-ssh-test ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub -E sha256
```

## Manual desktop acceptance

Use the disposable fixture first:

1. Add a server at `127.0.0.1:22222` with username `tester`. Select `.ssh-test/ed25519` using the native picker.
2. Open details and test. Verify that unknown trust blocks authentication and metrics. Compare the fingerprint with the fixture console, cancel once, then explicitly approve.
3. Verify that the subsequent connection test succeeds and displays its measured full handshake/authentication/probe duration. OS and Docker discovery follow automatically.
4. Verify Ubuntu 24.04, actual resource values, and Docker missing. Change the refresh interval and use manual refresh.
5. Navigate away during a refresh. Return and verify no previous session implies current connectivity. Quit/restart: metadata, trust, and historical snapshots survive; runtime health starts unknown.
6. Select the encrypted RSA fixture. Try a wrong passphrase, then `fixture-passphrase` without saving. Unlock/test; navigate away and return to confirm another passphrase is required.
7. Save the fixture passphrase in Keychain, restart, and test. Change/remove the credential and confirm the Keychain item is removed. Deny Keychain access or lock the login keychain and confirm a clear error, never a plaintext fallback.
8. Try missing files, invalid files, invalid input, unreachable ports, cancel, stale metrics, keyboard dialog navigation, and changed-host-key review. Replacement requires separate review and an independent-verification checkbox.
9. Confirm only the local main window has discovery permissions and browser preview reports that the desktop is required.

For an explicitly approved real VPS test, repeat read-only acceptance on Ubuntu 24.04 using an existing authorized user/key. Independently verify its fingerprint through the hosting console before approval. Test existing Docker installations with and without socket access; do not install Docker or change group membership, firewall, SSH configuration, or workloads. RemoteDock never uses sudo in this milestone.

## Current verification limits

The container suite is opt-in and ignored by default. A running local Docker daemon is required. Desktop dialogs, Keychain access policy, RSA/Ed25519 interoperability with a real SSH daemon, and packaged UI behavior require the above manual/integration checks; passing parser/mock tests alone is not evidence of those behaviors.
