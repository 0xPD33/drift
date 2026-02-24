use std::fs;

use crate::command::VoiceCommand;
use drift_core::events::{self, Event};
use drift_core::niri::NiriClient;
use drift_core::paths;
use drift_core::registry;

pub struct ActionResult {
    pub success: bool,
    pub message: String,
}

pub fn execute(command: &VoiceCommand) -> ActionResult {
    match command {
        VoiceCommand::SwitchToProject(name) => switch_to_project(name),
        VoiceCommand::CloseProject(name) => close_project(name.as_deref()),
        VoiceCommand::Status => status(),
        VoiceCommand::ListProjects => list_projects(),
        VoiceCommand::Mute => mute(),
        VoiceCommand::Unmute => unmute(),
        VoiceCommand::Unknown(text) => ActionResult {
            success: false,
            message: format!("I didn't understand: {text}"),
        },
    }
}

fn switch_to_project(name: &str) -> ActionResult {
    let mut client = match NiriClient::connect() {
        Ok(c) => c,
        Err(e) => {
            return ActionResult {
                success: false,
                message: format!("cannot connect to niri: {e}"),
            }
        }
    };

    match client.find_workspace_by_name(name) {
        Ok(Some(_)) => match client.focus_workspace(name) {
            Ok(()) => ActionResult {
                success: true,
                message: format!("switched to {name}"),
            },
            Err(e) => ActionResult {
                success: false,
                message: format!("failed to focus {name}: {e}"),
            },
        },
        Ok(None) => ActionResult {
            success: false,
            message: format!("project {name} not found or not open"),
        },
        Err(e) => ActionResult {
            success: false,
            message: format!("failed to search workspaces: {e}"),
        },
    }
}

fn close_project(name: Option<&str>) -> ActionResult {
    let project_name = match name {
        Some(n) => n.to_string(),
        None => match detect_current_project() {
            Some(n) => n,
            None => {
                return ActionResult {
                    success: false,
                    message: "cannot determine current project".into(),
                }
            }
        },
    };

    let event = Event {
        event_type: "drift.voice.close".into(),
        project: project_name.clone(),
        source: "commander".into(),
        ts: events::iso_now(),
        level: None,
        title: Some(format!("voice close request: {project_name}")),
        body: None,
        meta: None,
        priority: None,
    };

    events::try_emit_event(&event);

    ActionResult {
        success: true,
        message: format!("closing {project_name}"),
    }
}

fn detect_current_project() -> Option<String> {
    let mut client = NiriClient::connect().ok()?;
    let win = client.focused_window().ok()??;
    let ws_id = win.workspace_id?;
    let workspaces = client.workspaces().ok()?;
    workspaces
        .into_iter()
        .find(|ws| ws.id == ws_id)
        .and_then(|ws| ws.name)
}

fn status() -> ActionResult {
    let projects = registry::list_projects().unwrap_or_default();
    if projects.is_empty() {
        return ActionResult {
            success: true,
            message: "no projects configured".into(),
        };
    }

    let names: Vec<&str> = projects.iter().map(|p| p.project.name.as_str()).collect();
    let count = names.len();
    let list = names.join(", ");

    ActionResult {
        success: true,
        message: format!(
            "you have {count} project{}. {}",
            if count == 1 { "" } else { "s" },
            list
        ),
    }
}

fn list_projects() -> ActionResult {
    let projects = registry::list_projects().unwrap_or_default();
    if projects.is_empty() {
        return ActionResult {
            success: true,
            message: "no projects configured".into(),
        };
    }

    let names: Vec<&str> = projects.iter().map(|p| p.project.name.as_str()).collect();
    ActionResult {
        success: true,
        message: names.join(", "),
    }
}

fn mute() -> ActionResult {
    let path = paths::commander_muted_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    match fs::write(&path, "") {
        Ok(()) => ActionResult {
            success: true,
            message: "muted".into(),
        },
        Err(e) => ActionResult {
            success: false,
            message: format!("failed to mute: {e}"),
        },
    }
}

fn unmute() -> ActionResult {
    let path = paths::commander_muted_path();
    if !path.exists() {
        return ActionResult {
            success: true,
            message: "already unmuted".into(),
        };
    }
    match fs::remove_file(&path) {
        Ok(()) => ActionResult {
            success: true,
            message: "unmuted".into(),
        },
        Err(e) => ActionResult {
            success: false,
            message: format!("failed to unmute: {e}"),
        },
    }
}
