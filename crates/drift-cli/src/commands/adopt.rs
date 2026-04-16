use std::collections::HashSet;

use anyhow::bail;
use drift_core::config::{ProjectConfig, ProjectMeta, WindowConfig};
use drift_core::paths;
use drift_core::sync::generate_window_name;

pub fn run(workspace_name: &str, project_name: Option<&str>) -> anyhow::Result<()> {
    let name = project_name.unwrap_or(workspace_name);

    let dest = paths::project_config_path(name);
    if dest.exists() {
        bail!("Project '{name}' already exists");
    }

    let mut client = drift_core::niri::NiriClient::connect()
        .map_err(|e| anyhow::anyhow!("Cannot connect to niri: {e}"))?;

    let workspaces = client.workspaces()?;
    let ws = workspaces.iter()
        .find(|ws| ws.name.as_deref() == Some(workspace_name))
        .ok_or_else(|| anyhow::anyhow!("Workspace '{workspace_name}' not found"))?;
    let ws_id = ws.id;

    let windows = client.windows()?;
    let ws_windows: Vec<_> = windows.iter()
        .filter(|w| w.workspace_id == Some(ws_id))
        .collect();

    let global_cfg = drift_core::config::load_global_config().unwrap_or_default();
    let terminal_name = global_cfg.defaults.terminal.to_lowercase();

    let mut existing_names: HashSet<String> = HashSet::new();
    let mut window_configs: Vec<WindowConfig> = Vec::new();

    for win in &ws_windows {
        let app_id = win.app_id.as_deref().unwrap_or("unknown");
        let is_terminal = app_id.to_lowercase().contains(&terminal_name);
        let wname = generate_window_name(app_id, is_terminal, &mut existing_names);
        window_configs.push(WindowConfig {
            name: Some(wname),
            app_id: if is_terminal { None } else { Some(app_id.to_string()) },
            command: None,
            width: None,
            tmux: None,
        });
    }

    let repo = std::env::current_dir()
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_else(|| ".".into());

    let config = ProjectConfig {
        project: ProjectMeta {
            name: name.to_string(),
            repo,
            folder: None,
            icon: None,
        },
        auto_close: true,
        persist_windows: None,
        env: Default::default(),
        git: None,
        ports: None,
        services: None,
        windows: window_configs,
        tmux: None,
        scratchpad: None,
        verification: None,
        dispatcher: None,
    };

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    drift_core::config::save_project_config(name, &config)?;

    println!("Adopted workspace '{workspace_name}' as project '{name}'");
    println!("Config: {}", dest.display());
    println!("Windows captured: {}", config.windows.len());

    Ok(())
}
