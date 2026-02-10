use anyhow::bail;
use drift_core::{env, registry};

pub fn run(name: Option<&str>) -> anyhow::Result<()> {
    let project_name = match name {
        Some(n) => n.to_string(),
        None => match std::env::var("DRIFT_PROJECT") {
            Ok(val) if !val.is_empty() => val,
            _ => bail!(
                "No project specified. Pass a project name or set $DRIFT_PROJECT."
            ),
        },
    };

    let project = registry::find_project(&project_name)?;
    let env_map = env::build_env(&project)?;
    println!("{}", env::format_env_exports(&env_map));
    Ok(())
}
