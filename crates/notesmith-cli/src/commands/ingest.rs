//! `notesmith ingest` — run the drop-folder ingestion worker over one or more
//! vaults (ADR 0022, issue #263).
//!
//! Each vault's raw drop folder (`[ingest] raw_dir`, default `raw/`) is scanned
//! for documents an external tool has dropped in; each is extracted into a
//! provenance-tracked sidecar note under `[ingest] notes_dir` (default
//! `ingested/`). Raw files are never moved or deleted (keep-in-place, §2). This
//! runs one incremental pass and is fully runnable by hand, like `embed`.

use notesmith_config::{GlobalConfig, VaultConfig};
use notesmith_ingest::{IngestReport, IngestWorker};

use crate::commands::vault::OutputFormat;

#[derive(Debug, clap::Args)]
pub struct IngestCommand {}

impl IngestCommand {
    pub async fn run(
        &self,
        global_config: &GlobalConfig,
        explicit_vault: Option<&str>,
        format: OutputFormat,
    ) -> anyhow::Result<()> {
        let vault_names = resolve_vault_names(global_config, explicit_vault)?;
        let mut reports = Vec::new();

        for vault_name in vault_names {
            let registration = global_config
                .vault(&vault_name)
                .ok_or_else(|| anyhow::anyhow!("Vault '{vault_name}' is not registered"))?;
            let root = registration.path.clone();
            let vault_config = VaultConfig::load_from_vault(&root)?;
            let ingest = &vault_config.ingest;

            let worker = IngestWorker::new(&root, &ingest.raw_dir, &ingest.notes_dir);
            let report = worker.run()?;
            reports.push((vault_name, report));
        }

        match format {
            OutputFormat::Json => {
                let payload: Vec<_> = reports
                    .iter()
                    .map(|(vault, r)| ingest_json(vault, r))
                    .collect();
                println!("{}", serde_json::to_string_pretty(&payload)?);
            }
            OutputFormat::Text => {
                for (vault, r) in &reports {
                    println!(
                        "Ingested {vault}: {} new, {} unchanged, {} reingested, {} renamed, \
                         {} failed, {} unsupported, {} orphaned",
                        r.ingested(),
                        r.unchanged(),
                        r.reingested(),
                        r.renamed(),
                        r.failed(),
                        r.unsupported(),
                        r.orphaned.len()
                    );
                }
            }
        }

        Ok(())
    }
}

fn ingest_json(vault: &str, r: &IngestReport) -> serde_json::Value {
    serde_json::json!({
        "vault": vault,
        "ingested": r.ingested(),
        "unchanged": r.unchanged(),
        "reingested": r.reingested(),
        "renamed": r.renamed(),
        "failed": r.failed(),
        "unsupported": r.unsupported(),
        "orphaned": r.orphaned,
    })
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
