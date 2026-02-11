pub mod archive;
pub mod close;
pub mod commander;
pub mod daemon;
pub mod delete;
pub mod env;
pub mod init;
pub mod list;
pub mod logs;
pub mod niri_rules;
pub mod notify;
pub mod open;
pub mod ports;
pub mod save;
pub mod status;
pub mod to;

use clap::Subcommand;

#[derive(Subcommand)]
pub enum CommanderCommand {
    /// Start the TTS event announcer
    Start,
    /// Stop the TTS event announcer
    Stop,
    /// Show commander status
    Status,
    /// Test TTS output
    Say {
        /// Text to speak
        text: String,
    },
    /// Temporarily mute announcements
    Mute,
    /// Unmute announcements
    Unmute,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize a new project
    Init {
        /// Project name
        name: String,
        /// Repository path
        #[arg(long)]
        repo: Option<String>,
        /// Folder group
        #[arg(long)]
        folder: Option<String>,
    },
    /// List all projects
    List {
        /// Show archived projects instead
        #[arg(long)]
        archived: bool,
    },
    /// Open a project workspace
    Open {
        /// Project name
        name: String,
    },
    /// Close a project workspace
    Close {
        /// Project name (default: current workspace)
        name: Option<String>,
    },
    /// Archive a project (reversible)
    Archive {
        /// Project name
        name: String,
    },
    /// Restore an archived project
    Unarchive {
        /// Project name
        name: String,
    },
    /// Permanently delete a project and its state
    Delete {
        /// Project name
        name: String,
        /// Skip confirmation
        #[arg(long)]
        yes: bool,
    },
    /// Save current workspace state
    Save {
        /// Project name (default: current workspace)
        name: Option<String>,
    },
    /// Show status of current project
    Status,
    /// Switch to another project (saves current first)
    To {
        /// Project name
        name: String,
    },
    /// Print environment variables for a project
    Env {
        /// Project name (default: current workspace)
        name: Option<String>,
    },
    /// Regenerate niri-rules.kdl
    NiriRules,
    /// Run the drift daemon (for systemd, runs in foreground)
    Daemon,
    /// Send a notification to the drift notification bus
    Notify {
        /// Project name (default: $DRIFT_PROJECT)
        #[arg(long)]
        project: Option<String>,
        /// Event type (e.g. agent.completed, build.failed)
        #[arg(long, default_value = "notification")]
        r#type: String,
        /// Source identifier
        #[arg(long, default_value = "cli")]
        source: String,
        /// Event level (info, warn, error, success)
        #[arg(long, default_value = "info")]
        level: String,
        /// Event title
        title: String,
        /// Event body
        #[arg(default_value = "")]
        body: String,
    },
    /// View service logs
    Logs {
        /// Service name (omit to list available logs)
        service: Option<String>,
        /// Follow log output (tail -f)
        #[arg(short, long)]
        follow: bool,
        /// Project name (default: current)
        #[arg(long)]
        project: Option<String>,
    },
    /// Show allocated ports for a project
    Ports {
        /// Project name (default: current)
        #[arg(long)]
        project: Option<String>,
    },
    /// TTS event announcer
    Commander {
        #[command(subcommand)]
        command: CommanderCommand,
    },
    /// Internal: run service supervisor (not for direct use)
    #[command(name = "_supervisor", hide = true)]
    Supervisor {
        /// Project name
        project: String,
    },
    /// Internal: run commander process (not for direct use)
    #[command(name = "_commander", hide = true)]
    RunCommander,
}
