use clap::{Parser, Subcommand, ValueEnum};
use notesmith_cli::commands::{
    copy_html::CopyHtmlCommand,
    daemon::DaemonCommand,
    daily::DailyCommand,
    inbox::InboxCommand,
    mcp::McpCommand,
    note::NoteCommand,
    query::QueryCommand,
    route::RouteCommand,
    search::SearchCommand,
    skill::SkillCommand,
    task::TaskCommand,
    template::TemplateCommand,
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
    /// Note CRUD commands against the daemon API
    Note {
        #[command(subcommand)]
        command: NoteCommand,
    },
    /// MCP server commands
    Mcp {
        #[command(subcommand)]
        command: McpCommand,
    },
    /// Vault management commands
    Vault {
        #[command(subcommand)]
        command: VaultCommand,
    },
    /// Full-text search against the daemon index
    Search(SearchCommand),
    /// Task management commands
    Task {
        #[command(subcommand)]
        command: TaskCommand,
    },
    /// Inbox quick-capture commands
    Inbox {
        #[command(subcommand)]
        command: InboxCommand,
    },
    /// Template management commands
    Template {
        #[command(subcommand)]
        command: TemplateCommand,
    },
    /// Route notes to their destination folder
    Route {
        #[command(subcommand)]
        command: RouteCommand,
    },
    /// Daily note commands
    Daily {
        #[command(subcommand)]
        command: DailyCommand,
    },
    /// Vault skill file commands
    Skill {
        #[command(subcommand)]
        command: SkillCommand,
    },
    /// Copy a note as portable HTML
    CopyHtml(CopyHtmlCommand),
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
        Command::Note { command } => {
            command
                .run(&global_config, cli.vault.as_deref(), &cwd, format)
                .await?;
        }
        Command::Mcp { command } => {
            command
                .run(&global_config, cli.vault.as_deref(), &cwd)
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
        Command::Task { command } => {
            command
                .run(&global_config, cli.vault.as_deref(), &cwd, format)
                .await?;
        }
        Command::Inbox { command } => {
            command
                .run(&global_config, cli.vault.as_deref(), &cwd, format)
                .await?;
        }
        Command::Template { command } => {
            command
                .run(&global_config, cli.vault.as_deref(), &cwd, format)
                .await?;
        }
        Command::Route { command } => {
            command
                .run(&global_config, cli.vault.as_deref(), &cwd, format)
                .await?;
        }
        Command::Daily { command } => {
            command
                .run(&global_config, cli.vault.as_deref(), &cwd, format)
                .await?;
        }
        Command::Skill { command } => {
            command
                .run(&global_config, cli.vault.as_deref(), &cwd, format)
                .await?;
        }
        Command::CopyHtml(command) => {
            command.run(&global_config, cli.vault.as_deref(), &cwd)?;
        }
    }

    Ok(())
}
