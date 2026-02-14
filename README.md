# drift

A workspace-per-project development environment for the [Niri](https://github.com/YaLTeR/niri) scrollable tiling Wayland compositor.

One command opens your entire project: dedicated workspace, tiled terminal windows, background services, AI agents, environment variables, and git identity. One command tears it all down. Switching projects is instant.

## Why drift

Most developers juggle multiple projects by manually arranging terminals, starting services, setting env vars, and hoping nothing leaks across. Drift handles all of that.

`drift open myapp` creates a niri workspace, spawns your terminal layout with set column widths, starts background services with auto-restart, launches AI agents, injects environment variables, and sets your git identity. `drift to other-project` saves current state and switches instantly.

## Features

- **Workspace isolation** - each project gets its own niri workspace with named, tiled terminal windows
- **Service supervision** - background processes with restart policies, backoff, process groups, and log capture
- **AI agents** - launch Claude Code or Codex as interactive windows or headless workers with automatic prompt delivery
- **Declarative tiling** - set per-window column widths (`60%`, `800px`, `0.6`) applied via niri IPC
- **Environment variables** - merged from TOML config, `.env` files, port allocations, and `DRIFT_*` context
- **Event bus** - Unix socket notification system for services, agents, and external tools (newline-delimited JSON)
- **Port management** - per-project port ranges with conflict detection across workspaces
- **Workspace snapshots** - save and restore window layouts across sessions
- **Voice announcements** - TTS announcer (Qwen3-TTS, piper, espeak-ng) that speaks agent completions, crashes, and build failures

## Install

Requires Nix with flakes enabled:

```bash
# Run directly
nix run github:0xPD33/drift -- open myapp

# Or install
nix profile install github:0xPD33/drift

# Or build from source
nix develop  # enters dev shell with cargo
cargo build --release
# Binary: ./target/release/drift
```

## Quick Start

```bash
# Create a project
drift init myapp ~/code/myapp --folder work

# Add windows, services, agents
drift add window editor "nvim ." --project myapp
drift add window shell --project myapp
drift add service api "npm start" --restart on-failure --project myapp
drift add agent assistant claude "Help me build features" \
  --mode interactive --permissions full --project myapp

# Open everything
drift open myapp

# Switch projects (saves current, opens target)
drift to other-project

# Check status
drift status

# Close everything
drift close myapp
```

## CLI Reference

### Project Management

| Command | Description |
|---------|-------------|
| `drift init <name> [repo]` | Create project (`--folder`, `--template`) |
| `drift list` | List projects grouped by folder (`--archived`) |
| `drift open <name>` | Open workspace, spawn windows and services |
| `drift close [name]` | Save state, stop services, close workspace |
| `drift to <name>` | Switch projects (saves current, opens target) |
| `drift delete <name>` | Remove project permanently (`--yes` to skip prompt) |
| `drift archive <name>` | Hide project (reversible) |
| `drift unarchive <name>` | Restore hidden project |

### Configuration

| Command | Description |
|---------|-------------|
| `drift add service <name> <cmd>` | Add background service (`--restart`, `--cwd`) |
| `drift add agent <name> <type> <prompt>` | Add AI agent (`--mode`, `--permissions`, `--model`) |
| `drift add window <name> [cmd]` | Add terminal window |
| `drift add env <key> <value>` | Set environment variable |
| `drift add port <name> <port>` | Add named port |
| `drift add port-range <start> <end>` | Set port range |
| `drift remove <type> <name>` | Remove any of the above |

### Inspection

| Command | Description |
|---------|-------------|
| `drift status` | Project info, services, ports, recent events |
| `drift env [name]` | Print environment variables |
| `drift ports` | Show port allocations (`--project`) |
| `drift logs [service]` | View service logs (`-f` to follow) |
| `drift events` | View events (`-f` to follow, `--type`, `--last`, `--all`) |
| `drift save [name]` | Save workspace snapshot |
| `drift niri-rules` | Regenerate niri window rules |
| `drift shell-data` | Full state as JSON |

### Notifications

| Command | Description |
|---------|-------------|
| `drift notify <title> [body]` | Emit event (`--type`, `--level`, `--source`, `--project`) |

### Commander (TTS)

| Command | Description |
|---------|-------------|
| `drift commander start` | Start TTS announcer |
| `drift commander stop` | Stop announcer |
| `drift commander status` | Show status and voice |
| `drift commander say <text>` | Speak text |
| `drift commander mute/unmute` | Toggle announcements |

## Configuration

See [CONFIGURATION.md](CONFIGURATION.md) for the full reference covering global settings, project config, services, agents, windows, ports, environment variables, and TTS.

## Event Bus

Events are newline-delimited JSON over Unix sockets:

```json
{
  "type": "agent.completed",
  "project": "myapp",
  "source": "reviewer",
  "ts": "2026-02-12T15:30:00Z",
  "level": "success",
  "title": "Code review complete",
  "body": "All 5 files approved",
  "meta": { "files": 5 }
}
```

Emit from anywhere:

```bash
drift notify --type agent.completed "Task done" "Details here"
```

Subscribe:

```bash
drift events -f --type "agent.*"
```

### Speakable events

`agent.completed`, `agent.error`, `agent.needs_review`, `service.crashed`, `build.failed`

Priority controls delivery: critical speaks immediately, high queues, medium speaks only if idle, low/silent are suppressed.

## Daemon

The daemon handles event routing, workspace tracking, and auto-save. Run as a systemd user service:

```ini
# ~/.config/systemd/user/drift.service
[Unit]
Description=Drift daemon
After=niri.service

[Service]
ExecStart=/path/to/drift daemon
Restart=on-failure

[Install]
WantedBy=default.target
```

```bash
systemctl --user enable --now drift.service
```

Setting `commander.enabled = true` makes the daemon auto-launch the TTS announcer.

## Architecture

```
drift-core/     shared library: config, niri IPC, supervisor, events, agents, TTS
drift-cli/      CLI binary (clap): all user-facing commands
drift-daemon/   background daemon: event stream, notification bus, auto-save
```

Pure synchronous Rust, no async runtime. The daemon runs four threads coordinated via `std::sync::mpsc` channels. Services run in process groups (`setsid`) for clean shutdown. All state files use atomic writes (write to `.tmp`, then rename).

## License

MIT
