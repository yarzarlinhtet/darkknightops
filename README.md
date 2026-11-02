# RemoteDock

A macOS-first, local infrastructure-management desktop app built with Tauri 2, React, TypeScript, and Rust.

Milestone 1 provides a desktop workspace and persistent local server inventory. It does not connect over SSH, provision servers, manage Docker, deploy applications, or configure routing/certificates. Those sections are clearly marked unavailable.

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

Server fields are display name, hostname/IP, SSH port, SSH username, optional notes, ID and UTC timestamps. Credentials are deliberately not collected. Do not store secrets in notes. Deleting a server requires confirmation and removes local metadata only.

Dashboard reports saved records, not live server health. All connection states remain unknown. The mock SSH transport is injectable for tests and is not registered with production commands.

See [architecture](docs/architecture.md) for boundaries, migrations, IPC and security. The next proposed milestone is SSH key authentication, credential storage and explicit host-key trust; it is not implemented here.
