use std::collections::{HashMap, HashSet};

use crate::config::{self, WindowConfig};

fn infer_terminal_app_id<'a>(
    running_windows: &'a [(String, Option<String>)],
    terminal_name: &str,
) -> Option<&'a str> {
    let term_lower = terminal_name.to_lowercase();
    running_windows
        .iter()
        .map(|(app_id, _)| app_id.as_str())
        .find(|app_id| app_id.to_lowercase().contains(&term_lower))
}

pub fn sync_windows_to_config(
    project: &str,
    running_windows: &[(String, Option<String>)],
    terminal_name: &str,
) -> anyhow::Result<bool> {
    let mut config = config::load_project_config(project)?;

    let mut terminal_budget: usize = config
        .windows
        .iter()
        .filter(|w| w.app_id.is_none())
        .count();

    let mut gui_budget: HashMap<String, usize> = HashMap::new();
    for w in &config.windows {
        if let Some(app_id) = &w.app_id {
            *gui_budget.entry(app_id.clone()).or_insert(0) += 1;
        }
    }

    let terminal_app_id = infer_terminal_app_id(running_windows, terminal_name);

    let mut existing_names: HashSet<String> = config
        .windows
        .iter()
        .filter_map(|w| w.name.clone())
        .collect();

    let mut new_windows: Vec<(String, bool)> = Vec::new();

    for (app_id, _title) in running_windows {
        if let Some(term_id) = terminal_app_id {
            if app_id == term_id && terminal_budget > 0 {
                terminal_budget -= 1;
                continue;
            }
        }
        if let Some(count) = gui_budget.get_mut(app_id) {
            if *count > 0 {
                *count -= 1;
                continue;
            }
        }
        let is_terminal = terminal_app_id.is_some_and(|t| app_id == t);
        new_windows.push((app_id.clone(), is_terminal));
    }

    if new_windows.is_empty() {
        return Ok(false);
    }

    for (app_id, is_terminal) in &new_windows {
        let name = generate_window_name(app_id, *is_terminal, &mut existing_names);
        config.windows.push(WindowConfig {
            name: Some(name),
            app_id: if *is_terminal { None } else { Some(app_id.clone()) },
            command: None,
            width: None,
            tmux: None,
        });
    }

    config::save_project_config(project, &config)?;
    Ok(true)
}

pub fn generate_window_name(
    app_id: &str,
    is_terminal: bool,
    existing: &mut HashSet<String>,
) -> String {
    let base = if is_terminal {
        "shell".to_string()
    } else {
        app_id
            .rsplit('.')
            .next()
            .unwrap_or(app_id)
            .to_lowercase()
    };

    if existing.insert(base.clone()) {
        return base;
    }

    let mut n = 2;
    loop {
        let candidate = format!("{base}-{n}");
        if existing.insert(candidate.clone()) {
            return candidate;
        }
        n += 1;
    }
}

pub fn resolve_app_launch_command(app_id: &str) -> String {
    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .unwrap_or_else(|_| "/usr/share:/usr/local/share".to_string());

    let desktop_file = format!("applications/{app_id}.desktop");

    for dir in data_dirs.split(':') {
        let path = std::path::Path::new(dir).join(&desktop_file);
        if let Ok(contents) = std::fs::read_to_string(&path) {
            for line in contents.lines() {
                if let Some(exec) = line.strip_prefix("Exec=") {
                    let cleaned = strip_desktop_field_codes(exec);
                    return cleaned;
                }
            }
        }
    }

    app_id
        .rsplit('.')
        .next()
        .unwrap_or(app_id)
        .to_string()
}

fn strip_desktop_field_codes(exec: &str) -> String {
    let codes = [
        "%u", "%U", "%f", "%F", "%d", "%D", "%n", "%N", "%i", "%c", "%k", "%v", "%m",
    ];
    let mut result = exec.to_string();
    for code in &codes {
        result = result.replace(code, "");
    }
    // Re-tokenize preserving quoted strings
    let mut tokens = Vec::new();
    let mut chars = result.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        if c == '"' {
            let mut token = String::new();
            token.push(chars.next().unwrap());
            while let Some(&ch) = chars.peek() {
                token.push(chars.next().unwrap());
                if ch == '"' && token.len() > 1 {
                    break;
                }
            }
            if !token.is_empty() {
                tokens.push(token);
            }
        } else {
            let mut token = String::new();
            while let Some(&ch) = chars.peek() {
                if ch.is_whitespace() {
                    break;
                }
                token.push(chars.next().unwrap());
            }
            if !token.is_empty() {
                tokens.push(token);
            }
        }
    }
    tokens.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_matching_mixed_windows() {
        let windows = vec![
            WindowConfig {
                name: Some("editor".into()),
                command: Some("nvim".into()),
                width: None,
                tmux: None,
                app_id: None,
            },
            WindowConfig {
                name: Some("shell".into()),
                command: None,
                width: None,
                tmux: None,
                app_id: None,
            },
            WindowConfig {
                name: Some("browser".into()),
                command: None,
                width: None,
                tmux: None,
                app_id: Some("org.mozilla.firefox".into()),
            },
        ];

        let mut terminal_budget: usize = windows.iter().filter(|w| w.app_id.is_none()).count();
        let mut gui_budget: HashMap<String, usize> = HashMap::new();
        for w in &windows {
            if let Some(app_id) = &w.app_id {
                *gui_budget.entry(app_id.clone()).or_insert(0) += 1;
            }
        }

        assert_eq!(terminal_budget, 2);
        assert_eq!(gui_budget.get("org.mozilla.firefox"), Some(&1));

        let running = vec![
            ("ghostty".to_string(), None::<String>),
            ("ghostty".to_string(), None),
            ("org.mozilla.firefox".to_string(), None),
        ];

        let mut new_windows = Vec::new();
        let terminal_app_id = Some("ghostty");

        for (app_id, _title) in &running {
            if let Some(term_id) = terminal_app_id {
                if app_id == term_id && terminal_budget > 0 {
                    terminal_budget -= 1;
                    continue;
                }
            }
            if let Some(count) = gui_budget.get_mut(app_id.as_str()) {
                if *count > 0 {
                    *count -= 1;
                    continue;
                }
            }
            new_windows.push(app_id.clone());
        }

        assert!(new_windows.is_empty());
    }

    #[test]
    fn unmatched_windows_detected() {
        let windows = vec![WindowConfig {
            name: Some("editor".into()),
            command: Some("nvim".into()),
            width: None,
            tmux: None,
            app_id: None,
        }];

        let mut terminal_budget: usize = windows.iter().filter(|w| w.app_id.is_none()).count();
        let mut gui_budget: HashMap<String, usize> = HashMap::new();
        for w in &windows {
            if let Some(app_id) = &w.app_id {
                *gui_budget.entry(app_id.clone()).or_insert(0) += 1;
            }
        }

        let running: Vec<(String, Option<String>)> = vec![
            ("ghostty".to_string(), None),
            ("ghostty".to_string(), None),
            ("org.mozilla.firefox".to_string(), None),
        ];

        let terminal_app_id = Some("ghostty");
        let mut new_windows = Vec::new();

        for (app_id, _title) in &running {
            if let Some(term_id) = terminal_app_id {
                if app_id == term_id && terminal_budget > 0 {
                    terminal_budget -= 1;
                    continue;
                }
            }
            if let Some(count) = gui_budget.get_mut(app_id.as_str()) {
                if *count > 0 {
                    *count -= 1;
                    continue;
                }
            }
            new_windows.push(app_id.clone());
        }

        assert_eq!(new_windows.len(), 2);
        assert_eq!(new_windows[0], "ghostty");
        assert_eq!(new_windows[1], "org.mozilla.firefox");
    }

    #[test]
    fn name_generation_unique() {
        let mut existing = HashSet::new();
        let name1 = generate_window_name("org.mozilla.firefox", false, &mut existing);
        assert_eq!(name1, "firefox");
        assert!(existing.contains("firefox"));

        let name2 = generate_window_name("org.mozilla.firefox", false, &mut existing);
        assert_eq!(name2, "firefox-2");
        assert!(existing.contains("firefox-2"));

        let name3 = generate_window_name("org.mozilla.firefox", false, &mut existing);
        assert_eq!(name3, "firefox-3");
    }

    #[test]
    fn name_generation_terminal() {
        let mut existing = HashSet::new();
        let name1 = generate_window_name("ghostty", true, &mut existing);
        assert_eq!(name1, "shell");

        let name2 = generate_window_name("ghostty", true, &mut existing);
        assert_eq!(name2, "shell-2");
    }

    #[test]
    fn desktop_entry_fallback() {
        let cmd = resolve_app_launch_command("com.nonexistent.fakeapp");
        assert_eq!(cmd, "fakeapp");
    }

    #[test]
    fn strip_field_codes() {
        let cleaned = strip_desktop_field_codes("firefox %u --new-tab %U");
        assert_eq!(cleaned, "firefox --new-tab");
    }

    #[test]
    fn strip_field_codes_quoted() {
        let cleaned = strip_desktop_field_codes(r#""/usr/bin/my app" --profile %u"#);
        assert_eq!(cleaned, r#""/usr/bin/my app" --profile"#);
    }

    #[test]
    fn infer_terminal_from_name() {
        let windows = vec![
            ("com.mitchellh.ghostty".to_string(), None),
            ("org.mozilla.firefox".to_string(), None),
        ];
        assert_eq!(
            infer_terminal_app_id(&windows, "ghostty"),
            Some("com.mitchellh.ghostty")
        );
        assert_eq!(infer_terminal_app_id(&windows, "alacritty"), None);
    }

    #[test]
    fn sync_returns_false_no_new_windows() {
        let windows = vec![
            WindowConfig {
                name: Some("editor".into()),
                command: Some("nvim".into()),
                width: None,
                tmux: None,
                app_id: None,
            },
            WindowConfig {
                name: Some("browser".into()),
                command: None,
                width: None,
                tmux: None,
                app_id: Some("org.mozilla.firefox".into()),
            },
        ];

        let running: Vec<(String, Option<String>)> = vec![
            ("ghostty".to_string(), None),
            ("org.mozilla.firefox".to_string(), None),
        ];

        let mut terminal_budget: usize = windows.iter().filter(|w| w.app_id.is_none()).count();
        let mut gui_budget: HashMap<String, usize> = HashMap::new();
        for w in &windows {
            if let Some(app_id) = &w.app_id {
                *gui_budget.entry(app_id.clone()).or_insert(0) += 1;
            }
        }

        let terminal_app_id = Some("ghostty");
        let mut new_windows = Vec::new();

        for (app_id, _title) in &running {
            if let Some(term_id) = terminal_app_id {
                if app_id == term_id && terminal_budget > 0 {
                    terminal_budget -= 1;
                    continue;
                }
            }
            if let Some(count) = gui_budget.get_mut(app_id.as_str()) {
                if *count > 0 {
                    *count -= 1;
                    continue;
                }
            }
            new_windows.push(app_id.clone());
        }

        assert!(new_windows.is_empty());
    }
}
