use drift_core::registry;

#[derive(Debug, Clone, PartialEq)]
pub enum VoiceCommand {
    // Project lifecycle
    SwitchToProject(String),
    OpenProject(String),
    CloseProject(Option<String>),
    InitProject(String),
    ArchiveProject(String),
    UnarchiveProject(String),
    DeleteProject(String),
    SaveWorkspace,
    RestoreWorkspaces,
    // Info / monitoring
    Status,
    ListProjects,
    ShowLogs(Option<String>),
    ShowEvents,
    ShowPorts,
    // Configuration
    AddWindow(String),
    AddService { name: String, command: String },
    AddAgent { name: String, agent: String, prompt: String },
    RemoveWindow(String),
    RemoveService(String),
    RemoveAgent(String),
    // Notifications
    Notify(String),
    // Voice control
    Mute,
    Unmute,
    Unknown(String),
}

pub fn parse_command(transcript: &str) -> VoiceCommand {
    let normalized = transcript.trim().to_lowercase();
    // Strip trailing punctuation that STT often adds
    let normalized = normalized.trim_end_matches(|c: char| c.is_ascii_punctuation());
    let normalized = normalize_whitespace(normalized);

    if normalized.is_empty() {
        return VoiceCommand::Unknown(transcript.to_string());
    }

    // Mute/unmute (exact-ish matches)
    match normalized.as_str() {
        "mute" | "be quiet" | "shut up" | "silence" => return VoiceCommand::Mute,
        "unmute" | "speak" | "talk" => return VoiceCommand::Unmute,
        _ => {}
    }

    // Status
    match normalized.as_str() {
        "status" | "what's running" | "what is running" | "whats running" => {
            return VoiceCommand::Status
        }
        _ => {}
    }

    // List projects
    match normalized.as_str() {
        "list" | "list projects" | "what projects" | "projects" => {
            return VoiceCommand::ListProjects
        }
        _ => {}
    }

    // Switch to project: "switch to X", "go to X", "open X"
    let project_names = known_project_names();
    for prefix in &["switch to ", "go to ", "open "] {
        if let Some(rest) = normalized.strip_prefix(prefix) {
            let rest = rest.trim();
            if !rest.is_empty() {
                let name = resolve_project_name(rest, &project_names);
                return VoiceCommand::SwitchToProject(name);
            }
        }
    }

    // Close project: "close X", "close this", "close"
    if let Some(rest) = normalized.strip_prefix("close") {
        let rest = rest.trim();
        if rest.is_empty() || rest == "this" {
            return VoiceCommand::CloseProject(None);
        }
        let name = resolve_project_name(rest, &project_names);
        return VoiceCommand::CloseProject(Some(name));
    }

    VoiceCommand::Unknown(transcript.to_string())
}

fn normalize_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn known_project_names() -> Vec<String> {
    registry::list_projects()
        .unwrap_or_default()
        .into_iter()
        .map(|p| p.project.name)
        .collect()
}

fn resolve_project_name(spoken: &str, projects: &[String]) -> String {
    if let Some(m) = fuzzy_match_project(spoken, projects) {
        m
    } else {
        spoken.to_string()
    }
}

fn fuzzy_match_project(spoken: &str, projects: &[String]) -> Option<String> {
    let spoken_lower = spoken.to_lowercase();

    // Exact match (case-insensitive)
    for p in projects {
        if p.to_lowercase() == spoken_lower {
            return Some(p.clone());
        }
    }

    // Contains match
    for p in projects {
        let p_lower = p.to_lowercase();
        if p_lower.contains(&spoken_lower) || spoken_lower.contains(&p_lower) {
            return Some(p.clone());
        }
    }

    // Levenshtein distance <= 2
    let mut best: Option<(String, usize)> = None;
    for p in projects {
        let dist = levenshtein(&spoken_lower, &p.to_lowercase());
        if dist <= 2 {
            if best.as_ref().map_or(true, |(_, d)| dist < *d) {
                best = Some((p.clone(), dist));
            }
        }
    }
    best.map(|(name, _)| name)
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (m, n) = (a.len(), b.len());

    let mut prev = (0..=n).collect::<Vec<_>>();
    let mut curr = vec![0; n + 1];

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1)
                .min(curr[j - 1] + 1)
                .min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_switch_to() {
        assert_eq!(
            parse_command("switch to myapp"),
            VoiceCommand::SwitchToProject("myapp".into())
        );
    }

    #[test]
    fn parse_go_to() {
        assert_eq!(
            parse_command("go to webapp"),
            VoiceCommand::SwitchToProject("webapp".into())
        );
    }

    #[test]
    fn parse_open() {
        assert_eq!(
            parse_command("open dashboard"),
            VoiceCommand::SwitchToProject("dashboard".into())
        );
    }

    #[test]
    fn parse_close_named() {
        assert_eq!(
            parse_command("close myapp"),
            VoiceCommand::CloseProject(Some("myapp".into()))
        );
    }

    #[test]
    fn parse_close_this() {
        assert_eq!(
            parse_command("close this"),
            VoiceCommand::CloseProject(None)
        );
    }

    #[test]
    fn parse_close_bare() {
        assert_eq!(parse_command("close"), VoiceCommand::CloseProject(None));
    }

    #[test]
    fn parse_status() {
        assert_eq!(parse_command("status"), VoiceCommand::Status);
        assert_eq!(parse_command("what's running"), VoiceCommand::Status);
        assert_eq!(parse_command("what is running"), VoiceCommand::Status);
    }

    #[test]
    fn parse_list_projects() {
        assert_eq!(parse_command("list"), VoiceCommand::ListProjects);
        assert_eq!(parse_command("list projects"), VoiceCommand::ListProjects);
        assert_eq!(parse_command("what projects"), VoiceCommand::ListProjects);
        assert_eq!(parse_command("projects"), VoiceCommand::ListProjects);
    }

    #[test]
    fn parse_mute() {
        assert_eq!(parse_command("mute"), VoiceCommand::Mute);
        assert_eq!(parse_command("be quiet"), VoiceCommand::Mute);
        assert_eq!(parse_command("shut up"), VoiceCommand::Mute);
        assert_eq!(parse_command("silence"), VoiceCommand::Mute);
    }

    #[test]
    fn parse_unmute() {
        assert_eq!(parse_command("unmute"), VoiceCommand::Unmute);
        assert_eq!(parse_command("speak"), VoiceCommand::Unmute);
        assert_eq!(parse_command("talk"), VoiceCommand::Unmute);
    }

    #[test]
    fn parse_unknown() {
        assert_eq!(
            parse_command("abracadabra"),
            VoiceCommand::Unknown("abracadabra".into())
        );
    }

    #[test]
    fn parse_normalizes_case() {
        assert_eq!(
            parse_command("SWITCH TO MyApp"),
            VoiceCommand::SwitchToProject("myapp".into())
        );
        assert_eq!(parse_command("STATUS"), VoiceCommand::Status);
        assert_eq!(parse_command("MUTE"), VoiceCommand::Mute);
    }

    #[test]
    fn parse_strips_punctuation() {
        assert_eq!(parse_command("Status."), VoiceCommand::Status);
        assert_eq!(parse_command("mute!"), VoiceCommand::Mute);
        assert_eq!(parse_command("list projects."), VoiceCommand::ListProjects);
    }

    #[test]
    fn parse_normalizes_whitespace() {
        assert_eq!(
            parse_command("  switch  to   myapp  "),
            VoiceCommand::SwitchToProject("myapp".into())
        );
    }

    #[test]
    fn parse_empty_is_unknown() {
        assert_eq!(parse_command(""), VoiceCommand::Unknown("".into()));
        assert_eq!(parse_command("  "), VoiceCommand::Unknown("  ".into()));
    }

    #[test]
    fn levenshtein_identical() {
        assert_eq!(levenshtein("hello", "hello"), 0);
    }

    #[test]
    fn levenshtein_one_off() {
        assert_eq!(levenshtein("hello", "helo"), 1);
        assert_eq!(levenshtein("hello", "jello"), 1);
    }

    #[test]
    fn levenshtein_two_off() {
        assert_eq!(levenshtein("hello", "hllo"), 1);
        assert_eq!(levenshtein("kitten", "sitten"), 1);
        assert_eq!(levenshtein("abc", "aec"), 1);
    }

    #[test]
    fn levenshtein_empty() {
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("abc", ""), 3);
        assert_eq!(levenshtein("", ""), 0);
    }

    #[test]
    fn fuzzy_exact_match() {
        let projects = vec!["myapp".into(), "webapp".into()];
        assert_eq!(
            fuzzy_match_project("myapp", &projects),
            Some("myapp".into())
        );
    }

    #[test]
    fn fuzzy_case_insensitive() {
        let projects = vec!["MyApp".into()];
        assert_eq!(
            fuzzy_match_project("myapp", &projects),
            Some("MyApp".into())
        );
    }

    #[test]
    fn fuzzy_contains_match() {
        let projects = vec!["my-cool-app".into()];
        assert_eq!(
            fuzzy_match_project("cool", &projects),
            Some("my-cool-app".into())
        );
    }

    #[test]
    fn fuzzy_levenshtein_match() {
        let projects = vec!["myapp".into()];
        assert_eq!(
            fuzzy_match_project("myap", &projects),
            Some("myapp".into())
        );
    }

    #[test]
    fn fuzzy_no_match() {
        let projects = vec!["myapp".into()];
        assert_eq!(fuzzy_match_project("completely-different", &projects), None);
    }
}
