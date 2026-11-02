# RemoteDock — Codex Engineering Instructions

## Product

RemoteDock is a macOS-first, self-hosted infrastructure management desktop application.

It connects to remote Linux VPS servers over SSH and manages Docker workloads without installing a custom server agent or Coolify.

## Technology

- Tauri 2
- React + TypeScript
- Vite
- Tailwind CSS + shadcn/ui
- TanStack Query
- Zustand for local UI state
- Rust application core
- SQLite with versioned migrations
- SSH/SFTP transport
- Docker Engine and Docker Compose v2
- Traefik reverse proxy
- Let's Encrypt ACME

## Architecture

Use layered, feature-oriented architecture.

1. Domain: Pure models, invariants, and business rules.
2. Application: Use cases and orchestration.
3. Infrastructure: SSH, Docker, persistence, credentials, and proxy adapters.
4. Interface: Tauri commands, events, and React UI.

React must never execute shell commands directly.

All privileged operations must be implemented and validated in Rust.

Define interfaces for remote execution, file transfer, Docker operations, and credential storage to enable unit testing.

## MVP Modules

### Server Management
- Server CRUD
- SSH private-key authentication
- Host-key verification and explicit first-use trust
- Connection testing
- Server capabilities and system metrics

### Server Provisioning
- Detect OS and architecture
- Support Ubuntu 24.04 LTS initially
- Install Docker Engine and Compose using official repositories
- Verify sudo permissions
- Idempotent installation steps
- Progress tracking and recoverable failures

### Docker Management
- List and inspect containers
- Start, stop, restart containers
- Stream logs
- Inspect images, volumes, and networks
- Display container resource usage

### Application Deployment
- Projects and applications
- Docker Compose configuration
- Prebuilt registry images
- Environment variables and secrets
- Persistent volumes
- Deployment history
- Health verification

### Proxy Management
- Install Traefik on VPS
- Use Traefik file provider
- Generate dynamic routing configurations
- Shared proxy network
- Manage domains and HTTP routing
- HTTP-to-HTTPS redirect

### SSL Management
- Let's Encrypt HTTP-01
- Persistent ACME certificate storage
- Automatic renewal handled by Traefik
- Certificate status and expiration verification
- DNS and port validation
- ACME error reporting

## Security Requirements

- Never disable SSH host-key verification.
- Never log SSH private keys, passwords, tokens, or environment secrets.
- Store credentials in the OS keychain or an encrypted secret store.
- Never expose Docker's TCP API publicly.
- Do not mount the Docker socket into Traefik.
- Never concatenate untrusted input directly into shell commands.
- Validate remote paths, domains, container identifiers, and Compose project names.
- Avoid unrestricted sudo command execution.
- Require explicit confirmation for destructive operations.
- Do not automatically delete persistent volumes.
- Restrict local Tauri permissions and capabilities.
- Use least privilege where feasible, while documenting that Docker daemon access is root-equivalent.

## Engineering Standards

- TypeScript strict mode.
- Rust Result-based error handling.
- Structured errors with machine-readable codes.
- No unwrap() in production paths without a documented invariant.
- Explicit timeouts and cancellation for SSH operations.
- Idempotent provisioning and deployment operations.
- One active deployment per application.
- Unit tests for configuration generation and validation.
- Integration tests for SSH and Docker operations.
- Keep infrastructure implementations behind traits.
- Maintain SQLite migrations.
- Update documentation for architectural changes.

## Codex Workflow

For each milestone:

1. Inspect the existing repository.
2. Present a concise implementation plan.
3. Implement only the requested milestone.
4. Add relevant tests.
5. Run formatting, linting, type checking, and tests.
6. Report changed files, results, limitations, and remaining work.
7. Do not start the next milestone automatically.

Never provision or modify a real VPS without explicit approval.

Do not add unnecessary microservices, message brokers, or external databases.

Prefer small, testable components over large generic abstractions.

## Definition of Done

A feature is complete when its UI, Rust implementation, error handling, validation, tests, and documentation are implemented and verified.