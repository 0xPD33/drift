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
