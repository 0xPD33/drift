use anyhow::Context;

use drift_core::events::{self, Event};

pub fn run(
    project: Option<&str>,
    event_type: &str,
    source: &str,
    level: &str,
    title: &str,
    body: &str,
) -> anyhow::Result<()> {
    let project_name = match project {
        Some(p) => p.to_string(),
        None => std::env::var("DRIFT_PROJECT")
            .ok()
            .filter(|s| !s.is_empty())
            .context("No project specified. Use --project or set $DRIFT_PROJECT")?,
    };

    let event = Event {
        event_type: event_type.to_string(),
        project: project_name,
        source: source.to_string(),
        ts: events::iso_now(),
        level: Some(level.to_string()),
        title: Some(title.to_string()),
        body: if body.is_empty() { None } else { Some(body.to_string()) },
        meta: None,
        priority: None,
    };

    events::emit_event(&event).context("sending event to drift daemon")?;

    println!("Event sent");
    Ok(())
}
