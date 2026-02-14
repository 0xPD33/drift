# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

All commands must run inside the nix shell — cargo is not available outside it.

```bash
# Type check
nix develop . --command cargo check

# Build
nix develop . --command cargo build

# Run all tests
nix develop . --command cargo test

# Run a single test by name
nix develop . --command cargo test test_name

# Run tests for a specific crate
nix develop . --command cargo test -p drift-core

# Clippy
nix develop . --command cargo clippy
```

The binary name is `drift` (configured via `[[bin]]` in drift-cli/Cargo.toml).

## Architecture

Three-crate Rust workspace. Fully synchronous — no tokio, no async. Uses `std::thread` + `std::sync::mpsc` for concurrency.

### Crates

- **drift-core** (`crates/drift-core/`) — Shared library containing all domain logic: config parsing, niri IPC, supervisor, events, agent command building, commander (TTS), KDL generation, environment composition, registry, workspace snapshots, paths.
- **drift-cli** (`crates/drift-cli/`) — Clap-based CLI binary. Each subcommand lives in `src/commands/<name>.rs`. Depends on both drift-core and drift-daemon.
- **drift-daemon** (`crates/drift-daemon/`) — Background daemon with 4 threads coordinating via two mpsc channels:
  - `event_stream` — reads niri IPC events (blocking, consumes socket)
  - `emit_listener` — accepts events on `emit.sock` (nonblocking)
  - `subscriber_manager` — broadcasts to `subscribe.sock` clients
  - `main` — processes `DaemonMsg` from the first two threads, sends `Event` to subscriber manager

### Key Patterns

- **Atomic file writes**: Write to `.tmp`, then `fs::rename` — used for all state files (configs, snapshots, PID files, services.json)
- **Process groups**: Services spawned with `setsid()` in `pre_exec`, killed via negative PID (`kill(-pid, SIGTERM)`)
- **Signal handling**: `AtomicBool` + `nix::sys::signal::sigaction` for SIGTERM/SIGINT
- **Event priority routing**: Events classified by active/background project × level → critical/high/medium/low/silent
- **Agents are services**: `ServiceProcess` type unifies services and agents — agents have `agent` field set, command is built dynamically via `build_agent_command()`

### niri IPC Quirks

- `Socket::read_events(self)` consumes the socket — daemon creates a separate socket for the event stream
- `Socket::send(&mut self)` returns `io::Result<Reply>` where `Reply = Result<Response, String>` — must unwrap both layers
- Width values: `"60%"` → `SetProportion(60.0)`, `"800px"` → `SetFixed(800)`, `"0.6"` → `SetProportion(60.0)`

### File Locations at Runtime

| Path | Contents |
|------|----------|
| `~/.config/drift/` | config.toml, projects/*.toml, templates/, niri-rules.kdl |
| `~/.local/state/drift/<project>/` | logs/, workspace.json, services.json, supervisor.pid |
| `/run/user/$UID/drift/` | emit.sock, subscribe.sock, daemon.json, daemon.pid |

### Event Types

Workspace: `workspace.created`, `workspace.destroyed`, `workspace.activated`, `workspace.deactivated`
Service: `service.started`, `service.stopped`, `service.crashed`, `service.restarted`
Agent: `agent.completed`, `agent.error`, `agent.needs_review`
Build: `build.failed`
Window: `window.urgent`

### Integration Tests

`crates/drift-cli/tests/integration.rs` uses a `TestEnv` harness that creates isolated tempdir-based XDG directories and runs the actual `drift` binary via `Command`. Test helpers: `run_ok()`, `run_fail()`, `stdout()`, `stderr_fail()`.
