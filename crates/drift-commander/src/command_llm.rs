use drift_core::config::CommanderConfig;
use drift_core::registry;

use crate::command::VoiceCommand;

/// Try to parse a voice command using an LLM chat completion endpoint.
/// Returns None if the LLM is unavailable or returns an unparseable response,
/// allowing the caller to fall back to pattern matching.
pub fn parse_command_llm(transcript: &str, config: &CommanderConfig) -> Option<VoiceCommand> {
    let project_names = known_project_names();
    let system_prompt = build_system_prompt(&project_names);
    let url = format!("{}/v1/chat/completions", config.llm_endpoint);

    let mut body = serde_json::json!({
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": transcript}
        ],
        "temperature": 0.0,
        "max_tokens": 100,
        "response_format": {"type": "json_object"}
    });

    if let Some(ref model) = config.llm_model {
        body["model"] = serde_json::Value::String(model.clone());
    }

    let agent = crate::make_agent(5);
    let mut resp = match agent.post(&url).send_json(&body) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("commander: LLM request failed: {e}");
            return None;
        }
    };

    if resp.status() != 200 {
        eprintln!("commander: LLM returned status {}", resp.status());
        return None;
    }

    let resp_json: serde_json::Value = match resp.body_mut().read_json() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("commander: LLM response parse error: {e}");
            return None;
        }
    };

    let content = resp_json
        .get("choices")?
        .get(0)?
        .get("message")?
        .get("content")?
        .as_str()?;

    eprintln!("commander: LLM response: {content}");

    parse_llm_json(content)
}

fn build_system_prompt(project_names: &[String]) -> String {
    let projects = if project_names.is_empty() {
        "No projects configured.".to_string()
    } else {
        format!("Available projects: {}", project_names.join(", "))
    };

    format!(
        r#"You interpret voice commands for a workspace manager called "drift". The input is speech-to-text output that may contain errors (wrong words, phonetically similar substitutions, missing words).

{projects}

Available commands:
- open <project>: Open a project (start workspace, services, terminals)
- switch <project>: Switch to an already-open project workspace
- close [project]: Close current or named project
- init <project>: Initialize a new project
- archive <project>: Archive a project
- unarchive <project>: Restore an archived project
- delete <project>: Permanently delete a project
- save: Save current workspace state (window positions, layout)
- restore: Restore previously saved workspaces
- status: Show what's running
- list: List available projects
- logs [service]: Show logs, optionally for a specific service
- events: Show recent events
- ports: Show allocated ports
- add_window <name>: Add a terminal window to current project
- add_service <name> <command>: Add a background service (provide name and shell command)
- add_agent <name> <agent> <prompt>: Add an AI agent (agent is "claude" or "codex", prompt is the task)
- remove_window <name>: Remove a terminal window
- remove_service <name>: Remove a service
- remove_agent <name>: Remove an agent
- notify <message>: Send a notification
- mute: Mute voice feedback
- unmute: Unmute voice feedback

Match project/service/window names from context even if the transcription is imprecise (phonetic similarity, partial match). For example "scratch" might be transcribed as "switch" or "scotch".

Respond with ONLY a JSON object. Fields:
- "command": one of the command names above
- "project": project name if applicable, null otherwise
- "name": service/window/agent name if applicable, null otherwise
- "args": object with extra arguments if needed (e.g. {{"command": "npm start"}} for add_service, {{"agent": "claude", "prompt": "fix tests"}} for add_agent, {{"message": "hello"}} for notify)

Example: {{"command": "open", "project": "scratch", "name": null, "args": null}}
Example: {{"command": "add_service", "project": null, "name": "api", "args": {{"command": "npm start"}}}}
Example: {{"command": "status", "project": null, "name": null, "args": null}}"#
    )
}

fn parse_llm_json(content: &str) -> Option<VoiceCommand> {
    let parsed: serde_json::Value = serde_json::from_str(content).ok()?;

    let command = parsed.get("command")?.as_str()?;

    let project = parsed
        .get("project")
        .and_then(|p| match p {
            serde_json::Value::String(s) if !s.is_empty() && s != "null" => Some(s.clone()),
            _ => None,
        });

    let name = parsed
        .get("name")
        .and_then(|p| match p {
            serde_json::Value::String(s) if !s.is_empty() && s != "null" => Some(s.clone()),
            _ => None,
        });

    let args = parsed.get("args");

    let get_arg = |key: &str| -> Option<String> {
        args?.get(key)?.as_str().map(String::from)
    };

    match command {
        // Project lifecycle
        "open" => Some(VoiceCommand::OpenProject(project?)),
        "switch" => Some(VoiceCommand::SwitchToProject(project?)),
        "close" => Some(VoiceCommand::CloseProject(project)),
        "init" => Some(VoiceCommand::InitProject(project?)),
        "archive" => Some(VoiceCommand::ArchiveProject(project?)),
        "unarchive" => Some(VoiceCommand::UnarchiveProject(project?)),
        "delete" => Some(VoiceCommand::DeleteProject(project?)),
        "save" => Some(VoiceCommand::SaveWorkspace),
        "restore" => Some(VoiceCommand::RestoreWorkspaces),
        // Info / monitoring
        "status" => Some(VoiceCommand::Status),
        "list" => Some(VoiceCommand::ListProjects),
        "logs" => Some(VoiceCommand::ShowLogs(name)),
        "events" => Some(VoiceCommand::ShowEvents),
        "ports" => Some(VoiceCommand::ShowPorts),
        // Configuration
        "add_window" => Some(VoiceCommand::AddWindow(name?)),
        "add_service" => Some(VoiceCommand::AddService {
            name: name?,
            command: get_arg("command")?,
        }),
        "add_agent" => Some(VoiceCommand::AddAgent {
            name: name?,
            agent: get_arg("agent").unwrap_or_else(|| "claude".into()),
            prompt: get_arg("prompt")?,
        }),
        "remove_window" => Some(VoiceCommand::RemoveWindow(name?)),
        "remove_service" => Some(VoiceCommand::RemoveService(name?)),
        "remove_agent" => Some(VoiceCommand::RemoveAgent(name?)),
        // Notifications
        "notify" => Some(VoiceCommand::Notify(
            get_arg("message").or(name).unwrap_or_default(),
        )),
        // Voice control
        "mute" => Some(VoiceCommand::Mute),
        "unmute" => Some(VoiceCommand::Unmute),
        _ => None,
    }
}

fn known_project_names() -> Vec<String> {
    registry::list_projects()
        .unwrap_or_default()
        .into_iter()
        .map(|p| p.project.name)
        .collect()
}
