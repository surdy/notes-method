//! `notesmith transcribe` — transcribe a local audio file, or drain each
//! vault's pending-transcription queue into notes (ADR 0023 §4/§7, issues #271,
//! #270).
//!
//! Three modes:
//! - `notesmith transcribe <audio>` transcribes a single file and prints/writes
//!   the note (the #204 entry point and manual smoke path).
//! - `notesmith transcribe --drain` runs the [`TranscribeWorker`] over the
//!   pending queue of one or all vaults, mirroring `notesmith embed`. The
//!   daemon also spawns this on an interval, but it is runnable by hand.
//! - `notesmith transcribe --from-vtt <path|->` renders an *already existing*
//!   WebVTT transcript into a note without transcribing anything. This is the
//!   entry point connectors use (ADR 0025's 2026-09-04 amendment): a Teams
//!   transcript arrives as VTT, and a subprocess connector cannot call the
//!   shared renderer directly, so without this it would have to reimplement the
//!   body format and drift from it.
//!
//! The real engine (whisper.cpp) is compiled in only with `--features
//! local-whisper`; lean builds use the stub (empty transcript).

use std::path::PathBuf;

use chrono::Local;
use notesmith_clip::YoutubeAudioAcquirer;
use notesmith_config::{GlobalConfig, VaultConfig};
use notesmith_transcribe::{
    AudioInput, LOCAL_WHISPER_COMPILED, MediaMeta, TranscribeWorker, TranscriptionQueue,
    default_transcriber, parse_vtt, queue_db_path, render_transcript_note,
    render_transcript_note_with_frontmatter,
};

use crate::commands::vault::OutputFormat;

/// The `source_type` recorded for locally transcribed audio.
const SOURCE_TYPE_AUDIO: &str = "audio";

/// The `source_type` recorded for a VTT transcript with no explicit override —
/// Teams is the only producer we have (ADR 0025).
const SOURCE_TYPE_TEAMS: &str = "teams";

#[derive(Debug, clap::Args)]
pub struct TranscribeCommand {
    /// Path to the audio file to transcribe (WAV is decoded natively).
    /// Omit when using `--drain`.
    audio: Option<PathBuf>,

    /// Drain each vault's pending-transcription queue into notes instead of
    /// transcribing a single file.
    #[arg(long)]
    drain: bool,

    /// Write the rendered note to this file instead of printing it
    /// (single-file mode only).
    #[arg(long)]
    output: Option<PathBuf>,

    /// Extra tags to add to the note (in addition to the mandatory `inbox`).
    #[arg(long = "tag", value_name = "TAG")]
    tags: Vec<String>,

    /// Render an existing WebVTT transcript instead of transcribing audio.
    /// `-` reads the VTT from stdin, which is how a connector avoids ever
    /// writing transcript content to disk.
    #[arg(long, value_name = "PATH")]
    from_vtt: Option<PathBuf>,

    /// Note title for `--from-vtt` (defaults to the source).
    #[arg(long)]
    title: Option<String>,

    /// `source_url` for `--from-vtt` — the dedup key. Defaults to the VTT path,
    /// or `stdin` when reading a pipe, so a connector should pass a stable
    /// external identifier.
    #[arg(long)]
    source: Option<String>,

    /// `source_type` for `--from-vtt` (default `teams`).
    #[arg(long)]
    source_type: Option<String>,

    /// JSON object merged into the rendered frontmatter — how vault-specific
    /// keys (`event_id`, meeting and customer wikilinks) reach the note without
    /// core knowing the vault's model (ADR 0025 Decision 5). Keys already
    /// rendered are replaced; new keys are appended.
    #[arg(long, value_name = "JSON")]
    frontmatter: Option<String>,
}

impl TranscribeCommand {
    pub async fn run(
        &self,
        global_config: &GlobalConfig,
        explicit_vault: Option<&str>,
        format: OutputFormat,
    ) -> anyhow::Result<()> {
        if self.drain {
            return self.run_drain(global_config, explicit_vault, format).await;
        }
        if self.from_vtt.is_some() {
            return self.run_from_vtt(format);
        }
        self.run_single(format)
    }

    /// Render an existing WebVTT transcript into a note. No engine is involved,
    /// so this works identically in lean builds.
    fn run_from_vtt(&self, format: OutputFormat) -> anyhow::Result<()> {
        let path = self.from_vtt.as_ref().expect("checked by caller");
        let from_stdin = path.as_os_str() == "-";

        let vtt = if from_stdin {
            std::io::read_to_string(std::io::stdin())?
        } else {
            if !path.is_file() {
                anyhow::bail!("VTT file not found: {}", path.display());
            }
            std::fs::read_to_string(path)?
        };

        let transcript = parse_vtt(&vtt);
        if transcript.segments.is_empty() {
            anyhow::bail!("no cues parsed from the VTT input");
        }

        let source = self.source.clone().unwrap_or_else(|| {
            if from_stdin {
                "stdin".to_string()
            } else {
                path.display().to_string()
            }
        });
        let title = self.title.clone().unwrap_or_else(|| {
            if from_stdin {
                String::new()
            } else {
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string()
            }
        });

        let meta = MediaMeta {
            // ADR 0025: a Transcript Note is `kind: transcript`.
            kind: Some("transcript".to_string()),
            title,
            source: source.clone(),
            source_type: self
                .source_type
                .clone()
                .unwrap_or_else(|| SOURCE_TYPE_TEAMS.to_string()),
            // Cue intervals can overlap, so the last cue is not necessarily the
            // latest — take the maximum end rather than `segments.last()`.
            duration: transcript
                .segments
                .iter()
                .map(|seg| seg.end)
                .filter(|end| end.is_finite() && *end >= 0.0)
                .fold(None::<f64>, |acc, end| {
                    Some(acc.map_or(end, |m: f64| m.max(end)))
                })
                .map(|end| end as u64),
            channel: None,
            published: None,
        };

        let extra = parse_extra_frontmatter(self.frontmatter.as_deref())?;
        let note = render_transcript_note_with_frontmatter(
            &meta,
            &transcript,
            &self.tags,
            Local::now(),
            &extra,
        );

        if let Some(output) = &self.output {
            std::fs::write(output, &note)?;
        }

        let speakers = transcript
            .segments
            .iter()
            .filter_map(|s| s.speaker.as_deref())
            .collect::<std::collections::BTreeSet<_>>()
            .len();

        match format {
            OutputFormat::Json => {
                let payload = serde_json::json!({
                    "source": meta.source,
                    "segments": transcript.segments.len(),
                    "speakers": speakers,
                    "output": self.output.as_ref().map(|p| p.display().to_string()),
                    "note": if self.output.is_none() { Some(note.clone()) } else { None },
                });
                println!("{}", serde_json::to_string_pretty(&payload)?);
            }
            OutputFormat::Text => {
                if let Some(output) = &self.output {
                    println!(
                        "Rendered {} ({} segments, {speakers} speakers) -> {}",
                        meta.source,
                        transcript.segments.len(),
                        output.display()
                    );
                } else {
                    print!("{note}");
                }
            }
        }
        Ok(())
    }

    fn run_single(&self, format: OutputFormat) -> anyhow::Result<()> {
        let audio = self
            .audio
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("provide an audio path, or use --drain"))?;
        if !audio.is_file() {
            anyhow::bail!("Audio file not found: {}", audio.display());
        }
        warn_if_lean();

        let transcriber = default_transcriber();
        let transcript = transcriber
            .transcribe(&AudioInput::Path(audio.clone()))
            .map_err(|e| anyhow::anyhow!("transcription failed: {e}"))?;

        let title = audio
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let meta = MediaMeta {
            kind: None,
            title,
            source: audio.display().to_string(),
            source_type: SOURCE_TYPE_AUDIO.to_string(),
            duration: transcript.segments.last().map(|s| s.end.max(0.0) as u64),
            channel: None,
            published: None,
        };
        let note = render_transcript_note(&meta, &transcript, &self.tags, Local::now());

        if let Some(output) = &self.output {
            std::fs::write(output, &note)?;
        }

        match format {
            OutputFormat::Json => {
                let payload = serde_json::json!({
                    "source": meta.source,
                    "language": transcript.language,
                    "segments": transcript.segments.len(),
                    "compiled_in": LOCAL_WHISPER_COMPILED,
                    "output": self.output.as_ref().map(|p| p.display().to_string()),
                    "note": if self.output.is_none() { Some(note.clone()) } else { None },
                });
                println!("{}", serde_json::to_string_pretty(&payload)?);
            }
            OutputFormat::Text => {
                if let Some(output) = &self.output {
                    println!(
                        "Transcribed {} ({} segments) -> {}",
                        meta.source,
                        transcript.segments.len(),
                        output.display()
                    );
                } else {
                    print!("{note}");
                }
            }
        }
        Ok(())
    }

    async fn run_drain(
        &self,
        global_config: &GlobalConfig,
        explicit_vault: Option<&str>,
        format: OutputFormat,
    ) -> anyhow::Result<()> {
        warn_if_lean();
        let vault_names = resolve_vault_names(global_config, explicit_vault)?;
        let mut reports = Vec::new();

        for vault_name in vault_names {
            let registration = global_config
                .vault(&vault_name)
                .ok_or_else(|| anyhow::anyhow!("Vault '{vault_name}' is not registered"))?;
            let root = registration.path.clone();
            let vault_config = VaultConfig::load_from_vault(&root)?;

            let queue_path = queue_db_path(&vault_name)
                .map_err(|e| anyhow::anyhow!("resolve queue for '{vault_name}': {e}"))?;
            let queue = TranscriptionQueue::open(&queue_path)
                .map_err(|e| anyhow::anyhow!("open queue for '{vault_name}': {e}"))?;

            let notes_dir = root.join(&vault_config.transcribe.notes_dir);
            let queue_for_task = queue;
            // The worker is CPU-bound (Whisper) and the YouTube acquirer builds
            // its own async runtime, so it must run off the async runtime thread.
            let vault_for_task = vault_name.clone();
            let report = tokio::task::spawn_blocking(move || {
                let worker = TranscribeWorker::with_default_transcriber(queue_for_task, notes_dir)
                    .with_acquirer(Box::new(YoutubeAudioAcquirer::new()));
                worker.run()
            })
            .await
            .map_err(|e| anyhow::anyhow!("transcribe worker task for '{vault_for_task}': {e}"))?
            .map_err(|e| anyhow::anyhow!("transcribe worker for '{vault_name}': {e}"))?;
            reports.push((vault_name, report));
        }

        match format {
            OutputFormat::Json => {
                let payload: Vec<_> = reports
                    .iter()
                    .map(|(vault, r)| {
                        serde_json::json!({
                            "vault": vault,
                            "transcribed": r.transcribed,
                            "failed": r.failed,
                            "skipped": r.skipped,
                            "notes": r.notes,
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&payload)?);
            }
            OutputFormat::Text => {
                for (vault, r) in &reports {
                    println!(
                        "Transcribed {vault}: {} written, {} failed, {} skipped",
                        r.transcribed, r.failed, r.skipped
                    );
                }
            }
        }
        Ok(())
    }
}

/// Parse `--frontmatter` into YAML mapping entries.
///
/// JSON is a YAML subset, so the connector can pass a plain JSON object and it
/// renders as ordinary frontmatter. Rejecting non-objects here keeps a typo
/// from silently producing a note with no `event_id`, which would then never
/// join to its meeting.
fn parse_extra_frontmatter(raw: Option<&str>) -> anyhow::Result<serde_yaml::Mapping> {
    let Some(raw) = raw else {
        return Ok(serde_yaml::Mapping::new());
    };
    let value: serde_yaml::Value = serde_yaml::from_str(raw)
        .map_err(|error| anyhow::anyhow!("--frontmatter is not valid JSON/YAML: {error}"))?;
    match value {
        serde_yaml::Value::Mapping(map) => Ok(map),
        other => anyhow::bail!(
            "--frontmatter must be an object, got {}",
            match other {
                serde_yaml::Value::Sequence(_) => "an array",
                serde_yaml::Value::Null => "null",
                _ => "a scalar",
            }
        ),
    }
}

fn warn_if_lean() {
    if !LOCAL_WHISPER_COMPILED {
        tracing::warn!(
            "this build has no local transcription engine (compile with \
             --features local-whisper); producing empty transcripts",
        );
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
