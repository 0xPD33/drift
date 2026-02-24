use clap::Parser;

#[derive(Parser)]
#[command(name = "drift-commander")]
struct Cli {
    /// One-shot TTS: speak this text and exit
    #[arg(long)]
    say: Option<String>,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if let Some(text) = cli.say {
        drift_commander::say_text(&text)
    } else {
        drift_commander::run_commander()
    }
}
