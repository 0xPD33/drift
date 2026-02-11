use std::collections::BTreeMap;

use drift_core::config::resolve_repo_path;
use drift_core::registry;

pub fn run(archived: bool) -> anyhow::Result<()> {
    let projects = if archived {
        registry::list_archived()?
    } else {
        registry::list_projects()?
    };

    if projects.is_empty() {
        if archived {
            println!("No archived projects.");
        } else {
            println!("No projects configured.");
        }
        return Ok(());
    }

    let mut grouped: BTreeMap<Option<String>, Vec<(&str, String)>> = BTreeMap::new();
    for p in &projects {
        let folder = p.project.folder.clone();
        let repo = resolve_repo_path(&p.project.repo);
        let repo_display = if let Some(home) = dirs::home_dir() {
            if let Ok(relative) = repo.strip_prefix(&home) {
                format!("~/{}", relative.display())
            } else {
                repo.display().to_string()
            }
        } else {
            repo.display().to_string()
        };
        grouped
            .entry(folder)
            .or_default()
            .push((&p.project.name, repo_display));
    }

    let mut first = true;
    for (folder, entries) in &grouped {
        if !first {
            println!();
        }
        first = false;

        match folder {
            Some(name) => println!("{name}/"),
            None => println!("(ungrouped)"),
        }
        for (name, repo) in entries {
            println!("  {name:<20} {repo}");
        }
    }

    Ok(())
}
