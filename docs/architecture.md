# Milestone 1 architecture

## Layers

- **Domain**: server metadata, input invariants, structured errors and UUID validation. No Tauri or database access.
- **Application**: `ServerService` validates requests and delegates to the `ServerRepository` trait. No infrastructure-specific SQL.
- **Infrastructure**: `SqliteServerRepository` owns the SQLite connection behind a mutex. SSH has typed timeout, cancellation and mandatory host-key-policy contracts plus a deterministic injectable mock, but no network adapter.
- **Interface**: thin async Tauri command handlers dispatch service work to a blocking worker. React calls only typed IPC wrappers. TanStack Query owns server cache and mutation refresh; Zustand owns appearance/sidebar preferences.

The UI is feature-oriented under `src/features`. Shared shadcn-style Radix primitives live in `src/components/ui`, configured through `components.json`. Tailwind 4 uses the Vite plugin, CSS theme tokens and a class-based dark variant. React Router uses hash routes for packaged navigation. Nothing requires a separate HTTP server in production.

## IPC

`list_servers`, `get_server`, `create_server`, `update_server`, and `delete_server` form the complete command surface. Rust `Server`, `ServerInput`, `AppError` and `ErrorCode` generate TypeScript declarations through ts-rs. `src/lib/ipc.ts` centralizes command names, argument/result types, desktop-runtime detection and safe error conversion. Regenerate types with `npm run bindings`; the Rust contract test must pass before committing an interface change.

Validation, not-found, database and migration errors have stable machine-readable codes. Validation optionally identifies a form field. Raw database messages, input values and local paths are not exposed through errors. Invalid JSON argument shapes are rejected by Tauri before service dispatch; frontend wrappers do not expose these internal rejection details.

Database initialization failures are retained in application state. The desktop can display the error, but every metadata operation fails until initialization succeeds after restart; it never falls back to volatile data. Server mutations finish before invalidating cached inventory. Failed forms retain entered values. IDs are generated in Rust, and creation timestamps survive edits.

## SQLite migrations

Numbered SQL files are embedded at build time. SQLite `user_version` records the applied version. Pending migrations and version changes run in one transaction; errors roll them all back. A database newer than this binary is refused. Migration 001 is preserved from the initial scaffold. Migration 002 adds notes/timestamps and an index, backfilling timestamps for existing records.

Add each new migration to the ordered migration list, increment the version and include migration/reopening tests. Never rewrite an applied migration. Queries use bound parameters, deterministic case-insensitive name/ID ordering, and a five-second lock timeout. The repository trait permits test replacements without starting a webview.

## Security and limits

Only the local main window has the five metadata command permissions. Tauri's app manifest explicitly lists commands so application-command ACL checks are enabled. No shell, HTTP, filesystem or OS credential plugins are exposed to JavaScript. CSP blocks external scripts, frames and connections; fonts and assets are bundled. Vite's local development server is a development-only process.

No passwords, private keys, tokens or secret references are collected or logged. There is no production SSH adapter, host-key bypass, cloud service, telemetry, Docker socket/API, sudo operation or provisioning command. Future credential storage must use the OS keychain or an encrypted store. Future Docker functionality must document that daemon access is root-equivalent.

## Validation and testing

Rust is authoritative. Hosts accept ASCII DNS names or Rust-parsed IPv4/IPv6; URLs, embedded ports, malformed numeric IPs and shell syntax are rejected. Names, usernames, notes and ports have bounded validation. UI checks catch common errors early; IPv6 syntax is finally checked in Rust.

Rust tests cover domain boundaries, migration rollback/repeat execution and upgrades, persistence across reopening, CRUD, missing records, SQL-like metadata, generated type drift, and SSH mock timeout/cancellation/trust behavior. Frontend tests exercise IPC payloads/errors, invalid forms, edits with failed saves, query refresh, delete confirmation/cancellation/failure, navigation and theme persistence.

Manual macOS acceptance: launch the desktop, create/edit a server, restart and verify persistence, cancel then confirm deletion, switch all themes, follow system appearance changes, resize the window, and navigate dialogs by keyboard. No real VPS is involved.
