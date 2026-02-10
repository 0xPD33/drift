---
name: drift-agent
description: Announce work via drift event bus, check project context, and request TTS speech
allowed-tools: [Bash, Read]
---

# Drift Agent Skill

Emit notifications to the drift event bus and access project context during agent execution.

## When to Use

- **Announce task completion** — emit `agent.completed` after finishing significant work
- **Report errors** — emit `agent.error` when hitting unrecoverable problems
- **Request human review** — emit `agent.needs_review` when needing user input
- **Check project state** — inspect services, workspace, ports, environment
- **Direct speech** — use commander TTS for immediate announcements (testing/one-off)

## Announce Work via drift notify

When you finish significant work:
```bash
drift notify --type agent.completed --title "Implemented auth flow" --body "Added JWT to 3 endpoints"
```

When you encounter an unrecoverable error:
```bash
drift notify --type agent.error --title "Build failed" --body "Type error in booking.rs:42"
```

When human review or input is needed:
```bash
drift notify --type agent.needs_review --title "PR ready for review" --body "Added 3 files, removed 1"
```

## Check Project Context

View project status and service state:
```bash
drift status
```

List available log files:
```bash
drift logs
```

View recent service output:
```bash
drift logs <service>
```

Check allocated ports:
```bash
drift ports
```

Print all environment variables:
```bash
drift env
```

## Direct Speech (Testing)

For immediate announcements or testing:
```bash
drift commander say "task complete"
```

## Rules

- **Emit `agent.completed`** only for significant work — not for every small edit
- **Emit `agent.error`** only for unrecoverable problems — not for recoverable warnings
- **Emit `agent.needs_review`** when genuinely needing human input — not for every change
- **Keep titles short** (under 10 words) — they are spoken aloud
- **Put details in body** — not in the title
- **Don't emit for trivial operations** — saves noise in the event stream
- **Check `$DRIFT_PROJECT` env var** to detect if running in drift context

## Examples

Task completion with detail:
```bash
drift notify --type agent.completed \
  --title "Database schema updated" \
  --body "Added indices to users and transactions tables. Ran 5 migrations. Performance tests passing."
```

Error with location:
```bash
drift notify --type agent.error \
  --title "Type error in payment processor" \
  --body "Cannot assign string to number field at /src/payment.rs:156"
```

Review needed:
```bash
drift notify --type agent.needs_review \
  --title "Implementation requires architecture decision" \
  --body "Created 2 approaches for caching layer. Need feedback on which fits project goals."
```
