---
name: drift
description: Full drift CLI reference — manage projects, services, agents, events, and TTS from inside a drift workspace
allowed-tools: [Bash, Read]
---

# Drift Skill

Complete CLI reference for AI agents working inside drift workspaces on the Niri Wayland compositor.

## Detect Drift Context

Check if you're running inside a drift workspace:
```bash
echo $DRIFT_PROJECT   # project name
echo $DRIFT_REPO      # absolute repo path
echo $DRIFT_FOLDER    # folder group (if set)
echo $DRIFT_NOTIFY_SOCK  # event bus socket path
```

## Project Management

Create a new project:
```bash
drift init <name> --repo ~/code/myapp
drift init <name> --repo ~/code/myapp --folder work
drift init <name> --template rust    # from ~/.config/drift/templates/
```

Open/close workspaces:
```bash
drift open <name>       # create workspace, spawn windows + services + agents
drift close [name]      # save state, stop services, close workspace
drift to <name>         # save current, switch to target
```

List and manage projects:
```bash
drift list              # list all projects grouped by folder
drift list --archived   # list archived projects
drift archive <name>    # reversibly hide a project
drift unarchive <name>  # restore archived project
drift delete <name>     # permanently remove (--yes to skip confirmation)
```

## Configuration

Add items to a project (use `--project <name>` if not in a drift workspace):
```bash
# Services
drift add service <name> <command> --restart on-failure --cwd ./api

# AI agents
drift add agent <name> <agent-type> <prompt> --mode interactive --permissions full --model opus

# Terminal windows
drift add window <name> [command]

# Environment variables
drift add env <key> <value>

# Ports
drift add port <name> <port>
drift add port-range <start> <end>
```

Remove items:
```bash
drift remove service <name>
drift remove agent <name>
drift remove window <name>
drift remove env <key>
drift remove port <name>
drift remove port-range
```

## Inspection

```bash
drift status            # project info, services, ports, recent events
drift env [name]        # print project environment variables
drift ports             # show allocated ports (--project <name>)
drift logs              # list available log files
drift logs <service>    # view service log (-f to follow)
drift events            # view recent events
drift events -f         # follow live event stream
drift events --type "agent.*" --last 50   # filter by type
drift events --all      # events from all projects
drift save [name]       # snapshot workspace state
drift niri-rules        # regenerate niri window rules
drift shell-data        # full project/service/agent state as JSON
```

## Notifications

Emit events to the drift event bus:
```bash
drift notify --type <event-type> --title <title> [body]
drift notify --type <event-type> --title <title> --level <level> --source <source> [body]
```

### Event Types

| Type | When to emit |
|------|-------------|
| `agent.completed` | Finished significant work |
| `agent.error` | Hit an unrecoverable problem |
| `agent.needs_review` | Need human input or review |
| `build.failed` | Build or compilation failure |
| `notification` | General notification (default) |

### Levels

`info` (default), `warn`, `error`, `success`

### Examples

```bash
drift notify --type agent.completed \
  --title "Implemented auth flow" \
  --body "Added JWT middleware to 3 endpoints"

drift notify --type agent.error \
  --title "Build failed" \
  --body "Type error in booking.rs:42"

drift notify --type agent.needs_review \
  --title "PR ready for review" \
  --body "Added 3 files, removed 1"
```

## Commander (TTS)

Voice announcements for events:
```bash
drift commander start       # start TTS event announcer
drift commander stop        # stop announcer
drift commander status      # show status, voice, engine
drift commander say <text>  # test speech output
drift commander mute        # temporarily mute
drift commander unmute      # unmute
```

Speakable events: `agent.completed`, `agent.error`, `agent.needs_review`, `service.crashed`, `build.failed`

## Daemon

The daemon provides event routing, workspace tracking, and auto-save:
```bash
drift daemon    # run in foreground (for systemd)
drift restore   # restore previously-open projects after reboot
drift restore <name>  # restore a specific project
```

## Service Fields Reference

| Field | Default | Values |
|-------|---------|--------|
| `restart` | `never` | `never`, `on-failure`, `always` |
| `agent` | — | `claude`, `codex` |
| `agent_mode` | `oneshot` | `oneshot` (headless), `interactive` (TUI window) |
| `agent_permissions` | `full` | `full` (all tools), `safe` (read-only) |
| `width` | — | `"60%"`, `"800px"`, `"0.6"` |

## Rules

- Emit `agent.completed` only for significant work — not every small edit
- Emit `agent.error` only for unrecoverable problems — not recoverable warnings
- Emit `agent.needs_review` when genuinely needing human input
- Keep titles short (under 10 words) — they are spoken aloud by TTS
- Put details in body, not title
- Don't emit for trivial operations — reduces noise in the event stream
- Check `$DRIFT_PROJECT` before running drift commands to confirm you're in a workspace
