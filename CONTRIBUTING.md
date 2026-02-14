# Contributing to drift

## Prerequisites

- [Nix](https://nixos.org/) with flakes enabled
- [Niri](https://github.com/YaLTeR/niri) Wayland compositor (for runtime testing)

## Setup

```bash
git clone https://github.com/0xPD33/drift.git
cd drift
nix develop  # enters dev shell with cargo + rust-analyzer
```

## Building and Testing

```bash
cargo build           # compile
cargo test            # run all tests
cargo test -p drift-core  # test a single crate
cargo clippy          # lint
```

## Project Structure

```
crates/
  drift-core/     shared library (config, niri IPC, supervisor, events, agents)
  drift-cli/      CLI binary — one file per subcommand in src/commands/
  drift-daemon/   background daemon (event bus, workspace tracking, auto-save)
```

## Code Style

- No async — the project is fully synchronous using `std::thread` + `std::sync::mpsc`
- Atomic file writes everywhere: write to `.tmp`, then `fs::rename`
- Process groups via `setsid()` for clean service tree kills
- Keep things simple — no abstractions for single-use code

## Adding a CLI Command

1. Create `crates/drift-cli/src/commands/<name>.rs` with a `pub fn run(...) -> anyhow::Result<()>`
2. Add the variant to `Commands` enum in `commands/mod.rs`
3. Wire it up in `main.rs`
4. Add integration tests in `crates/drift-cli/tests/integration.rs`

## Testing

Integration tests use a `TestEnv` harness that creates isolated tempdir-based XDG directories and runs the actual `drift` binary. No real niri connection is needed for config/registry tests.

```bash
cargo test -p drift-cli   # integration tests
cargo test -p drift-core  # unit tests
```

## Submitting Changes

1. Fork the repo and create a branch
2. Make your changes
3. Run `cargo test` and `cargo clippy` — both must pass
4. Open a pull request with a clear description of what changed and why
