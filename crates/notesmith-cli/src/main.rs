use clap::{Parser, Subcommand, ValueEnum};
use notesmith_cli::commands::{
    daemon::DaemonCommand,
    query::QueryCommand,
    search::SearchCommand,
    vault::{OutputFormat, VaultCommand},
};
use notesmith_config::GlobalConfig;

#[derive(Parser)]
#[command(
    name = "notesmith",
    version,
    about = "A markdown notes app for agentic workflows"
)]
struct Cli {
    /// Vault name or path (overrides directory detection)
    #[arg(long, global = true)]
    vault: Option<String>,

    /// Output format
    #[arg(long, global = true, default_value = "text")]
    format: FormatArg,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Clone, ValueEnum)]
enum FormatArg {
    Text,
    Json,
}

impl From<FormatArg> for OutputFormat {
    fn from(f: FormatArg) -> Self {
        match f {
            FormatArg::Text => OutputFormat::Text,
            FormatArg::Json => OutputFormat::Json,
        }
    }
}

#[derive(Subcommand)]
enum Command {
    /// Daemon lifecycle commands
    Daemon {
        #[command(subcommand)]
        command: DaemonCommand,
    },
    /// Query commands against the daemon cache
    Query {
        #[command(subcommand)]
        command: QueryCommand,
    },
    /// Vault management commands
    Vault {
        #[command(subcommand)]
        command: VaultCommand,
    },
    /// Full-text search against the daemon index
    Search(SearchCommand),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let global_config = GlobalConfig::load().unwrap_or_default();
    let cwd = std::env::current_dir()?;
    let format: OutputFormat = cli.format.into();

    match cli.command {
        Command::Daemon { command } => {
            command.run(&global_config).await?;
        }
        Command::Query { command } => {
            command
                .run(&global_config, cli.vault.as_deref(), &cwd, format)
                .await?;
        }
        Command::Vault { command } => {
            command.run(&global_config, cli.vault.as_deref(), &cwd, format)?;
        }
        Command::Search(command) => {
            command
                .run(&global_config, cli.vault.as_deref(), &cwd, format)
                .await?;
        }
    }

    Ok(())
}
