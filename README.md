# drift

A workspace-per-project development environment for the [Niri](https://github.com/YaLTeR/niri) scrollable tiling Wayland compositor.

One command opens your entire project: dedicated workspace, tiled terminal windows, background services, AI coding agents, environment variables, git identity — all isolated from every other project. One command tears it all down. Switching projects is instant.

## Why drift

Most developers juggle multiple projects by manually arranging terminals, starting services, setting env vars, and hoping nothing bleeds across. Drift eliminates that overhead entirely.

`drift open myapp` creates a named niri workspace, spawns your configured terminal layout with precise column widths, boots background services with automatic restart, launches interactive AI agents that start working immediately (no prompts, no trust dialogs), injects project-specific environment variables, and sets your git identity — all in under a second. `drift to other-project` saves everything and switches context atomically.

## Features

- **Full workspace isolation** — each project owns a niri workspace with named, tiled terminal windows at exact column proportions
- **Service supervision** — background processes with restart policies (`never`/`on-failure`/`always`), exponential backoff, process group management, and per-service log capture
- **AI agent orchestration** — spawn Claude Code or Codex agents as interactive TUI windows or headless oneshot workers with configurable permissions and automatic prompt delivery
- **Declarative tiling** — configure per-window column widths (`60%`, `800px`, `0.6`) applied automatically via niri IPC on spawn
- **Environment composition** — merge variables from TOML config, `.env` files, port allocations, and auto-injected `DRIFT_*` context into every spawned process
- **Event bus** — priority-routed Unix socket notification system connecting services, agents, and external tooling via newline-delimited JSON
- **Port management** — per-project port ranges with cross-workspace conflict detection at open time
- **Workspace persistence** — snapshot and restore window layouts across sessions
- **Voice announcements** — TTS event announcer (Qwen3-TTS, piper, espeak-ng) that speaks agent completions, service crashes, and build failures with configurable per-event delivery styles

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
drift init myapp --repo ~/code/myapp --folder work

# Configure your layout
drift add window editor "nvim ." --project myapp
drift add window shell --project myapp
drift add service api "npm start" --restart on-failure --project myapp
drift add agent assistant claude "Help me build features" \
  --mode interactive --permissions full --project myapp

# One command opens everything
drift open myapp

# Instant context switch — saves current state, opens target
drift to other-project

# See what's running
drift status

# Tear it all down
drift close myapp
```

## CLI Reference

### Project Management

| Command | Description |
|---------|-------------|
| `drift init <name>` | Create project (`--repo`, `--folder`, `--template`) |
| `drift list` | List projects grouped by folder (`--archived`) |
| `drift open <name>` | Open workspace, spawn windows and services |
| `drift close [name]` | Save state, stop services, close workspace |
| `drift to <name>` | Switch projects (saves current, opens target) |
| `drift delete <name>` | Permanently remove project (`--yes` to skip confirmation) |
| `drift archive <name>` | Reversibly hide project |
| `drift unarchive <name>` | Restore archived project |

### Configuration

| Command | Description |
|---------|-------------|
| `drift add service <name> <cmd>` | Add service (`--restart`, `--cwd`) |
| `drift add agent <name> <agent> <prompt>` | Add AI agent (`--mode`, `--permissions`, `--model`) |
| `drift add window <name> [cmd]` | Add terminal window |
| `drift add env <key> <value>` | Add environment variable |
| `drift add port <name> <port>` | Add named port |
| `drift add port-range <start> <end>` | Set port range |
| `drift remove <type> <name>` | Remove service/agent/window/env/port/port-range |

### Inspection

| Command | Description |
|---------|-------------|
| `drift status` | Show project info, services, ports, recent events |
| `drift env [name]` | Print project environment variables |
| `drift ports` | Show allocated ports (`--project`) |
| `drift logs [service]` | View service logs (`-f` to follow) |
| `drift events` | View event stream (`-f`, `--type`, `--last`, `--all`) |
| `drift save [name]` | Snapshot workspace state |
| `drift niri-rules` | Regenerate niri window rules |

### Notifications

| Command | Description |
|---------|-------------|
| `drift notify` | Emit event (`--type`, `--title`, `--level`, `--source`) |

### Commander (TTS)

| Command | Description |
|---------|-------------|
| `drift commander start` | Start TTS event announcer |
| `drift commander stop` | Stop announcer |
| `drift commander status` | Show status, voice, engine |
| `drift commander say <text>` | Test speech output |
| `drift commander mute/unmute` | Toggle announcements |

## Configuration

### Global: `~/.config/drift/config.toml`

```toml
[defaults]
terminal = "ghostty"   # ghostty, foot, alacritty, kitty, wezterm
editor = "nvim"
shell = "zsh"

[ports]
base = 3000            # auto-allocation base
range_size = 10        # ports per project

[events]
buffer_size = 200
replay_on_subscribe = 20

[commander]
enabled = false
endpoint = "http://localhost:8880"
voice = "Vivian"
fallback_engine = "espeak-ng"    # piper | espeak-ng
fallback_voice = "en_US-lessac-medium"
cooldown_sec = 5
max_queue = 3

[commander.event_instructs]
"service.crashed" = "urgent, tense, clipped delivery"
"agent.completed" = "calm, satisfied, brief"
"agent.error" = "alert, serious"
```

### Project: `~/.config/drift/projects/<name>.toml`

```toml
[project]
name = "myapp"
repo = "~/code/myapp"
folder = "work"              # optional grouping
icon = "🚀"                  # optional

[env]
env_file = ".env"            # load from repo-relative file
NODE_ENV = "development"
DATABASE_URL = "postgres://localhost/myapp"

[git]
user_name = "Alice"
user_email = "alice@dev.com"

[ports]
range = [3000, 3009]
api = 3001
frontend = 3002

[services]
processes = [
    { name = "api", command = "npm start", restart = "on-failure" },
    { name = "assistant", agent = "claude", prompt = "Help me code",
      agent_mode = "interactive", agent_permissions = "full", width = "60%" },
]

[[windows]]
name = "editor"
command = "nvim ."
width = "40%"

[[windows]]
name = "shell"
```

#### Service fields

| Field | Default | Description |
|-------|---------|-------------|
| `name` | required | Service identifier |
| `command` | required | Shell command (empty for agents) |
| `cwd` | `"."` | Working directory relative to repo |
| `restart` | `"never"` | `never`, `on-failure`, `always` |
| `stop_command` | — | Custom stop command (optional) |

#### Agent fields

| Field | Default | Description |
|-------|---------|-------------|
| `agent` | — | Agent type: `claude`, `codex` |
| `prompt` | — | Task prompt |
| `agent_mode` | `"oneshot"` | `oneshot` (headless) or `interactive` (TUI window) |
| `agent_permissions` | `"full"` | `full` (all tools) or `safe` (read-only) |
| `agent_model` | — | Model override (optional) |
| `width` | — | Column width for interactive agents |

#### Window fields

| Field | Description |
|-------|-------------|
| `name` | Window identifier (used in terminal title) |
| `command` | Shell command to run (omit for plain shell) |
| `width` | Column width: `"60%"`, `"800px"`, or `"0.6"` |

## Environment Variables

Every process spawned by drift inherits:

| Variable | Description |
|----------|-------------|
| `DRIFT_PROJECT` | Project name |
| `DRIFT_REPO` | Absolute repo path |
| `DRIFT_FOLDER` | Folder group (if set) |
| `DRIFT_NOTIFY_SOCK` | Event bus emit socket path |

Plus all `[env]` vars and env_file contents from the project config.

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
drift notify --type agent.completed --title "Task done" --body "Details here"
```

Subscribe to events:

```bash
drift events -f --type "agent.*"
```

### Speakable events (commander)

`agent.completed`, `agent.error`, `agent.needs_review`, `service.crashed`, `build.failed`

Priority determines behavior: critical speaks immediately, high queues, medium speaks only if queue is empty, low/silent are suppressed.

## Daemon

The daemon provides event routing, workspace tracking, and auto-save. Run as a systemd user service:

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

When `commander.enabled = true`, the daemon auto-launches the TTS announcer.

## Architecture

```
drift-core/     shared library: config, niri IPC, supervisor, events, agent, commander
drift-cli/      CLI binary (clap): all user-facing commands
drift-daemon/   background daemon: niri event stream, notification bus, auto-save
```

Written in pure synchronous Rust — no tokio, no async runtime. Four daemon threads coordinate via `std::sync::mpsc` channels: niri event stream, emit socket listener, subscriber manager, and main event loop. Service processes run in dedicated process groups (`setsid`) for clean tree kills on shutdown. All state files use atomic writes (`.tmp` + `rename`) to prevent corruption.

## File Layout

```
~/.config/drift/
  config.toml                  global settings
  projects/<name>.toml         project configs
  templates/                   init templates
  niri-rules.kdl               auto-generated window rules

~/.local/state/drift/<project>/
  logs/                        service logs (supervisor.log, <service>.log)
  workspace.json               saved workspace snapshot
  services.json                supervisor state
  supervisor.pid               supervisor PID file

/run/user/$UID/drift/
  emit.sock                    event emission socket
  subscribe.sock               event subscription socket
  daemon.json                  daemon state
```

## License

MIT
