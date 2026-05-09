use clap::Subcommand;
use notesmith_config::GlobalConfig;

#[derive(Debug, Subcommand)]
pub enum DaemonCommand {
    /// Start the Notesmith HTTP daemon
    Start {
        /// Bind address for the daemon
        #[arg(long)]
        bind: Option<String>,
    },
}

impl DaemonCommand {
    pub async fn run(&self, global_config: &GlobalConfig) -> anyhow::Result<()> {
        match self {
            DaemonCommand::Start { bind } => {
                notesmith_http::serve_configured_vaults(global_config, bind.as_deref()).await
            }
        }
    }
}
