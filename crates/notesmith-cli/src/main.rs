use clap::{Parser, Subcommand, ValueEnum};
use notesmith_cli::commands::vault::{OutputFormat, VaultCommand};
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
    /// Vault management commands
    Vault {
        #[command(subcommand)]
        command: VaultCommand,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let global_config = GlobalConfig::load().unwrap_or_default();
    let cwd = std::env::current_dir()?;
    let format: OutputFormat = cli.format.into();

    match cli.command {
        Command::Vault { command } => {
            command.run(&global_config, cli.vault.as_deref(), &cwd, format)?;
        }
    }

    Ok(())
}
