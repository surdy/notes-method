//! `notesmith transcribe <audio>` — transcribe a local audio file into a
//! timestamped Markdown note using the local engine (ADR 0023 §7, issue #271).
//!
//! Uses [`notesmith_transcribe::default_transcriber`]: the real whisper.cpp
//! engine when built with `--features local-whisper` and a model resolves
//! ([`NOTESMITH_WHISPER_MODEL_DIR`](notesmith_transcribe::WHISPER_MODEL_DIR_ENV)),
//! otherwise a stub that yields an empty transcript. The rendered note carries
//! ADR 0019 §3 media-provenance frontmatter. By default the note is printed to
//! stdout; `--output <FILE>` writes it to disk.

use std::path::PathBuf;

use chrono::Local;
use notesmith_transcribe::{
    AudioInput, LOCAL_WHISPER_COMPILED, MediaMeta, default_transcriber, render_transcript_note,
};

use crate::commands::vault::OutputFormat;

/// The `source_type` recorded for locally transcribed audio.
const SOURCE_TYPE_AUDIO: &str = "audio";

#[derive(Debug, clap::Args)]
pub struct TranscribeCommand {
    /// Path to the audio file to transcribe (WAV is decoded natively).
    audio: PathBuf,

    /// Write the rendered note to this file instead of printing it.
    #[arg(long)]
    output: Option<PathBuf>,

    /// Extra tags to add to the note (in addition to the mandatory `inbox`).
    #[arg(long = "tag", value_name = "TAG")]
    tags: Vec<String>,
}

impl TranscribeCommand {
    pub async fn run(&self, format: OutputFormat) -> anyhow::Result<()> {
        if !self.audio.is_file() {
            anyhow::bail!("Audio file not found: {}", self.audio.display());
        }
        if !LOCAL_WHISPER_COMPILED {
            tracing::warn!(
                "this build has no local transcription engine (compile with \
                 --features local-whisper); producing an empty transcript",
            );
        }

        let transcriber = default_transcriber();
        let transcript = transcriber
            .transcribe(&AudioInput::Path(self.audio.clone()))
            .map_err(|e| anyhow::anyhow!("transcription failed: {e}"))?;

        let title = self
            .audio
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let meta = MediaMeta {
            title,
            source: self.audio.display().to_string(),
            source_type: SOURCE_TYPE_AUDIO.to_string(),
            duration: transcript.segments.last().map(|s| s.end.max(0.0) as u64),
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
}
