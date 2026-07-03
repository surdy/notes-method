//! `notesmith embed` — run the embedding worker over one or more vaults.
//!
//! The worker is the sole writer of each vault's `embeddings.db` (ADR 0018 §2).
//! This runs one incremental pass: changed notes are re-embedded, unchanged
//! notes are skipped, deleted notes are pruned. The daemon also spawns this on
//! an interval, but it is fully runnable by hand for backfills and debugging.

use clap::{Args, Subcommand};
use notesmith_config::GlobalConfig;
use notesmith_embed::{EmbedWorker, EmbeddingStore, default_embedder};

use crate::commands::vault::OutputFormat;

#[derive(Debug, Args)]
pub struct EmbedCommand {
    #[command(subcommand)]
    command: Option<EmbedSubcommand>,
}

#[derive(Debug, Subcommand)]
enum EmbedSubcommand {
    /// Benchmark this host's brute-force k-NN latency curve (#250, ADR 0018 §5).
    Bench(BenchArgs),
}

#[derive(Debug, Args)]
struct BenchArgs {
    /// Synthetic vector dimension (default matches bge-small-en-v1.5).
    #[arg(long, default_value_t = 384)]
    dim: usize,
    /// Vector-count scales to measure.
    #[arg(
        long,
        value_delimiter = ',',
        default_value = "50000,100000,250000,500000,1000000"
    )]
    scales: Vec<usize>,
    /// Neighbours to retrieve per query.
    #[arg(long, default_value_t = 10)]
    k: usize,
    /// Queries sampled per scale for the latency distribution.
    #[arg(long, default_value_t = 50)]
    queries: usize,
    /// Also embed + search the target vault as a real-content baseline.
    #[arg(long)]
    baseline: bool,
}

impl EmbedCommand {
    pub async fn run(
        &self,
        global_config: &GlobalConfig,
        explicit_vault: Option<&str>,
        format: OutputFormat,
    ) -> anyhow::Result<()> {
        match &self.command {
            Some(EmbedSubcommand::Bench(args)) => {
                self.run_bench(global_config, explicit_vault, format, args)
                    .await
            }
            None => self.run_pass(global_config, explicit_vault, format).await,
        }
    }

    async fn run_pass(
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

    async fn run_bench(
        &self,
        global_config: &GlobalConfig,
        explicit_vault: Option<&str>,
        format: OutputFormat,
        args: &BenchArgs,
    ) -> anyhow::Result<()> {
        use notesmith_embed::{SWITCH_P95_MS, WARN_P95_MS, crossover, synthetic_knn_bench};

        let curve = synthetic_knn_bench(args.dim, &args.scales, args.k, args.queries)?;
        let warn_at = crossover(&curve, WARN_P95_MS);
        let switch_at = crossover(&curve, SWITCH_P95_MS);

        // Optional real-content baseline over the target vault.
        let baseline = if args.baseline {
            let vault_names = resolve_vault_names(global_config, explicit_vault)?;
            let vault_name = vault_names
                .first()
                .ok_or_else(|| anyhow::anyhow!("no vault available for baseline"))?
                .clone();
            let root = global_config
                .vault(&vault_name)
                .ok_or_else(|| anyhow::anyhow!("Vault '{vault_name}' is not registered"))?
                .path
                .clone();
            let embedder = default_embedder()?;
            Some((
                vault_name.clone(),
                notesmith_embed::golden_baseline(
                    &vault_name,
                    &root,
                    embedder.as_ref(),
                    &["project", "meeting notes", "tasks"],
                )?,
            ))
        } else {
            None
        };

        match format {
            OutputFormat::Json => {
                let payload = serde_json::json!({
                    "dim": args.dim,
                    "k": args.k,
                    "queries_per_scale": args.queries,
                    "warn_p95_ms": WARN_P95_MS,
                    "switch_p95_ms": SWITCH_P95_MS,
                    "warn_crossover_count": warn_at,
                    "switch_crossover_count": switch_at,
                    "curve": curve,
                    "baseline": baseline.as_ref().map(|(v, b)| serde_json::json!({
                        "vault": v,
                        "notes_embedded": b.notes_embedded,
                        "chunks_written": b.chunks_written,
                        "embed_ms": b.embed_ms,
                        "search_p50_ms": b.search_p50_ms,
                        "search_p95_ms": b.search_p95_ms,
                    })),
                });
                println!("{}", serde_json::to_string_pretty(&payload)?);
            }
            OutputFormat::Text => {
                println!(
                    "Vector k-NN benchmark (dim={}, k={}, {} queries/scale)",
                    args.dim, args.k, args.queries
                );
                println!(
                    "{:>12}  {:>10}  {:>10}  {:>10}",
                    "vectors", "p50 ms", "p95 ms", "mean ms"
                );
                for r in &curve {
                    println!(
                        "{:>12}  {:>10.2}  {:>10.2}  {:>10.2}",
                        r.count, r.p50_ms, r.p95_ms, r.mean_ms
                    );
                }
                println!(
                    "warn (>{WARN_P95_MS:.0}ms p95) first crossed at: {}",
                    warn_at.map_or("never".to_string(), |c| c.to_string())
                );
                println!(
                    "switch (>{SWITCH_P95_MS:.0}ms p95) first crossed at: {}",
                    switch_at.map_or("never".to_string(), |c| c.to_string())
                );
                if let Some((vault, b)) = &baseline {
                    println!(
                        "baseline[{vault}]: embedded {} notes / {} chunks in {:.0}ms; \
                         search p50 {:.2}ms p95 {:.2}ms",
                        b.notes_embedded,
                        b.chunks_written,
                        b.embed_ms,
                        b.search_p50_ms,
                        b.search_p95_ms
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
