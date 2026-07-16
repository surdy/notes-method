//! YouTube no-caption audio fallback: download the audio-only adaptive stream
//! and hand decoded PCM to the transcription worker (ADR 0023 §6).
//!
//! This implements [`notesmith_transcribe::AudioAcquirer`] so the worker (which
//! lives in `notesmith-transcribe` and cannot depend on this crate) can acquire
//! YouTube audio without any YouTube/InnerTube knowledge of its own. The daemon
//! never runs this — only the colocated CLI worker does (ADR 0023 §5).
//!
//! Acquisition steps:
//! 1. Fetch the InnerTube player response (reuses the caption path's fetch).
//! 2. Select the smallest MP4/AAC audio-only stream (decodable without an
//!    external demuxer; ADR 0023 §6 forbids `yt-dlp`/`ffmpeg` shell-outs).
//! 3. Download it under the SSRF guard and a byte cap ([`fetch_bytes`]).
//! 4. Decode to PCM, downmix to mono, and resample to 16 kHz for Whisper.
//!
//! The MP4/AAC decode requires the `symphonia` stack, compiled only behind the
//! off-by-default `youtube-audio` Cargo feature (mirroring how the real Whisper
//! engine sits behind `local-whisper`). Without it, acquisition returns a
//! degraded [`TranscribeError::Unsupported`] the worker logs and retries — never
//! a crash (ADR 0009).

use std::time::Duration;

use notesmith_transcribe::{AcquiredAudio, AudioAcquirer, AudioInput, TranscribeError};

use crate::fetch::{FetchLimits, fetch_bytes};
use crate::youtube::{fetch_youtube_player, select_audio_format, youtube_video_id};

/// Sample rate Whisper expects (16 kHz mono).
const TARGET_SAMPLE_RATE: u32 = 16_000;
/// Byte cap for a downloaded audio stream (audio-only m4a for a long video is
/// still modest; this bounds a hostile/huge stream — ADR 0023 §8).
const MAX_AUDIO_BYTES: usize = 96 * 1024 * 1024;
/// Per-request timeout for the (potentially large) audio download.
const AUDIO_FETCH_TIMEOUT: Duration = Duration::from_secs(120);

/// Whether the MP4/AAC decode stack (symphonia) is compiled into this build.
/// Gated by the off-by-default `youtube-audio` feature, mirroring
/// [`notesmith_transcribe::LOCAL_WHISPER_COMPILED`]. When `false`, the acquirer
/// short-circuits before any network fetch so a lean build never downloads
/// (potentially tens of MB of) audio it cannot decode.
const DECODE_COMPILED: bool = cfg!(feature = "youtube-audio");

/// Acquires YouTube audio for the no-caption transcription fallback.
pub struct YoutubeAudioAcquirer {
    player_limits: FetchLimits,
    audio_limits: FetchLimits,
}

impl YoutubeAudioAcquirer {
    /// Build an acquirer with default (SSRF-guarded, bounded) fetch limits.
    pub fn new() -> Self {
        let audio_limits = FetchLimits {
            timeout: AUDIO_FETCH_TIMEOUT,
            max_bytes: MAX_AUDIO_BYTES,
            ..FetchLimits::default()
        };
        Self {
            player_limits: FetchLimits::default(),
            audio_limits,
        }
    }
}

impl Default for YoutubeAudioAcquirer {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioAcquirer for YoutubeAudioAcquirer {
    fn acquire_youtube(&self, source_url: &str) -> Result<AcquiredAudio, TranscribeError> {
        let video_id = youtube_video_id(source_url).ok_or_else(|| {
            TranscribeError::Unsupported(format!("not a YouTube video URL: {source_url}"))
        })?;

        // Short-circuit lean builds before any network work: without the decode
        // stack the download is guaranteed to be discarded, and the item is
        // retried every tick, so re-downloading the audio each time is pure
        // waste (ADR 0023 §6/§8).
        if !DECODE_COMPILED {
            return Err(TranscribeError::Unsupported(
                "YouTube audio decoding is not compiled into this build \
                 (enable the `youtube-audio` feature)"
                    .to_string(),
            ));
        }

        // The acquirer's public method is sync (the worker loop is sync), but the
        // fetch stack is async. We are always invoked from a blocking context
        // (the CLI runs `worker.run()` under `spawn_blocking`), so building a
        // private current-thread runtime here is safe and self-contained.
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| TranscribeError::Io(format!("build async runtime: {e}")))?;

        let (player, audio_bytes, mime_type) = runtime.block_on(async {
            let player = fetch_youtube_player(&video_id, &self.player_limits)
                .await
                .map_err(|e| TranscribeError::Decode(format!("fetch player: {e}")))?;

            let format = select_audio_format(&player.audio_formats)
                .ok_or_else(|| {
                    TranscribeError::Unsupported(
                        "no downloadable audio-only stream (all ciphered or absent)".to_string(),
                    )
                })?
                .clone();

            let fetched = fetch_bytes(&format.url, &self.audio_limits)
                .await
                .map_err(|e| TranscribeError::Decode(format!("download audio: {e}")))?;

            Ok::<_, TranscribeError>((player, fetched.bytes, format.mime_type))
        })?;

        let (interleaved, channels, sample_rate) = decode_to_pcm(&audio_bytes, &mime_type)?;
        let mono = downmix_to_mono(&interleaved, channels);
        let samples = resample_linear(&mono, sample_rate, TARGET_SAMPLE_RATE);

        Ok(AcquiredAudio {
            audio: AudioInput::Pcm {
                samples,
                sample_rate: TARGET_SAMPLE_RATE,
            },
            title: player.title,
            channel: player.channel,
            published: player.published,
            duration: player.duration,
        })
    }
}

/// Average interleaved multi-channel samples down to mono. A zero/1-channel
/// input is returned as-is. Pure and unit-testable.
fn downmix_to_mono(interleaved: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    interleaved
        .chunks(channels)
        .map(|frame| frame.iter().copied().sum::<f32>() / frame.len() as f32)
        .collect()
}

/// Nearest-neighbour/linear resample of a mono signal from `from_hz` to `to_hz`.
/// A no-op when the rates match or the input is empty. Pure and unit-testable;
/// good enough for speech fed to Whisper (which is robust to mild artifacts).
fn resample_linear(mono: &[f32], from_hz: u32, to_hz: u32) -> Vec<f32> {
    if mono.is_empty() || from_hz == 0 || to_hz == 0 || from_hz == to_hz {
        return mono.to_vec();
    }
    let ratio = to_hz as f64 / from_hz as f64;
    let out_len = ((mono.len() as f64) * ratio).round().max(1.0) as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src = i as f64 / ratio;
        let idx = src.floor() as usize;
        let frac = (src - idx as f64) as f32;
        let a = mono.get(idx).copied().unwrap_or(0.0);
        let b = mono.get(idx + 1).copied().unwrap_or(a);
        out.push(a + (b - a) * frac);
    }
    out
}

/// Decode a downloaded audio stream to interleaved f32 PCM, returning
/// `(samples, channels, sample_rate)`.
///
/// Requires the `youtube-audio` feature (symphonia). Without it, returns a
/// degraded [`TranscribeError::Unsupported`] so the worker skips + retries the
/// item rather than crashing (ADR 0009 / ADR 0023 §8).
#[cfg(feature = "youtube-audio")]
fn decode_to_pcm(bytes: &[u8], mime_type: &str) -> Result<(Vec<f32>, usize, u32), TranscribeError> {
    decode::decode_mp4_aac(bytes, mime_type)
}

#[cfg(not(feature = "youtube-audio"))]
fn decode_to_pcm(
    _bytes: &[u8],
    _mime_type: &str,
) -> Result<(Vec<f32>, usize, u32), TranscribeError> {
    Err(TranscribeError::Unsupported(
        "YouTube audio decoding is not compiled into this build \
         (enable the `youtube-audio` feature)"
            .to_string(),
    ))
}

#[cfg(feature = "youtube-audio")]
mod decode {
    use notesmith_transcribe::TranscribeError;
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    /// Decode an MP4/AAC byte stream to interleaved f32 PCM using symphonia.
    /// Defensive per ADR 0009: no `unwrap`/`expect` on decoded data; any error
    /// is a degraded [`TranscribeError::Decode`].
    pub(super) fn decode_mp4_aac(
        bytes: &[u8],
        mime_type: &str,
    ) -> Result<(Vec<f32>, usize, u32), TranscribeError> {
        let source = std::io::Cursor::new(bytes.to_vec());
        let mss = MediaSourceStream::new(Box::new(source), Default::default());

        let mut hint = Hint::new();
        if mime_type.contains("mp4") {
            hint.with_extension("m4a");
        }

        let probed = symphonia::default::get_probe()
            .format(
                &hint,
                mss,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .map_err(|e| TranscribeError::Decode(format!("probe audio: {e}")))?;
        let mut format = probed.format;

        let track = format
            .default_track()
            .ok_or_else(|| TranscribeError::Decode("audio has no default track".to_string()))?;
        let track_id = track.id;
        let sample_rate = track
            .codec_params
            .sample_rate
            .ok_or_else(|| TranscribeError::Decode("audio track has no sample rate".to_string()))?;

        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .map_err(|e| TranscribeError::Decode(format!("make decoder: {e}")))?;

        let mut samples: Vec<f32> = Vec::new();
        let mut channels = 0usize;

        loop {
            let packet = match format.next_packet() {
                Ok(packet) => packet,
                // Clean end of stream.
                Err(symphonia::core::errors::Error::IoError(_)) => break,
                Err(e) => return Err(TranscribeError::Decode(format!("read packet: {e}"))),
            };
            if packet.track_id() != track_id {
                continue;
            }
            match decoder.decode(&packet) {
                Ok(decoded) => {
                    let spec = *decoded.spec();
                    channels = spec.channels.count();
                    let mut buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
                    buf.copy_interleaved_ref(decoded);
                    samples.extend_from_slice(buf.samples());
                }
                // Decoder recoverable errors: skip the packet.
                Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
                Err(e) => return Err(TranscribeError::Decode(format!("decode packet: {e}"))),
            }
        }

        if samples.is_empty() {
            return Err(TranscribeError::Decode(
                "decoded audio produced no samples".to_string(),
            ));
        }
        Ok((samples, channels.max(1), sample_rate))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downmix_averages_stereo_to_mono() {
        // Two stereo frames: (1.0, -1.0) -> 0.0, (0.5, 0.5) -> 0.5.
        let interleaved = vec![1.0, -1.0, 0.5, 0.5];
        assert_eq!(downmix_to_mono(&interleaved, 2), vec![0.0, 0.5]);
    }

    #[test]
    fn downmix_passes_mono_through() {
        let mono = vec![0.1, 0.2, 0.3];
        assert_eq!(downmix_to_mono(&mono, 1), mono);
        assert_eq!(downmix_to_mono(&mono, 0), mono);
    }

    #[test]
    fn resample_is_noop_when_rates_match() {
        let s = vec![0.0, 0.5, 1.0];
        assert_eq!(resample_linear(&s, 16_000, 16_000), s);
        assert!(resample_linear(&[], 44_100, 16_000).is_empty());
    }

    #[test]
    fn resample_downsamples_length_by_ratio() {
        // 8 samples at 32 kHz -> ~4 samples at 16 kHz.
        let s: Vec<f32> = (0..8).map(|i| i as f32).collect();
        let out = resample_linear(&s, 32_000, 16_000);
        assert_eq!(out.len(), 4);
        // First sample preserved; values are monotonically increasing.
        assert_eq!(out[0], 0.0);
        assert!(out.windows(2).all(|w| w[1] >= w[0]));
    }

    #[test]
    fn resample_upsamples_length_by_ratio() {
        let s = vec![0.0, 1.0];
        let out = resample_linear(&s, 8_000, 16_000);
        assert_eq!(out.len(), 4);
        assert_eq!(out[0], 0.0);
    }

    #[test]
    fn acquire_rejects_non_youtube_url() {
        let acquirer = YoutubeAudioAcquirer::new();
        let err = acquirer.acquire_youtube("https://example.com/watch?v=x");
        assert!(matches!(err, Err(TranscribeError::Unsupported(_))));
    }

    #[cfg(not(feature = "youtube-audio"))]
    #[test]
    fn decode_is_unsupported_without_feature() {
        let err = decode_to_pcm(b"not audio", "audio/mp4");
        assert!(matches!(err, Err(TranscribeError::Unsupported(_))));
    }

    #[cfg(not(feature = "youtube-audio"))]
    #[test]
    fn acquire_short_circuits_valid_url_without_decode_feature() {
        // A lean build must reject a well-formed YouTube URL up front with
        // `Unsupported` (never attempting the network download it cannot use).
        let acquirer = YoutubeAudioAcquirer::new();
        let err = acquirer.acquire_youtube("https://www.youtube.com/watch?v=dQw4w9WgXcQ");
        assert!(matches!(err, Err(TranscribeError::Unsupported(_))));
    }
}
