//! Local whisper.cpp transcription engine (ADR 0023 §2), behind the
//! `local-whisper` feature.
//!
//! Wraps `whisper-rs` (whisper.cpp bindings). Audio is decoded to the 16 kHz
//! mono `f32` PCM whisper.cpp requires; WAV is decoded via `hound`. Full
//! container/codec demuxing beyond WAV is out of scope for this crate (ADR 0023
//! §6) — the acquisition worker supplies a decodable input.

use std::path::Path;

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::model::whisper_model_file;
use crate::{AudioInput, TranscribeError, Transcriber, Transcript, TranscriptSegment};

/// whisper.cpp's required input sample rate.
const WHISPER_SAMPLE_RATE: u32 = 16_000;

/// A local whisper.cpp engine holding a loaded model context.
pub struct LocalWhisper {
    ctx: WhisperContext,
}

impl LocalWhisper {
    /// Load the first `ggml-*.bin` model found in `dir`.
    pub fn from_model_dir(dir: &Path) -> Result<Self, TranscribeError> {
        let model = whisper_model_file(dir).ok_or_else(|| {
            TranscribeError::ModelUnavailable(format!("no ggml-*.bin model in {}", dir.display()))
        })?;
        Self::from_model_file(&model)
    }

    /// Load a specific whisper.cpp GGML model file.
    pub fn from_model_file(path: &Path) -> Result<Self, TranscribeError> {
        let path_str = path.to_str().ok_or_else(|| {
            TranscribeError::ModelUnavailable(format!("non-UTF-8 model path {}", path.display()))
        })?;
        let ctx = WhisperContext::new_with_params(path_str, WhisperContextParameters::default())
            .map_err(|e| TranscribeError::ModelUnavailable(e.to_string()))?;
        Ok(Self { ctx })
    }

    /// Decode an [`AudioInput`] into 16 kHz mono `f32` PCM.
    fn to_pcm_16k_mono(audio: &AudioInput) -> Result<Vec<f32>, TranscribeError> {
        let (samples, sample_rate) = match audio {
            AudioInput::Pcm {
                samples,
                sample_rate,
            } => (samples.clone(), *sample_rate),
            AudioInput::Path(path) => decode_wav(path)?,
        };
        if samples.is_empty() {
            return Err(TranscribeError::Unsupported("empty audio".into()));
        }
        if sample_rate == 0 {
            return Err(TranscribeError::Unsupported("zero sample rate".into()));
        }
        Ok(resample_linear(&samples, sample_rate, WHISPER_SAMPLE_RATE))
    }
}

impl Transcriber for LocalWhisper {
    fn transcribe(&self, audio: &AudioInput) -> Result<Transcript, TranscribeError> {
        let pcm = Self::to_pcm_16k_mono(audio)?;

        let mut state = self
            .ctx
            .create_state()
            .map_err(|e| TranscribeError::Backend(e.to_string()))?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        state
            .full(params, &pcm)
            .map_err(|e| TranscribeError::Backend(e.to_string()))?;

        let mut segments = Vec::new();
        for seg in state.as_iter() {
            let text = seg
                .to_str_lossy()
                .map(|c| c.trim().to_string())
                .unwrap_or_default();
            if text.is_empty() {
                continue;
            }
            // whisper.cpp timestamps are in centiseconds (10 ms units).
            let start = seg.start_timestamp() as f64 / 100.0;
            let end = seg.end_timestamp() as f64 / 100.0;
            segments.push(TranscriptSegment { start, end, text });
        }

        Ok(Transcript {
            language: None,
            segments,
        })
    }
}

/// Decode a WAV file into interleaved `f32` samples plus its sample rate,
/// downmixed to mono. Non-fatal on malformed data — returns a typed error.
fn decode_wav(path: &Path) -> Result<(Vec<f32>, u32), TranscribeError> {
    let reader =
        hound::WavReader::open(path).map_err(|e| TranscribeError::Decode(e.to_string()))?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;

    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .into_samples::<f32>()
            .filter_map(Result::ok)
            .collect(),
        hound::SampleFormat::Int => {
            let max = ((1i64 << (spec.bits_per_sample.saturating_sub(1))) as f32).max(1.0);
            reader
                .into_samples::<i32>()
                .filter_map(Result::ok)
                .map(|s| (s as f32 / max).clamp(-1.0, 1.0))
                .collect()
        }
    };

    Ok((downmix_to_mono(&interleaved, channels), spec.sample_rate))
}

/// Average interleaved `channels`-channel samples down to mono.
fn downmix_to_mono(interleaved: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    interleaved
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
        .collect()
}

/// Linear-resample mono `samples` from `from_hz` to `to_hz`.
fn resample_linear(samples: &[f32], from_hz: u32, to_hz: u32) -> Vec<f32> {
    if from_hz == to_hz || samples.len() < 2 {
        return samples.to_vec();
    }
    let ratio = to_hz as f64 / from_hz as f64;
    let out_len = ((samples.len() as f64) * ratio).round() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src = i as f64 / ratio;
        let idx = src.floor() as usize;
        let frac = (src - idx as f64) as f32;
        let a = samples[idx.min(samples.len() - 1)];
        let b = samples[(idx + 1).min(samples.len() - 1)];
        out.push(a + (b - a) * frac);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downmix_averages_stereo() {
        let stereo = [0.0, 1.0, 0.5, -0.5];
        assert_eq!(downmix_to_mono(&stereo, 2), vec![0.5, 0.0]);
    }

    #[test]
    fn downmix_mono_is_identity() {
        let mono = [0.1, 0.2, 0.3];
        assert_eq!(downmix_to_mono(&mono, 1), mono.to_vec());
    }

    #[test]
    fn resample_identity_when_rates_match() {
        let s = [0.0, 0.5, 1.0];
        assert_eq!(resample_linear(&s, 16_000, 16_000), s.to_vec());
    }

    #[test]
    fn resample_downsamples_length() {
        let s: Vec<f32> = (0..32_000).map(|i| (i as f32).sin()).collect();
        let out = resample_linear(&s, 32_000, 16_000);
        assert_eq!(out.len(), 16_000);
    }

    #[test]
    fn decode_wav_on_garbage_is_err_not_panic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("garbage.wav");
        std::fs::write(&path, b"not a wav file at all").unwrap();
        assert!(decode_wav(&path).is_err());
    }

    #[test]
    fn to_pcm_rejects_empty_and_zero_rate() {
        let empty = AudioInput::Pcm {
            samples: vec![],
            sample_rate: 16_000,
        };
        assert!(matches!(
            LocalWhisper::to_pcm_16k_mono(&empty),
            Err(TranscribeError::Unsupported(_))
        ));

        let zero_rate = AudioInput::Pcm {
            samples: vec![0.0; 8],
            sample_rate: 0,
        };
        assert!(matches!(
            LocalWhisper::to_pcm_16k_mono(&zero_rate),
            Err(TranscribeError::Unsupported(_))
        ));
    }

    #[test]
    fn from_model_file_on_missing_file_is_err_not_panic() {
        let err = LocalWhisper::from_model_file(std::path::Path::new("/no/such/model.bin"));
        assert!(err.is_err());
    }
}
