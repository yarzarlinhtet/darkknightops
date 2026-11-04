# Milestone 3: provisioning

Provisioning is a Rust application service over the existing SSH transport and credential resolution. React supplies stable IDs and reviewed plan IDs, never scripts. The closed command vocabulary selects bundled scripts; every SSH handshake still validates the stored host key before authentication. Root or passwordless sudo for the required administrative commands is necessary. Interactive sudo passwords are not accepted or stored.

## Supported behavior

Ubuntu 24.04 on amd64 and arm64 is supported. Preflight reads os-release as data and checks systemd, apt, privileges, disk space (minimum 2 GiB on every detected target filesystem), DNS, canonical Ubuntu/Docker HTTPS endpoints, package state, Docker CLI/daemon/Compose, workloads and group access. Missing curl/wget, unavailable repository checks, incomplete dpkg state and active RemoteDock units block installation rather than assuming success. Network checks use canonical Ubuntu endpoints; custom mirror reachability is ultimately checked by apt during the approved run.

Plans expire after five minutes and are single-use. Their material hash binds SSH identity/configuration, trusted host key, package/service/access state, workload count, compatibility and prerequisite outcomes. Free space is compared by threshold, not fluctuating byte counts. Execution rechecks the plan before changes and compatibility/privileges/package activity before each step. An approval for provisioning cannot authorize group membership, or vice versa.

The installer follows https://docs.docker.com/engine/install/ubuntu/ (reviewed November 4, 2026): stable Docker apt repository, ASCII signing key and Deb822 docker.sources. The Docker release key fingerprint is pinned to `9DC858229FC7DD38854AE2D88D81803C0EBFCD88`; key rotation requires a reviewed application update. Matching canonical configuration is reused. Custom/duplicate repositories, reserved-file collisions, conflicting packages, removal transactions and upgrades to installed packages block with manual remediation guidance. Existing healthy installations are retained. Compose detection accepts the v2 plugin command and compatible later major versions.

Missing CA/curl/GnuPG prerequisites are installed first. Docker installation requests docker-ce, docker-ce-cli, containerd.io, docker-buildx-plugin and docker-compose-plugin. apt simulations reject removals/upgrades, Docker package candidates must originate from Docker's noble/stable repository, and simulated package versions are pinned for installation. apt lock acquisition waits at most 120 seconds. Existing daemon settings and persistent Docker data are preserved; no firewall configuration, TCP API or socket mode is changed. Existing running services are never explicitly restarted. Package maintainer scripts may start Docker and change networking, as disclosed during approval.

Docker group membership is a separate, explicitly approved, root-equivalent grant. The target is the authenticated session's `id -un`, passed as a quoted argument. A fresh authenticated SSH session verifies access. Unverified or pending login access is reported as a failure requiring reconnection, not success. Provisioning itself can succeed with administrative-only Docker access; direct access remains a separate visible status.

## Execution, cancellation and recovery

Each mutation runs in a uniquely named transient systemd oneshot unit with RemainAfterExit. These units are execution wrappers, not an installed management agent. They survive desktop/SSH disconnection and retain a verifiable exit status until reboot. Package operations have no kill-on-cancel behavior. Monitoring is bounded to 30 minutes per unit; a timeout reports unknown/interrupted state while the remote operation may continue. apt HTTP timeouts/retries are bounded, but a hanging package maintainer script must be diagnosed administratively rather than killed automatically.

The user can request cancellation. The worker lets an active step finish and stops before the next step. Partial changes are retained and explicitly reported; no rollback is claimed. The existing per-server connection guard excludes concurrent provisioning, monitoring and configuration changes. The worker owns its SSH session and zeroized ephemeral passphrase independently of detail navigation. The lease sweeper retains active detail leases.

Migration 005 adds provisioning_runs and provisioning_steps. Snapshots and ordered steps update transactionally after transitions. A partial unique index rejects simultaneous local active runs. Desktop startup changes abandoned runs to INTERRUPTED and running steps to UNKNOWN. A fresh preflight inspects actual packages, audit state and active remote units before retry. A reboot that lost the transient unit never counts as verified success. No automatic dpkg repair or destructive cleanup is attempted.

Progress events include a monotonically increasing run sequence. The UI uses snapshots and local history polling to recover missed events. Installation log export admits only known package phase/name pairs; arbitrary remote journal text, URLs, versions, credentials and terminal controls are not displayed or persisted. Safe unit identifiers are retained for manual journal diagnosis. History returns the latest 50 runs; older rows remain stored.

## Automated checks

Normal Rust tests cover compatibility, conservative detection, Compose, privileges, repository collisions, filesystem thresholds, apt script syntax, log redaction, approval expiry/replay/drift, operation exclusion, fresh/repeated/partial installation, disconnects, cancellation and durable recovery. Mock SSH tests do not establish that package installation works on a real OS. Frontend tests cover local-only opening, plan review/approval, blocked prerequisites, Docker access warnings and cancellation/progress.

The ignored `provisioning_integration` test mutates only an explicitly opted-in disposable fixture at 127.0.0.1:22223. It requires the fixture marker and a locally pinned VM host key before making changes. No production target or arbitrary host override is supported.

## Disposable Ubuntu integration setup

Use a disposable systemd Ubuntu 24.04 VM for each scenario, with snapshots if available. A container without a working systemd manager is unsuitable. No Docker socket from the host should be shared with the VM.

1. Create `.provisioning-test/` (ignored by Git), mode 0700, and an unencrypted disposable Ed25519 key at `.provisioning-test/id_ed25519`.
2. Copy `src-tauri/tests/fixtures/provisioning/cloud-init.yaml` into that directory and replace its public-key placeholder with the disposable public key. Launch an isolated Ubuntu 24.04 VM using that cloud-init configuration. The configuration creates only a fixture account and marker; it does not install Docker.
3. Copy the VM's `/etc/ssh/ssh_host_ed25519_key.pub` through the VM manager's trusted console into `.provisioning-test/ssh_host_ed25519_key.pub`. Do not derive trust from an unverified network handshake.
4. Forward local 127.0.0.1:22223 to the disposable VM's SSH port using the VM manager or a separately verified SSH tunnel. Do not forward a production host.
5. Explicitly run `REMOTEDOCK_PROVISIONING_FIXTURE=disposable-ubuntu-24.04 cargo test --manifest-path src-tauri/Cargo.toml --test provisioning_integration -- --ignored --nocapture`.
6. Remove the disposable VM using its manager after reviewing results. The application/test does not delete infrastructure automatically.

Repeat on amd64 and arm64. The integration test installs/verifies Docker and repeats the operation, requiring the second plan's mutation steps to be satisfied.

Manual fault matrix (use separate disposable snapshots):

| Fixture state                              | Expected outcome                                                        |
| ------------------------------------------ | ----------------------------------------------------------------------- |
| Fresh Ubuntu                               | Official packages installed; service enabled/running; versions verified |
| Healthy existing Docker                    | No package/service mutation                                             |
| CLI only, unknown installation             | Blocked for manual inspection                                           |
| Official engine installed, service stopped | Start only as required; verify actual daemon                            |
| Official engine with Compose missing       | Missing plugin installed; no explicit daemon restart                    |
| Interactive sudo / no sudo                 | Blocked with privilege guidance                                         |
| Docker socket denied                       | Administrative access distinguished; direct access never assumed        |
| Explicit group grant                       | Warning and approval; membership and fresh-session access verified      |
| SSH interrupted during apt                 | Unit continues; local outcome unknown; fresh preflight reconciles       |
| Desktop killed during apt                  | Durable interrupted history on restart; no duplicate active install     |
| Partial installation, clean dpkg           | Fresh plan installs remaining packages                                  |
| Incomplete dpkg transaction                | Blocked for administrative repair                                       |
| Repository unavailable                     | Failed connectivity or package step; no assumed completion              |
| Service deliberately configured to fail    | Verification fails; logs/history retain safe diagnosis                  |
| External apt lock held                     | Finite wait; safe failure without killing apt/dpkg                      |

macOS UI acceptance: review actual prerequisites, approve only the displayed plan, navigate away/back during installation, request cancellation during apt, restart the desktop, verify persistent history, retry using a new plan, and independently verify Docker versions/service/access in a fresh SSH login.

### Provisioning UI history additions

New run snapshots include the complete approved plan and sanitized log entries with stable IDs, step keys, levels, and UTC observation timestamps. Observation timestamps record when RemoteDock collected output, rather than claiming remote execution times. Repeated identical messages within a step are deduplicated. These additive JSON snapshot fields use deserialization defaults, so existing SQLite rows remain readable without a schema migration. Legacy plan and timestamp omissions are labeled in the UI.

The provisioning interface groups actual preflight observations by compatibility, administration, repositories, installation, and access. Plan expiry disables approval locally; Rust remains authoritative for approval expiry and state drift. Log clearing only affects visible output and never deletes persisted history.
