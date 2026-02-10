# drift

Project-oriented workspace isolation for Niri.

Each project gets its own workspace, browser profile, terminal sessions, environment variables, and background services. Switching projects means switching everything at once. Context follows you.

## Concepts

**Project** — the atomic unit. A project has a name, a repo path, a config file, and runtime state. One project = one Niri workspace = a set of terminal windows + services. Projects are the only thing that has state.

**Folder** — organizational grouping. Folders contain projects but have no state themselves. They exist to impose structure when you have 15+ projects across different domains (game-studio, clients, infra, etc.). Folders map to Niri workspace groups for visual separation.

**Drift** — the act of switching project context. `drift to soul-cartridge` saves current state, swaps workspace focus, and loads the target project's full context. Should feel instant.

## Directory Layout

```
~/.config/drift/
  config.toml                   # global settings
  niri-rules.kdl                # auto-generated, included by niri config
  projects/
    soul-cartridge.toml
    hotel-malta.toml
    ctx.toml

~/.local/state/drift/
  soul-cartridge/
    workspace.json              # niri window snapshot
    services.json               # running service PIDs + status
    logs/
      dev-server.log
      tailwind.log

~/.local/share/drift/
  templates/
    rust-gamedev.toml
    nextjs-client.toml
```

## Global Config

```toml
# ~/.config/drift/config.toml

[defaults]
editor = "nvim"
shell = "zsh"

[ports]
base = 3000                     # projects get ranges starting here
range_size = 10                 # ports per project

[notifications]
socket = "/run/user/$UID/drift/notify.sock"
tts = false                     # piper TTS for agent events
```

## Project Config

```toml
# ~/.config/drift/projects/soul-cartridge.toml

[project]
name = "soul-cartridge"
repo = "~/code/soul-cartridge"
folder = "game-studio"
icon = "🎮"

[env]
NODE_ENV = "development"
DATABASE_URL = "postgres://localhost:5432/soul_cartridge"
env_file = ".env.local"         # relative to repo, merged with above

[git]
user_name = "Paddy"
user_email = "paddy@gamestudio.dev"

[ports]
range = [3000, 3009]
dev_server = 3000

[services]
[[services.processes]]
name = "dev-server"
command = "cargo run"
cwd = "."                       # relative to repo
restart = "on-failure"

[[services.processes]]
name = "tailwind"
command = "npx tailwindcss --watch"
restart = "always"

[[windows]]
name = "editor"
command = "nvim"

[[windows]]
name = "git"
command = "lazygit"

[[windows]]
command = ""                    # empty = plain shell

# Optional: use tmux instead of bare terminal windows
# [tmux]
# session = "soul-cartridge"
# windows = [
#   { name = "editor", command = "nvim" },
#   { name = "git", command = "lazygit" },
#   { name = "shell" },
# ]

[scratchpad]
file = "SCRATCHPAD.md"          # relative to repo, gitignored
```

## CLI

```
drift init <name>                 # create project config (optional --template)
drift list                        # all projects grouped by folder
drift to <name>                   # switch to project (save current, open target)
drift open <name>                 # open project without closing current
drift close [name]                # save state, stop services, tear down workspace
drift save                        # snapshot current workspace state
drift status                      # what's running in current project
drift services start|stop|restart [name]
drift scratch                     # open project scratchpad in $EDITOR
drift env                         # print project env vars (for piping)
drift niri-rules                  # regenerate ~/.config/drift/niri-rules.kdl
drift template list|create
```

## Core Flows

### `drift open soul-cartridge`

1. Read `~/.config/drift/projects/soul-cartridge.toml`
2. Check restore tier (query `Workspaces` via niri-ipc for existing named workspaces):
   - **Hot**: named workspace `soul-cartridge` exists → `FocusWorkspace`, done
   - **Cold**: no such workspace → full launch sequence below
3. Cold boot:
   a. Create named workspace via `SetWorkspaceName` (niri-ipc)
   b. Set env vars from config (merge `[env]` + `env_file`)
   c. Set git identity (`git config --local`)
   d. Start services (spawn supervisor, write PIDs)
   e. Spawn terminal windows from `[[windows]]` via niri `Spawn` action (or create tmux session if `[tmux]` defined)
   f. Write state to `~/.local/state/drift/soul-cartridge/`
4. Focus the workspace

### `drift to soul-cartridge`

1. Identify current project from focused workspace
2. `drift save` on current project
3. `drift open soul-cartridge`

### `drift close [name]`

1. Identify target project (argument or current workspace)
2. `drift save` — write state snapshot
3. Stop services (run `stop_command` if defined, otherwise SIGTERM)
4. Close all windows on the workspace (`CloseWindow` via niri-ipc for each)
5. `UnsetWorkspaceName` — workspace becomes dynamic, auto-removed since empty
6. Clean up PID files and supervisor state

Everything dies. State is saved first so cold boot can restore it.

### Spawn Ordering

When `drift open` creates a workspace and spawns windows, the sequence must be deterministic. Niri's IPC processes requests sequentially on a single socket connection, so drift issues them in strict order:

1. `SetWorkspaceName` — create/name the workspace
2. `FocusWorkspace` — focus it (spawned windows land on focused workspace)
3. `Spawn` terminal 1 — lands on now-focused project workspace
4. `Spawn` terminal 2
5. `Spawn` terminal N

Each request completes before the next is sent. No races. If drift needs to verify the workspace exists before focusing, it can query `Workspaces` first.

Services are spawned separately by the supervisor (not via niri `Spawn`), since they're headless processes that don't need a window.

### `drift save`

1. Query `Windows` via niri-ipc — snapshot app IDs, positions, workspace assignments
2. Query `Workspaces` — record workspace-to-output mapping
3. Record tmux session names if applicable
4. Record service PIDs and status
5. Write everything to `~/.local/state/drift/<project>/workspace.json`

## Service Manager

Embedded in `drift`, not a separate daemon. On `drift open`:

- Spawns each process from `[services]`
- Sets project env vars for each process
- Pipes stdout/stderr to `~/.local/state/drift/<project>/logs/<name>.log`
- Handles restart policies (`always`, `on-failure`, `never`)
- Writes PID files
- Emits events to notification bus
- Killed cleanly on `drift close`

Supervisor PID stored in `~/.local/state/drift/<project>/supervisor.pid`.

If a service defines `stop_command` (e.g., `podman stop <name>`), that runs on close instead of SIGTERM.

## Daemon (`drift daemon`)

Single process, single systemd user service. Combines three responsibilities:

1. **Event stream listener** — subscribes to niri's IPC event stream, tracks which project is active, triggers auto-save on workspace switch
2. **Notification bus** — listens on Unix socket, receives events from services/agents/builds, forwards to consumers
3. **Service supervisor** — manages background processes for all open projects

The CLI commands talk to the daemon when it's running. Core operations (`drift open`, `drift close`, `drift list`) work without the daemon via direct niri-ipc — the daemon is an enhancement, not a hard dependency.

### Notification Protocol

Listens on Unix socket at `$XDG_RUNTIME_DIR/drift/notify.sock`.

Protocol: newline-delimited JSON.

```json
{
  "project": "soul-cartridge",
  "source": "dev-server",
  "level": "info",
  "title": "Server started",
  "body": "Listening on :3000",
  "ts": "2026-02-08T15:30:00Z"
}
```

Sending a notification from anywhere:

```bash
echo '{"project":"$DRIFT_PROJECT","source":"agent","level":"success","title":"Done","body":"Auth flow implemented"}' \
  | socat - UNIX-CONNECT:$DRIFT_NOTIFY_SOCK
```

The daemon:
- Receives events from services, agents, builds, anything
- Tags with project context
- Stores ring buffer (last 100 per project)
- Exposes subscription socket for Quickshell
- Forwards to `notify-send` with project name in title
- Active project = urgent priority, background = low priority

This is also the **agent bridge**. Any agent writes JSON to the socket. No SDK needed.

## Environment Isolation

Every process launched through `drift` inherits:

```bash
DRIFT_PROJECT=soul-cartridge
DRIFT_REPO=~/code/soul-cartridge
DRIFT_FOLDER=game-studio
DRIFT_NOTIFY_SOCK=$XDG_RUNTIME_DIR/drift/notify.sock
# plus everything from [env] and env_file
```

Agents discover project context through these env vars. Any agent can emit events:

```bash
echo "{\"project\":\"$DRIFT_PROJECT\",\"source\":\"agent\",\"level\":\"success\",\"title\":\"Done\"}" \
  | socat - UNIX-CONNECT:$DRIFT_NOTIFY_SOCK
```

Git identity set via `git config --local` in the repo on open.

Ports enforced by convention — config declares ranges, `drift status` warns on conflicts.

## Niri Integration

Drift leverages Niri's named workspaces, IPC event stream, and the `niri-ipc` Rust crate for native compositor integration.

### Named Workspaces

Named workspaces (since 0.1.6) persist even when empty and can be targeted by window rules and focus actions.

**Dynamic management** (since 25.01): `drift open` creates named workspaces via `SetWorkspaceName` and `drift close` removes them with `UnsetWorkspaceName`. No need to pre-declare projects in the niri config.

**Output stickiness** (since 25.02): Named workspaces remember which monitor they belong to. Project workspaces stay on their assigned monitor across restarts.

**Per-workspace layout** (since 25.11): Named workspaces can override layout settings (gaps, struts, borders). Future enhancement — see FUTURE.md.

### Event Stream

Niri provides a continuous event stream over its IPC socket. The drift daemon subscribes to this instead of polling. Key events:

- **`WorkspaceActivated`** — workspace focus changed → drift knows which project is active
- **`WorkspacesChanged`** — full workspace state → detect workspace creation/deletion
- **`WindowOpenedOrChanged`** — window opened or changed → track which windows belong to which project
- **`WindowClosed`** — window closed → update project window list
- **`WindowFocusChanged`** — focus changed → update active project tracking
- **`WindowUrgencyChanged`** — urgency state changed → route to notification system

The event stream gives complete state up-front on connection, then streams deltas. Drift's daemon maintains a persistent connection for zero-latency project awareness.

### Config Include

Niri supports `include` directives with live-reload. Drift generates a KDL fragment at `~/.config/drift/niri-rules.kdl` containing workspace declarations and window rules. The user adds one line to their niri config:

```kdl
include "~/.config/drift/niri-rules.kdl"
```

Drift regenerates this file on `drift init`, `drift niri-rules`, and whenever projects are added/removed. Niri live-reloads it automatically. The generated file contains:

```kdl
// Auto-generated by drift. Do not edit.

// game-studio
workspace "soul-cartridge"

// clients
workspace "hotel-malta"
workspace "hotel-dach"

// infra
workspace "drift"
workspace "nixos-config"

window-rule {
    match app-id="foot" title="drift:soul-cartridge"
    open-on-workspace "soul-cartridge"
}
```

Workspaces are ordered by folder, giving spatial consistency in the workspace strip.

### Restart Resilience

Dynamically created workspaces via `SetWorkspaceName` are lost on niri restart. But workspace declarations in `niri-rules.kdl` survive, since niri reads the config include on startup. Drift does both:

- **niri-rules.kdl** — declares all project workspaces statically. Survives niri restart/crash. Source of truth.
- **`SetWorkspaceName`** — used at runtime for newly created projects that haven't been written to the include file yet.

On niri restart, named workspaces reappear (empty). Services and terminals are gone. The user runs `drift open <project>` to re-spawn terminals and services into the already-existing named workspace. The daemon, running as a systemd user service, reconnects to niri's event stream automatically.

### Native Rust IPC

Drift links the `niri-ipc` crate directly — no shelling out to `niri msg`. Typed Rust structs for all requests, responses, and events. The crate follows niri's version (use exact version pin).

```rust
// Direct socket communication, no subprocess overhead
use niri_ipc::{Request, Action, Event};
```

### Key Actions

Actions drift uses via IPC:

- `SetWorkspaceName` / `UnsetWorkspaceName` — create/destroy project workspaces
- `FocusWorkspace` — switch to project workspace by name
- `Spawn` — launch processes through niri (workspace context preserved)
- `MoveWindowToWorkspace` — move windows between projects
- `MoveWorkspaceToMonitor` — assign project to specific monitor
- `FocusWindow` (by id) — focus specific window
- `CloseWindow` — close window by id
- `SetWindowUrgent` — mark background project windows needing attention (renders with urgent border color)

### Key Queries

- `FocusedWindow` — returns window id, title, app_id, workspace_id, is_focused
- `Windows` — all open windows with their workspace assignments
- `Workspaces` — all workspaces with id, name, output
- `FocusedOutput` — which monitor has focus

## Scratchpad

Each project optionally has a `SCRATCHPAD.md` in the repo (gitignored). Shared surface between human and agents.

`drift scratch` opens it in `$EDITOR`. drift-shell shows it in sidebar overlay.

Use cases: agent leaves notes about what it did, human leaves TODO for agent, shared discoveries, session context that doesn't belong in git.

## Build Order

```
Phase 1: Foundation
  - Project config format + parser (TOML)
  - drift init / drift list
  - drift open (cold boot: workspace via niri-ipc, terminal windows via Spawn, services)
  - drift close (stop services, UnsetWorkspaceName)
  - Per-project env loading
  - Generate niri-rules.kdl (workspace declarations + window rules)

Phase 2: Services
  - Service supervisor
  - drift status
  - Log capture
  - Restart policies

Phase 3: Event Stream + Notifications
  - Daemon subscribing to niri event stream
  - Project-aware workspace tracking (WorkspaceActivated → current project)
  - Unix socket notification bus (JSON protocol)
  - notify-send forwarding with project context
  - Agent bridge (anything writes JSON to socket)
  - Window urgency for background project alerts

Phase 4: Persistence
  - drift save (snapshot via niri-ipc Windows/Workspaces queries)
  - Hot restore detection (check if named workspace exists)
  - Auto-save on workspace switch (event stream triggers save)

Phase 5: drift-shell (separate app, same repo)
  - Quickshell bar: project indicator (event stream powered)
  - Sidebar: project tree + service status
  - Notification panel (scoped by project)
  - Scratchpad overlay
  - Launcher integration
```

## Tech

- **Language**: Rust
- **Niri IPC**: `niri-ipc` Rust crate (direct socket, no subprocess)
- **Terminal**: spawns terminal windows on workspace (optional tmux for power users)
- **Notifications**: Unix socket, newline-delimited JSON
- **Config**: TOML
- **State**: JSON
- **Package**: Nix flake

## Repo Structure

```
drift/
  crates/
    drift-core/       # config parsing, niri-ipc integration, project registry
    drift-cli/        # CLI binary
    drift-daemon/     # event stream, notification bus, service supervisor
  shell/
    drift-shell/      # Quickshell/QML — bar, sidebar, notifications, scratchpad overlay
```

`drift-shell` is a separate application in the same repo. It consumes the daemon's subscription socket and calls `drift` CLI for actions. It provides the visual layer (project indicator bar, folder tree sidebar, scoped notification panel) but is not required — drift works fully from the CLI and daemon alone.
