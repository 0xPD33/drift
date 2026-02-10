# drift

Project-oriented workspace isolation for the [Niri](https://github.com/YaLTeR/niri) Wayland compositor.

Each project gets its own workspace, terminal sessions, environment variables, background services, and event bus. Switching projects means switching everything at once.

## Install

Requires Nix with flakes enabled:

```bash
nix develop  # enters dev shell with cargo
cargo build --release
```

## Quick Start

```bash
# Create a project
drift init myapp --repo ~/code/myapp

# Open it (creates workspace, spawns terminals, starts services)
drift open myapp

# Switch between projects (saves current, opens target)
drift to other-project

# Check what's running
drift status
```

## Project Config

Projects are defined in `~/.config/drift/projects/<name>.toml`:

```toml
[project]
name = "myapp"
repo = "~/code/myapp"
folder = "work"        # optional workspace grouping

[env]
NODE_ENV = "development"
DATABASE_URL = "postgres://localhost/myapp"

[services]
processes = [
    { name = "api", command = "cargo run", restart = "on-failure" },
    { name = "worker", command = "npm run worker", restart = "always" },
]

[[windows]]
name = "editor"
command = "nvim"

[[windows]]
name = "shell"
```

## CLI Reference

### Core Commands

| Command | Description |
|---------|-------------|
| `drift init <name>` | Create a new project config |
| `drift list` | List all projects grouped by folder |
| `drift open <name>` | Open project workspace (creates workspace, spawns terminals/services) |
| `drift close [name]` | Save state, stop services, close workspace |
| `drift to <name>` | Switch to another project (saves current first) |
| `drift save [name]` | Snapshot current workspace state |
| `drift status` | Show current project, services, workspace info |

### Environment & Config

| Command | Description |
|---------|-------------|
| `drift env [name]` | Print project environment variables |
| `drift niri-rules` | Regenerate `~/.config/drift/niri-rules.kdl` |
| `drift ports [--project name]` | Show allocated ports |

### Services

| Command | Description |
|---------|-------------|
| `drift logs` | List available log files |
| `drift logs <service>` | View service logs |
| `drift logs -f <service>` | Follow service logs |

### Event Bus

| Command | Description |
|---------|-------------|
| `drift notify --type <type> --title "text"` | Send event to the bus |
| `drift daemon` | Run the drift daemon (for systemd) |

### Commander (TTS Announcer)

| Command | Description |
|---------|-------------|
| `drift commander start` | Start the TTS event announcer |
| `drift commander stop` | Stop the announcer |
| `drift commander status` | Show announcer status, voice, engine |
| `drift commander say "text"` | Test TTS output |
| `drift commander mute` | Temporarily silence announcements |
| `drift commander unmute` | Resume announcements |

## Global Config

`~/.config/drift/config.toml`:

```toml
[defaults]
terminal = "ghostty"   # terminal emulator to spawn
editor = "nvim"
shell = "zsh"

[ports]
base = 3000            # port range base for projects
range_size = 10

[events]
buffer_size = 200
replay_on_subscribe = 20

[commander]
enabled = false                          # auto-launch with daemon
endpoint = "http://localhost:8880"       # Qwen3-TTS server
voice = "Vivian"
fallback_engine = "piper"               # piper | espeak-ng
fallback_voice = "en_US-lessac-medium"
audio_filter = ""                        # e.g. "sox -t wav - -t wav - bandpass 1000 500"
cooldown_sec = 5
max_queue = 3

[commander.event_instructs]
"service.crashed" = "urgent, tense, clipped delivery"
"agent.completed" = "calm, satisfied, brief"
"agent.error" = "alert, serious"
```

## Niri Integration

Add to your niri config:

```kdl
include "~/.config/drift/niri-rules.kdl"
```

Drift generates workspace declarations and window rules. Niri live-reloads changes.

## Daemon

The daemon provides event streaming, workspace tracking, auto-save, and notification routing. Run as a systemd user service:

```ini
# ~/.config/systemd/user/drift.service
[Unit]
Description=Drift daemon

[Service]
ExecStart=/path/to/drift daemon
Restart=on-failure

[Install]
WantedBy=default.target
```

When `commander.enabled = true`, the daemon auto-launches the TTS announcer.

## Commander

The commander subscribes to the event bus and speaks project events aloud using TTS.

### TTS Engines

1. **Qwen3-TTS** (primary) — HTTP to a local OpenAI-compatible TTS server. Supports per-event voice instructions for different delivery styles.
2. **piper** (fallback) — CLI pipe through piper with aplay.
3. **espeak-ng** (fallback) — CLI pipe through espeak-ng.

The commander checks the HTTP endpoint at startup and periodically rechecks if it goes down. Falls back to CLI automatically.

### Event Filtering

Only certain event types are spoken, filtered by priority:

| Priority | Behavior |
|----------|----------|
| critical | Speak immediately |
| high | Speak, queue |
| medium | Speak only if queue empty |
| low/silent | Never spoken |

Speakable event types: `agent.completed`, `agent.error`, `agent.needs_review`, `service.crashed`, `build.failed`.

### Cooldown

Repeated events of the same type from the same project within the cooldown window are batched: "myapp: 3 more agent.completed events".

## Agent Integration

Agents running inside drift projects can emit events:

```bash
drift notify --type agent.completed --title "Implemented auth" --body "Added JWT to 3 endpoints"
drift notify --type agent.error --title "Build failed" --body "Type error in main.rs:42"
drift notify --type agent.needs_review --title "PR ready" --body "Added 3 files"
```

A Claude Code plugin is included at `plugins/drift-agent/` for automatic agent integration.

## Architecture

```
drift-core/     # config, paths, events, niri IPC, supervisor, commander, workspace
drift-cli/      # CLI binary (clap)
drift-daemon/   # daemon: niri event stream, emit listener, subscriber manager
```

- Fully synchronous — `std::thread` + `std::sync::mpsc`, no async runtime
- Event bus: emit.sock (producers) + subscribe.sock (consumers)
- Daemon tracks workspace-to-project mapping via niri event stream
- Supervisor manages service processes with restart policies and process groups
- Commander: two threads (event reader + speech worker)

See [DESIGN.md](DESIGN.md) for detailed architecture.
