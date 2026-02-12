use crate::config::ServiceProcess;

const FULL_TOOLS: &str = "Bash,Read,Edit,Write,Glob,Grep,WebFetch,WebSearch,NotebookEdit,Task";
const SAFE_TOOLS: &str = "Read,Glob,Grep,WebFetch,WebSearch";

/// Build the shell command string for an agent service.
/// Returns the full command to pass to `sh -c`.
pub fn build_agent_command(svc: &ServiceProcess, project_name: &str) -> String {
    let agent = svc.agent.as_deref().expect("build_agent_command called without agent");
    let raw_prompt = svc.prompt.as_deref().unwrap_or("You are an AI assistant.");
    let full = svc.agent_permissions.as_str() == "full";

    // Build the prompt with project context
    let prompt = format!(
        "You are working on drift project '{}'. {}\n\n\
         Use `drift notify --type agent.completed --title \"<summary>\"` when you finish significant work.\n\
         Use `drift notify --type agent.error --title \"<summary>\"` when you hit errors.",
        project_name, raw_prompt
    );

    let escaped_prompt = shell_escape(&prompt);

    match (agent, svc.agent_mode.as_str()) {
        ("claude", "oneshot") => {
            let mut cmd = String::from("claude -p");
            if full {
                cmd.push_str(" --dangerously-skip-permissions");
            } else {
                cmd.push_str(&format!(" --allowedTools '{SAFE_TOOLS}'"));
            }
            if let Some(model) = &svc.agent_model {
                cmd.push_str(&format!(" --model {}", model));
            }
            format!("{cmd} {escaped_prompt}")
        }
        ("claude", "interactive") => {
            let mut cmd = String::from("claude");
            let tools = if full { FULL_TOOLS } else { SAFE_TOOLS };
            cmd.push_str(&format!(" --allowedTools '{tools}'"));
            if let Some(model) = &svc.agent_model {
                cmd.push_str(&format!(" --model {}", model));
            }
            format!("{cmd} --system-prompt {escaped_prompt}")
        }
        ("codex", "oneshot") => {
            let mut cmd = String::from("codex exec");
            if full {
                cmd.push_str(" -s danger-full-access");
            }
            if let Some(model) = &svc.agent_model {
                cmd.push_str(&format!(" -m {}", model));
            }
            format!("{cmd} {escaped_prompt}")
        }
        ("codex", "interactive") => {
            let mut cmd = String::from("codex");
            if full {
                cmd.push_str(" -s danger-full-access");
            }
            if let Some(model) = &svc.agent_model {
                cmd.push_str(&format!(" -m {}", model));
            }
            format!("{cmd} {escaped_prompt}")
        }
        _ => {
            // Unknown agent type — try running it as a command with the prompt as argument
            format!("{agent} {escaped_prompt}")
        }
    }
}

/// Check if a service is an interactive agent (should be spawned as a window, not a headless service).
pub fn is_interactive_agent(svc: &ServiceProcess) -> bool {
    svc.agent.is_some() && svc.agent_mode == "interactive"
}

fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{RestartPolicy, ServiceProcess};

    fn make_agent(agent: &str, prompt: &str) -> ServiceProcess {
        ServiceProcess {
            name: "test".into(),
            command: String::new(),
            cwd: ".".into(),
            restart: RestartPolicy::Never,
            stop_command: None,
            agent: Some(agent.into()),
            prompt: Some(prompt.into()),
            agent_mode: "oneshot".into(),
            agent_model: None,
            agent_permissions: "full".into(),
        }
    }

    #[test]
    fn claude_oneshot_full() {
        let svc = make_agent("claude", "Review code");
        let cmd = build_agent_command(&svc, "myapp");
        assert!(cmd.starts_with("claude -p --dangerously-skip-permissions"));
        assert!(cmd.contains("drift project"));
        assert!(cmd.contains("myapp"));
        assert!(cmd.contains("Review code"));
    }

    #[test]
    fn claude_oneshot_safe() {
        let mut svc = make_agent("claude", "Review code");
        svc.agent_permissions = "safe".into();
        let cmd = build_agent_command(&svc, "myapp");
        assert!(cmd.contains(&format!("--allowedTools '{SAFE_TOOLS}'")));
        assert!(!cmd.contains("dangerously-skip-permissions"));
    }

    #[test]
    fn claude_interactive() {
        let mut svc = make_agent("claude", "Help me");
        svc.agent_mode = "interactive".into();
        let cmd = build_agent_command(&svc, "myapp");
        assert!(cmd.contains(&format!("--allowedTools '{FULL_TOOLS}'")));
        assert!(cmd.contains("--system-prompt"));
        assert!(!cmd.starts_with("claude -p"));
    }

    #[test]
    fn codex_oneshot_full() {
        let svc = make_agent("codex", "Run tests");
        let cmd = build_agent_command(&svc, "myapp");
        assert!(cmd.starts_with("codex exec -s danger-full-access"));
        assert!(cmd.contains("Run tests"));
    }

    #[test]
    fn codex_with_model() {
        let mut svc = make_agent("codex", "Fix bugs");
        svc.agent_model = Some("o3".into());
        let cmd = build_agent_command(&svc, "myapp");
        assert!(cmd.contains("-m o3"));
    }

    #[test]
    fn is_interactive_agent_true() {
        let mut svc = make_agent("claude", "Help");
        svc.agent_mode = "interactive".into();
        assert!(is_interactive_agent(&svc));
    }

    #[test]
    fn is_interactive_agent_false_for_oneshot() {
        let svc = make_agent("claude", "Help");
        assert!(!is_interactive_agent(&svc));
    }

    #[test]
    fn is_interactive_agent_false_for_regular_service() {
        let svc = ServiceProcess {
            name: "api".into(),
            command: "bash server.sh".into(),
            cwd: ".".into(),
            restart: RestartPolicy::Never,
            stop_command: None,
            agent: None,
            prompt: None,
            agent_mode: "oneshot".into(),
            agent_model: None,
            agent_permissions: "full".into(),
        };
        assert!(!is_interactive_agent(&svc));
    }
}
