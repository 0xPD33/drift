use std::fs;
use std::path::Path;

/// Pre-trust a directory for Claude Code by writing to `~/.claude.json`.
///
/// Ensures `projects.<repo_path>.hasTrustDialogAccepted = true` in the file,
/// preserving all other fields.
pub fn ensure_claude_trust(repo_path: &Path) -> std::io::Result<()> {
    let home = dirs::home_dir()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no home directory"))?;
    let claude_json = home.join(".claude.json");

    let mut root: serde_json::Value = if claude_json.exists() {
        let contents = fs::read_to_string(&claude_json)?;
        serde_json::from_str(&contents).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e)
        })?
    } else {
        serde_json::json!({})
    };

    let repo_key = repo_path.to_string_lossy();

    let projects = root
        .as_object_mut()
        .ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "~/.claude.json is not an object")
        })?
        .entry("projects")
        .or_insert_with(|| serde_json::json!({}));

    let project_entry = projects
        .as_object_mut()
        .ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "projects is not an object")
        })?
        .entry(repo_key.as_ref())
        .or_insert_with(|| serde_json::json!({}));

    project_entry
        .as_object_mut()
        .ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "project entry is not an object")
        })?
        .insert(
            "hasTrustDialogAccepted".into(),
            serde_json::Value::Bool(true),
        );

    let json = serde_json::to_string_pretty(&root).map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::Other, e)
    })?;

    let tmp = claude_json.with_extension("json.tmp");
    fs::write(&tmp, &json)?;
    fs::rename(&tmp, &claude_json)?;

    Ok(())
}
