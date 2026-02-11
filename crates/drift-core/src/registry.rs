use anyhow::Context;

use crate::config::{self, ProjectConfig};
use crate::paths;

pub fn list_projects() -> anyhow::Result<Vec<ProjectConfig>> {
    let dir = paths::projects_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut projects = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "toml") {
            let contents = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            let config: ProjectConfig = toml::from_str(&contents)
                .with_context(|| format!("parsing {}", path.display()))?;
            projects.push(config);
        }
    }

    projects.sort_by(|a, b| {
        let folder_a = a.project.folder.as_deref().unwrap_or("");
        let folder_b = b.project.folder.as_deref().unwrap_or("");
        folder_a
            .cmp(folder_b)
            .then_with(|| a.project.name.cmp(&b.project.name))
    });

    Ok(projects)
}

pub fn find_project(name: &str) -> anyhow::Result<ProjectConfig> {
    config::load_project_config(name)
}

pub fn delete_project(name: &str) -> anyhow::Result<()> {
    let config_path = paths::project_config_path(name);
    if !config_path.exists() {
        anyhow::bail!("Project '{name}' not found");
    }
    std::fs::remove_file(&config_path)
        .with_context(|| format!("removing config file {}", config_path.display()))?;

    // Remove state directory (logs, PIDs, etc.)
    let state = paths::state_dir(name);
    if state.exists() {
        std::fs::remove_dir_all(&state)
            .with_context(|| format!("removing state directory {}", state.display()))?;
    }

    Ok(())
}

pub fn archive_project(name: &str) -> anyhow::Result<()> {
    let config_path = paths::project_config_path(name);
    if !config_path.exists() {
        anyhow::bail!("Project '{name}' not found");
    }

    let archived_dir = paths::archived_projects_dir();
    std::fs::create_dir_all(&archived_dir)
        .with_context(|| format!("creating archived directory {}", archived_dir.display()))?;

    let dest = archived_dir.join(format!("{name}.toml"));
    if dest.exists() {
        anyhow::bail!("Archived project '{name}' already exists");
    }

    std::fs::rename(&config_path, &dest)
        .with_context(|| format!("moving config to {}", dest.display()))?;

    Ok(())
}

pub fn unarchive_project(name: &str) -> anyhow::Result<()> {
    let archived = paths::archived_projects_dir().join(format!("{name}.toml"));
    if !archived.exists() {
        anyhow::bail!("Archived project '{name}' not found");
    }

    let dest = paths::project_config_path(name);
    if dest.exists() {
        anyhow::bail!("Active project '{name}' already exists");
    }

    std::fs::rename(&archived, &dest)
        .with_context(|| format!("restoring config to {}", dest.display()))?;

    Ok(())
}

pub fn list_archived() -> anyhow::Result<Vec<ProjectConfig>> {
    let dir = paths::archived_projects_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut projects = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "toml") {
            let contents = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            let config: ProjectConfig = toml::from_str(&contents)
                .with_context(|| format!("parsing {}", path.display()))?;
            projects.push(config);
        }
    }

    projects.sort_by(|a, b| {
        let folder_a = a.project.folder.as_deref().unwrap_or("");
        let folder_b = b.project.folder.as_deref().unwrap_or("");
        folder_a
            .cmp(folder_b)
            .then_with(|| a.project.name.cmp(&b.project.name))
    });

    Ok(projects)
}
