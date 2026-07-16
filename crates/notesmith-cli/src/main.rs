use clap::{Parser, Subcommand, ValueEnum};
use notesmith_cli::commands::{
    ai::AiCommand,
    capture::CaptureCommand,
    clip::ClipCommand,
    copy_html::CopyHtmlCommand,
    daemon::DaemonCommand,
    daily::DailyCommand,
    embed::EmbedCommand,
    ingest::IngestCommand,
    mcp::McpCommand,
    note::NoteCommand,
    periodic::PeriodicCommand,
    query::QueryCommand,
    reindex::ReindexCommand,
    route::RouteCommand,
    search::SearchCommand,
    skill::SkillCommand,
    task::TaskCommand,
    template::TemplateCommand,
    transcribe::TranscribeCommand,
    url_open::UrlOpenCommand,
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

    /// Target daemon base URL (e.g. https://host:8443). Overrides the local
    /// daemon; can also be set via NOTESMITH_URL. A remote daemon is never
    /// auto-started.
    #[arg(long, global = true)]
    url: Option<String>,

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
    /// Rebuild daemon cache and search indexes
    Reindex(ReindexCommand),
    /// Run the embedding worker over one or more vaults
    Embed(EmbedCommand),
    /// Ingest documents from each vault's raw drop folder into notes
    Ingest(IngestCommand),
    /// Transcribe a local audio file into a timestamped Markdown note
    Transcribe(TranscribeCommand),
    /// Task management commands
    Task {
        #[command(subcommand)]
        command: TaskCommand,
    },
    /// Capture a note quickly
    Capture(CaptureCommand),
    /// Clip a web page into the vault
    Clip(ClipCommand),
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
    /// Periodic note commands
    Periodic {
        #[command(subcommand)]
        command: PeriodicCommand,
    },
    /// Vault skill file commands
    Skill {
        #[command(subcommand)]
        command: SkillCommand,
    },
    /// Copy a note as portable HTML
    CopyHtml(CopyHtmlCommand),
    /// Open a notesmith:// deep-link URL
    UrlOpen(UrlOpenCommand),
    /// Headless ACP agent commands (summarize, weekly-digest)
    Ai {
        #[command(subcommand)]
        command: AiCommand,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let global_config = GlobalConfig::load().unwrap_or_default();
    let cwd = std::env::current_dir()?;
    let format: OutputFormat = cli.format.into();

    // Resolve the remote daemon override from `--url` / `NOTESMITH_URL`. The
    // `daemon` lifecycle subcommands always manage the local daemon, so the
    // override does not apply to them.
    if let Some(base) = notesmith_cli::daemon_client::resolve_override(cli.url.as_deref())? {
        if !matches!(cli.command, Command::Daemon { .. }) {
            notesmith_cli::daemon_client::set_remote_override(base);
        }
    }

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
        Command::Reindex(command) => {
            command
                .run(&global_config, cli.vault.as_deref(), format)
                .await?;
        }
        Command::Embed(command) => {
            command
                .run(&global_config, cli.vault.as_deref(), format)
                .await?;
        }
        Command::Ingest(command) => {
            command
                .run(&global_config, cli.vault.as_deref(), format)
                .await?;
        }
        Command::Transcribe(command) => {
            command.run(format).await?;
        }
        Command::Task { command } => {
            command
                .run(&global_config, cli.vault.as_deref(), &cwd, format)
                .await?;
        }
        Command::Capture(command) => {
            command
                .run(&global_config, cli.vault.as_deref(), &cwd, format)
                .await?;
        }
        Command::Clip(command) => {
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
        Command::Periodic { command } => {
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
        Command::UrlOpen(command) => {
            command
                .run(&global_config, cli.vault.as_deref(), &cwd, format)
                .await?;
        }
        Command::Ai { command } => {
            command
                .run(&global_config, cli.vault.as_deref(), &cwd, format)
                .await?;
        }
    }

    Ok(())
}
