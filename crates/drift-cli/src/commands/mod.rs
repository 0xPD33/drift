pub mod add;
pub mod adopt;
pub mod archive;
pub mod close;
pub mod commander;
pub mod daemon;
#[cfg(feature = "dispatch")]
pub mod dispatch;
pub mod delete;
pub mod env;
pub mod events;
pub mod init;
pub mod list;
pub mod logs;
pub mod niri_rules;
pub mod notify;
pub mod open;
#[cfg(feature = "dispatch")]
pub mod post_dispatch;
pub mod ports;
pub mod remove;
pub mod restore;
#[cfg(feature = "dispatch")]
pub mod review;
pub mod save;
pub mod shell_data;
pub mod status;
#[cfg(feature = "dispatch")]
pub mod task;
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
    /// Download voice control models (VAD + STT)
    Setup,
    /// Record wake word samples and build model
    Train {
        /// Wake word name (default: from config)
        #[arg(long)]
        word: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum Commands {
    // ── Workspace ──────────────────────────────────────────────
    /// Open a project workspace
    #[command(next_help_heading = "Workspace")]
    Open {
        /// Project name
        name: String,
        /// Attach to an existing workspace instead of creating a new one (piggyback)
        #[arg(long)]
        attach: Option<String>,
    },
    /// Close a project workspace
    Close {
        /// Project name (default: current workspace)
        name: Option<String>,
    },
    /// Switch to another project (saves current first)
    To {
        /// Project name
        name: String,
    },
    /// Save current workspace state
    Save {
        /// Project name (default: current workspace)
        name: Option<String>,
    },
    /// Show status of current project
    Status,
    /// Restore previously-open projects
    Restore {
        /// Project name (omit to restore entire session)
        name: Option<String>,
    },

    // ── Project ────────────────────────────────────────────────
    /// Adopt an unmanaged niri workspace as a drift project
    #[command(next_help_heading = "Project")]
    Adopt {
        /// Workspace name to adopt
        workspace_name: String,
        /// Project name (default: workspace name)
        #[arg(long)]
        project_name: Option<String>,
    },
    /// Initialize a new project
    Init {
        /// Project name
        name: String,
        /// Repository path (default: current directory)
        repo: Option<String>,
        /// Folder group
        #[arg(long)]
        folder: Option<String>,
        /// Template name (from ~/.config/drift/templates/)
        #[arg(long, short)]
        template: Option<String>,
    },
    /// List all projects
    List {
        /// Show archived projects instead
        #[arg(long)]
        archived: bool,
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
    /// Add items to a project (services, windows, env vars, ports)
    Add {
        #[command(subcommand)]
        command: add::AddCommand,
    },
    /// Remove items from a project (services, windows, env vars, ports)
    Remove {
        #[command(subcommand)]
        command: remove::RemoveCommand,
    },

    // ── Inspect ────────────────────────────────────────────────
    /// Print environment variables for a project
    #[command(next_help_heading = "Inspect")]
    Env {
        /// Project name (default: current workspace)
        name: Option<String>,
    },
    /// Show allocated ports for a project
    Ports {
        /// Project name (default: current)
        #[arg(long)]
        project: Option<String>,
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
    /// View event stream
    Events {
        /// Filter by event type (supports * glob, e.g. "agent.*")
        #[arg(long, short = 't')]
        r#type: Option<String>,
        /// Number of events to show
        #[arg(long, default_value = "20")]
        last: usize,
        /// Show events from all projects
        #[arg(long)]
        all: bool,
        /// Follow live event stream
        #[arg(short, long)]
        follow: bool,
        /// Project name (default: current)
        #[arg(long)]
        project: Option<String>,
    },
    /// Send a notification to the drift event bus
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

    // ── Advanced ───────────────────────────────────────────────
    /// TTS event announcer
    #[command(next_help_heading = "Advanced")]
    Commander {
        #[command(subcommand)]
        command: CommanderCommand,
    },

    // ── Tasks ──────────────────────────────────────────────────
    /// Manage task queue
    #[cfg(feature = "dispatch")]
    #[command(next_help_heading = "Tasks")]
    Task {
        #[command(subcommand)]
        command: task::TaskCommand,
    },
    /// Review completed agent tasks
    #[cfg(feature = "dispatch")]
    Review {
        #[command(subcommand)]
        command: review::ReviewCommand,
    },
    /// Dispatch the next task to an agent
    #[cfg(feature = "dispatch")]
    Dispatch(dispatch::DispatchArgs),

    // ── Hidden (internal) ──────────────────────────────────────
    /// Run the drift daemon (for systemd, runs in foreground)
    #[command(hide = true)]
    Daemon,
    /// Regenerate niri-rules.kdl
    #[command(hide = true)]
    NiriRules,
    /// Output project/service/agent state as JSON (for shell integration)
    #[command(hide = true)]
    ShellData,
    /// Internal: run service supervisor (not for direct use)
    #[command(name = "_supervisor", hide = true)]
    Supervisor {
        /// Project name
        project: String,
    },
    /// Internal: process completed dispatch (not for direct use)
    #[cfg(feature = "dispatch")]
    #[command(name = "_post-dispatch", hide = true)]
    PostDispatch(post_dispatch::PostDispatchArgs),
}
