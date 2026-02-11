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
        Commands::Init { name, repo, folder, template } => {
            commands::init::run(&name, repo.as_deref(), folder.as_deref(), template.as_deref())
        }
        Commands::List { archived } => commands::list::run(archived),
        Commands::Open { name } => commands::open::run(&name),
        Commands::Close { name } => commands::close::run(name.as_deref()),
        Commands::Archive { name } => commands::archive::archive(&name),
        Commands::Unarchive { name } => commands::archive::unarchive(&name),
        Commands::Delete { name, yes } => commands::delete::run(&name, yes),
        Commands::Save { name } => commands::save::run(name.as_deref()),
        Commands::Status => commands::status::run(),
        Commands::To { name } => commands::to::run(&name),
        Commands::Env { name } => commands::env::run(name.as_deref()),
        Commands::Events { r#type, last, all, follow, project } => {
            commands::events::run(r#type.as_deref(), last, all, follow, project.as_deref())
        }
        Commands::NiriRules => commands::niri_rules::run(),
        Commands::Daemon => commands::daemon::run(),
        Commands::Logs { service, follow, project } => {
            commands::logs::run(service.as_deref(), follow, project.as_deref())
        }
        Commands::Add { command } => commands::add::run(command),
        Commands::Remove { command } => commands::remove::run(command),
        Commands::Ports { project } => commands::ports::run(project.as_deref()),
        Commands::Notify { project, r#type, source, level, title, body } => {
            commands::notify::run(project.as_deref(), &r#type, &source, &level, &title, &body)
        }
        Commands::Commander { command } => match command {
            commands::CommanderCommand::Start => commands::commander::start(),
            commands::CommanderCommand::Stop => commands::commander::stop(),
            commands::CommanderCommand::Status => commands::commander::status(),
            commands::CommanderCommand::Say { text } => commands::commander::say(&text),
            commands::CommanderCommand::Mute => commands::commander::mute(),
            commands::CommanderCommand::Unmute => commands::commander::unmute(),
        },
        Commands::Supervisor { project } => {
            drift_core::supervisor::run_supervisor(&project)
        }
        Commands::RunCommander => {
            drift_core::commander::run_commander()
        }
    }
}
