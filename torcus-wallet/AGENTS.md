# Repository Guidelines

## Project Structure & Module Organization
- `crates/` holds the Rust workspace crates: `cli`, `coordinator`, `node`, `common`, `chains`, `protocols`.
- `scripts/` contains helper scripts for starting/stopping and verification (`start-*.sh`, `verify.sh`).
- `docker/` and `docker-compose.yml` define container images and the multi-service runtime.
- `Makefile` provides common build, run, and test shortcuts.

## Build, Test, and Development Commands
- Prereqs: Docker + Docker Compose for services; Rust nightly for local CLI builds.
- `scripts/start-regtest.sh` starts the full stack with a local regtest bitcoind (recommended).
- `scripts/start-testnet.sh` starts testnet services; `scripts/stop.sh` shuts them down.
- `make build` builds release binaries locally.
- `make run` starts all services via Docker Compose; use `make run-fg` to debug in the foreground.
- `make test` runs unit tests for the `node` crate.
- `make verify` runs integration verification against running services (regtest by default with `make run`).
- `make full-test` performs build + run + verify + stop end-to-end.
- Direct usage: `cargo run --bin mpc-wallet -- <command>` for CLI operations.

## Coding Style & Naming Conventions
- Rust code follows standard Rustfmt defaults (4-space indentation).
- Prefer `snake_case` for functions/modules and `CamelCase` for types/structs.
- Crate boundaries matter: shared types live in `crates/common`, protocol logic in `crates/protocols`.
- Optional hygiene: run `cargo fmt` and `cargo clippy` before PRs (not enforced in repo).

## Testing Guidelines
- Unit tests: `cargo test --package node -- --nocapture`.
- Integration verification: start services and run `scripts/verify.sh` or `make verify`.
- Name tests using Rust’s `#[test]` conventions; keep protocol tests near their modules.

## Commit & Pull Request Guidelines
- Commit messages follow a conventional prefix style: `feat:`, `chore:`, `refactor:`.
- For PRs, include a clear description, link relevant issues, and note test coverage (e.g., `make test`, `make verify`).
- Provide command output or screenshots if behavior or CLI output changes.

## Security & Configuration Tips
- Local development is easiest in regtest mode (`scripts/start-regtest.sh`).
- Do not commit secrets or wallet data; keep local SQLite data in Docker volumes.
