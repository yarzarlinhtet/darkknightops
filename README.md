# RemoteDock

A macOS-first, local infrastructure-management desktop app built with Tauri 2, React, TypeScript, and Rust.

Milestone 2 adds SSH private-key connections, explicit host-key trust, macOS Keychain passphrases, read-only Linux discovery, Docker capability detection, and resource polling on the server detail screen. Provisioning, workload changes, deployment, and routing/certificates remain unavailable.

## Prerequisites

- Node.js 22 LTS or newer supported LTS and npm.
- Stable Rust including Cargo, rustfmt and Clippy.
- Xcode command-line tools on macOS (`xcode-select --install` if absent).

See [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for platform setup. With Homebrew, `brew install rust` supplies the Rust toolchain.

## Run

```sh
npm ci
npm run tauri dev
```

`npm run dev` opens a browser preview. Persistence is available only inside Tauri; browser preview reports that limitation instead of simulating successful operations.

```sh
npm run tauri build -- --bundles app
```

This creates a local unsigned macOS application. Signing, notarization, distribution and update services are not configured.

## Checks

```sh
npm run format:check
npm run lint
npm run typecheck
npm test
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

`npm run bindings` regenerates the committed TypeScript contracts from Rust. A Rust test detects contract drift. Do not format the generated bindings manually.

## Local data and boundaries

Server metadata lives in `remotedock.sqlite` under Tauri's application-data directory (normally `~/Library/Application Support/com.remotedock.desktop` on macOS). SQLite migrations run automatically and transactionally. The migration from the initial scaffold preserves saved records. Theme and sidebar preferences are stored in local webview storage.

Server metadata and SSH key-file paths are local. Private-key contents and passphrases never enter SQLite. Saved passphrases use macOS Keychain; unsaved passphrases stay in Rust memory for the detail session and are cleared on close or idle expiry. Only OpenSSH-format private keys are accepted; encrypted Ed25519 and RSA keys are supported by the configured SSH/crypto libraries. Other OS credential stores are not configured and return an explicit unavailable error.

Add a server and optionally select a key, then open its details. Test the connection, independently verify and approve first-use host identity, and unlock encrypted keys when requested. A successful test automatically discovers system/Docker information. Metrics refresh every 10 seconds by default while the visible detail page is open; select 5/10/30/60 seconds there. Changed keys block authentication and require deliberate review/replacement.

Connection history and discovered snapshots survive restart. Historical success never implies current reachability. Closing the detail page cancels operations and releases its SSH session. Browser preview has no SSH or persistence access.

See [architecture](docs/architecture.md) and [Milestone 2 testing](docs/milestone-2-testing.md) for security boundaries, disposable SSH integration tests, and manual read-only VPS acceptance. Docker access is root-equivalent; no sudo, provisioning, or remote configuration writes occur in this milestone.

SSH key generation and management is documented in [Milestone 2.5](docs/milestone-2.5-ssh-keys.md), including supported formats, local storage security, legacy compatibility and disposable authentication tests.
