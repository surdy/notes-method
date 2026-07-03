//! `notesmith embed` — run the embedding worker over one or more vaults.
//!
//! The worker is the sole writer of each vault's `embeddings.db` (ADR 0018 §2).
//! This runs one incremental pass: changed notes are re-embedded, unchanged
//! notes are skipped, deleted notes are pruned. The daemon also spawns this on
//! an interval, but it is fully runnable by hand for backfills and debugging.

use clap::Args;
use notesmith_config::GlobalConfig;
use notesmith_embed::{EmbedWorker, EmbeddingStore, default_embedder};

use crate::commands::vault::OutputFormat;

#[derive(Debug, Args)]
pub struct EmbedCommand {}

impl EmbedCommand {
    pub async fn run(
        &self,
        global_config: &GlobalConfig,
        explicit_vault: Option<&str>,
        format: OutputFormat,
    ) -> anyhow::Result<()> {
        let vault_names = resolve_vault_names(global_config, explicit_vault)?;
        let embedder = default_embedder()?;
        let mut reports = Vec::new();

        for vault_name in vault_names {
            let registration = global_config
                .vault(&vault_name)
                .ok_or_else(|| anyhow::anyhow!("Vault '{vault_name}' is not registered"))?;
            let root = registration.path.clone();
            let db_path = notesmith_embed::embeddings_db_path(&vault_name)?;
            let store = EmbeddingStore::open(&db_path)?;

            let worker = EmbedWorker::new(vault_name.clone(), root, &store, embedder.as_ref());
            let report = worker.run()?;
            reports.push((vault_name, report));
        }

        match format {
            OutputFormat::Json => {
                let payload: Vec<_> = reports
                    .iter()
                    .map(|(vault, r)| {
                        serde_json::json!({
                            "vault": vault,
                            "embedded": r.embedded,
                            "skipped": r.skipped,
                            "deleted": r.deleted,
                            "failed": r.failed,
                            "chunks_written": r.chunks_written,
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&payload)?);
            }
            OutputFormat::Text => {
                for (vault, r) in &reports {
                    println!(
                        "Embedded {vault}: {} re-embedded, {} unchanged, {} deleted, \
                         {} failed ({} chunks)",
                        r.embedded, r.skipped, r.deleted, r.failed, r.chunks_written
                    );
                }
            }
        }

        Ok(())
    }
}

fn resolve_vault_names(
    global_config: &GlobalConfig,
    explicit_vault: Option<&str>,
) -> anyhow::Result<Vec<String>> {
    if let Some(vault_name) = explicit_vault {
        if global_config.vault(vault_name).is_none() {
            anyhow::bail!("Vault '{vault_name}' is not registered");
        }
        return Ok(vec![vault_name.to_string()]);
    }

    let mut vault_names = global_config.vaults.keys().cloned().collect::<Vec<_>>();
    vault_names.sort();
    if vault_names.is_empty() {
        anyhow::bail!("No vaults registered. Add vaults to ~/.config/notesmith/config.toml");
    }
    Ok(vault_names)
}
