mod commands;

use clap::Parser;
use commands::Commands;

#[derive(Parser)]
#[command(name = "drift", about = "Project-oriented workspace isolation for Niri")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { name, repo, folder } => {
            commands::init::run(&name, repo.as_deref(), folder.as_deref())
        }
        Commands::List => commands::list::run(),
        Commands::Open { name } => commands::open::run(&name),
        Commands::Close { name } => commands::close::run(name.as_deref()),
        Commands::Save { name } => commands::save::run(name.as_deref()),
        Commands::Status => commands::status::run(),
        Commands::To { name } => commands::to::run(&name),
        Commands::Env { name } => commands::env::run(name.as_deref()),
        Commands::NiriRules => commands::niri_rules::run(),
        Commands::Daemon => commands::daemon::run(),
        Commands::Notify { project, r#type, source, level, title, body } => {
            commands::notify::run(project.as_deref(), &r#type, &source, &level, &title, &body)
        }
        Commands::Supervisor { project } => {
            drift_core::supervisor::run_supervisor(&project)
        }
    }
}
