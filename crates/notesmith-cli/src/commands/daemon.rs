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
    /// Stop the running daemon
    Stop,
    /// Restart the daemon (stop then start)
    Restart {
        /// Bind address for the daemon
        #[arg(long)]
        bind: Option<String>,
    },
    /// Show daemon status
    Status,
}

impl DaemonCommand {
    pub async fn run(&self, global_config: &GlobalConfig) -> anyhow::Result<()> {
        match self {
            DaemonCommand::Start { bind } => {
                notesmith_http::serve_configured_vaults(global_config, bind.as_deref()).await
            }
            DaemonCommand::Stop => stop_daemon(global_config).await,
            DaemonCommand::Restart { bind } => {
                // Stop if running, then start in background
                let _ = stop_daemon(global_config).await;
                // Wait a moment for port to free up
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                eprintln!("Starting Notesmith daemon...");
                crate::daemon_client::ensure_daemon(global_config).await?;
                let bind_addr = bind.as_deref().unwrap_or(&global_config.daemon.bind);
                eprintln!("Daemon restarted on {bind_addr}");
                Ok(())
            }
            DaemonCommand::Status => show_status(global_config).await,
        }
    }
}

async fn stop_daemon(global_config: &GlobalConfig) -> anyhow::Result<()> {
    if !crate::daemon_client::is_daemon_running(global_config).await {
        eprintln!("Daemon is not running.");
        return Ok(());
    }

    let url = crate::daemon_client::daemon_url(global_config)?
        .join("admin/shutdown")
        .map_err(|_| anyhow::anyhow!("invalid URL"))?;

    let response = reqwest::Client::new()
        .post(url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await;

    match response {
        Ok(resp) if resp.status().is_success() => {
            eprintln!("Daemon stopped.");
            Ok(())
        }
        Ok(resp) => {
            anyhow::bail!("Shutdown request returned status {}", resp.status());
        }
        Err(e) => {
            // Connection reset is expected — daemon shut down mid-response
            if e.is_connect() || e.is_request() {
                eprintln!("Daemon stopped.");
                Ok(())
            } else {
                Err(e.into())
            }
        }
    }
}

async fn show_status(global_config: &GlobalConfig) -> anyhow::Result<()> {
    if !crate::daemon_client::is_daemon_running(global_config).await {
        println!("Daemon is not running.");
        return Ok(());
    }

    let url = crate::daemon_client::daemon_url(global_config)?
        .join("api/status")
        .map_err(|_| anyhow::anyhow!("invalid URL"))?;

    let response = reqwest::Client::new()
        .get(url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await?;

    let body: serde_json::Value = response.json().await?;
    println!("{}", serde_json::to_string_pretty(&body)?);
    Ok(())
}
