use clap::Args;

#[derive(Args)]
pub struct PostDispatchArgs {
    /// Project name
    project: String,
    /// Task ID
    task_id: String,
}

pub fn run(args: PostDispatchArgs) -> anyhow::Result<()> {
    drift_core::post_dispatch::process_completed_dispatch(&args.project, &args.task_id)
}
