# Configuration

## Global: `~/.config/drift/config.toml`

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

### Defaults

| Field | Default | Description |
|-------|---------|-------------|
| `terminal` | `"foot"` | Terminal emulator for spawned windows |
| `editor` | `"nvim"` | Default editor |
| `shell` | `"bash"` | Default shell |

### Ports

| Field | Default | Description |
|-------|---------|-------------|
| `base` | `3000` | Starting port for auto-allocation |
| `range_size` | `10` | Number of ports per project |

### Events

| Field | Default | Description |
|-------|---------|-------------|
| `buffer_size` | `200` | Max events kept in memory |
| `replay_on_subscribe` | `20` | Events replayed to new subscribers |

### Commander (TTS)

| Field | Default | Description |
|-------|---------|-------------|
| `enabled` | `false` | Enable voice announcements |
| `endpoint` | `"http://localhost:8880"` | TTS API endpoint |
| `voice` | `"Vivian"` | Voice name for primary TTS |
| `fallback_engine` | `"espeak-ng"` | Fallback engine: `piper` or `espeak-ng` |
| `fallback_voice` | `"en_US-lessac-medium"` | Voice for fallback engine |
| `cooldown_sec` | `5` | Min seconds between announcements |
| `max_queue` | `3` | Max queued announcements |

#### Event instructs

Custom delivery styles per event type. Used as instructions for the TTS model:

```toml
[commander.event_instructs]
"service.crashed" = "urgent, tense, clipped delivery"
"agent.completed" = "calm, satisfied, brief"
"agent.error" = "alert, serious"
```

## Project: `~/.config/drift/projects/<name>.toml`

```toml
[project]
name = "myapp"
repo = "~/code/myapp"
folder = "work"
icon = "🚀"

[env]
env_file = ".env"
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

### Project fields

| Field | Required | Description |
|-------|----------|-------------|
| `name` | yes | Project identifier |
| `repo` | yes | Path to repository (supports `~`) |
| `folder` | no | Folder group for organization |
| `icon` | no | Emoji shown in listings |

### Environment

| Field | Description |
|-------|-------------|
| `env_file` | Path to `.env` file relative to repo |
| `<KEY> = "<value>"` | Any other key-value pairs become environment variables |

All processes spawned by drift also inherit these automatic variables:

| Variable | Description |
|----------|-------------|
| `DRIFT_PROJECT` | Project name |
| `DRIFT_REPO` | Absolute repo path |
| `DRIFT_FOLDER` | Folder group (if set) |
| `DRIFT_NOTIFY_SOCK` | Event bus socket path |
| `DRIFT_PORT_<NAME>` | Each named port (uppercased) |
| `DRIFT_PORT_RANGE_START` | Start of port range |
| `DRIFT_PORT_RANGE_END` | End of port range |

### Git

| Field | Description |
|-------|-------------|
| `user_name` | Git user.name set via `git config --local` on open |
| `user_email` | Git user.email set via `git config --local` on open |

### Ports

| Field | Description |
|-------|-------------|
| `range` | `[start, end]` port range for this project |
| `<name> = <port>` | Named port allocations |

Drift checks for port conflicts with other open projects on `drift open`.

### Services

Services are background processes managed by the supervisor.

| Field | Default | Description |
|-------|---------|-------------|
| `name` | required | Identifier |
| `command` | required | Shell command (empty string for agents) |
| `cwd` | `"."` | Working directory relative to repo |
| `restart` | `"never"` | Restart policy: `never`, `on-failure`, `always` |
| `stop_command` | | Custom shutdown command instead of SIGTERM |

### Agents

Agents are services with AI-specific fields. They can run as headless workers or interactive TUI windows.

| Field | Default | Description |
|-------|---------|-------------|
| `agent` | | Agent type: `claude` or `codex` |
| `prompt` | | Task prompt sent on launch |
| `agent_mode` | `"oneshot"` | `oneshot` (headless, runs once) or `interactive` (TUI window) |
| `agent_permissions` | `"full"` | `full` (all tools) or `safe` (read-only tools) |
| `agent_model` | | Model override (e.g. `opus`, `sonnet`, `o3`) |
| `width` | | Column width for interactive agents |

Interactive agents are spawned as terminal windows, not headless processes. Oneshot agents run as regular services.

### Windows

Terminal windows spawned on workspace open.

| Field | Description |
|-------|-------------|
| `name` | Identifier (used in terminal title as `drift:<project>/<name>`) |
| `command` | Shell command to run (omit for plain shell) |
| `width` | Column width: `"60%"`, `"800px"`, or `"0.6"` (proportion) |

### Scratchpad

| Field | Description |
|-------|-------------|
| `file` | Path to scratchpad file relative to repo |

## File Layout

```
~/.config/drift/
  config.toml                  global settings
  projects/<name>.toml         project configs
  templates/                   init templates
  niri-rules.kdl               auto-generated window rules

~/.local/state/drift/<project>/
  logs/                        service and supervisor logs
  workspace.json               saved workspace snapshot
  services.json                supervisor state
  supervisor.pid               supervisor PID

/run/user/$UID/drift/
  emit.sock                    event emission socket
  subscribe.sock               event subscription socket
  daemon.json                  daemon state
```
