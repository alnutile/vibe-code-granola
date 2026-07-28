//! PCM helpers: downmix, resample, and WAV encoding.
//!
//! ScreenCaptureKit hands us 48 kHz stereo float planes. Speech recognition wants
//! 16 kHz mono. Doing that conversion here means every chunk we upload is a sixth
//! the size it would otherwise be, which matters a lot on a two-hour call.

use crate::error::Result;
use std::io::Cursor;

/// Target rate for transcription. Every Whisper-family model is trained at 16 kHz
/// and resamples internally anyway, so sending anything higher just wastes bytes.
pub const STT_SAMPLE_RATE: u32 = 16_000;

/// Average N channel planes down to mono.
///
/// Planes are allowed to differ in length — a sample buffer can arrive with a
/// short tail — so the result is as long as the shortest plane.
pub fn downmix(planes: &[Vec<f32>]) -> Vec<f32> {
    match planes.len() {
        0 => Vec::new(),
        1 => planes[0].clone(),
        n => {
            let len = planes.iter().map(Vec::len).min().unwrap_or(0);
            let scale = 1.0 / n as f32;
            (0..len)
                .map(|i| planes.iter().map(|p| p[i]).sum::<f32>() * scale)
                .collect()
        }
    }
}

/// Resample by averaging fixed-size windows.
///
/// This is a box filter, not a windowed-sinc: it has audible artifacts you'd never
/// ship in a music app. For speech headed to an ASR model it is inaudible in the
/// results, and it keeps the audio path dependency-free and trivially auditable.
/// Swap in `rubato` here if you ever want to keep the audio for listening.
pub fn resample(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate || samples.is_empty() {
        return samples.to_vec();
    }

    let ratio = from_rate as f64 / to_rate as f64;
    let out_len = (samples.len() as f64 / ratio).floor() as usize;
    let mut out = Vec::with_capacity(out_len);

    for i in 0..out_len {
        let start = (i as f64 * ratio) as usize;
        let end = (((i + 1) as f64 * ratio) as usize).min(samples.len()).max(start + 1);
        let window = &samples[start..end.min(samples.len())];
        if window.is_empty() {
            break;
        }
        out.push(window.iter().sum::<f32>() / window.len() as f32);
    }
    out
}

/// Root mean square — the loudness measure used for silence detection.
pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

/// Encode mono float samples as a 16-bit PCM WAV file in memory.
///
/// 16-bit rather than 32-bit float: universally accepted by transcription
/// endpoints, and half the upload.
pub fn encode_wav(samples: &[f32], sample_rate: u32) -> Result<Vec<u8>> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut cursor = Cursor::new(Vec::<u8>::new());
    {
        // Borrow the cursor so it survives the writer — `finalize` consumes the
        // writer, and we still need the bytes it wrote.
        let mut writer = hound::WavWriter::new(&mut cursor, spec)?;
        for &s in samples {
            // Clamp before scaling: a sum of two loud sources can exceed 1.0, and
            // an unclamped cast wraps around into a nasty click.
            let clamped = s.clamp(-1.0, 1.0);
            writer.write_sample((clamped * i16::MAX as f32) as i16)?;
        }
        writer.finalize()?;
    }
    Ok(cursor.into_inner())
}

/// Full path from captured audio to an upload-ready WAV: mono, 16 kHz, 16-bit.
pub fn prepare_for_stt(samples: &[f32], source_rate: u32) -> Result<Vec<u8>> {
    let resampled = resample(samples, source_rate, STT_SAMPLE_RATE);
    encode_wav(&resampled, STT_SAMPLE_RATE)
}

/// Sum two sources into one track, used when transcribing mic and system audio
/// as a single stream. Halving first keeps two simultaneous speakers from
/// clipping; the trailing part of the longer input is preserved.
pub fn mix(a: &[f32], b: &[f32]) -> Vec<f32> {
    let len = a.len().max(b.len());
    (0..len)
        .map(|i| {
            let x = a.get(i).copied().unwrap_or(0.0);
            let y = b.get(i).copied().unwrap_or(0.0);
            ((x + y) * 0.5).clamp(-1.0, 1.0)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downmix_averages_channels() {
        let planes = vec![vec![1.0, 0.0, 1.0], vec![0.0, 1.0, 1.0]];
        assert_eq!(downmix(&planes), vec![0.5, 0.5, 1.0]);
    }

    #[test]
    fn downmix_truncates_to_the_shortest_plane() {
        let planes = vec![vec![1.0, 1.0, 1.0], vec![0.0, 0.0]];
        assert_eq!(downmix(&planes).len(), 2);
    }

    #[test]
    fn resample_hits_the_expected_length_for_48k_to_16k() {
        let input = vec![0.5f32; 48_000];
        let out = resample(&input, 48_000, 16_000);
        assert_eq!(out.len(), 16_000);
        // A constant signal must survive averaging unchanged.
        assert!(out.iter().all(|s| (s - 0.5).abs() < 1e-6));
    }

    #[test]
    fn resample_is_a_no_op_at_matching_rates() {
        let input = vec![0.1, 0.2, 0.3];
        assert_eq!(resample(&input, 16_000, 16_000), input);
    }

    #[test]
    fn rms_of_silence_is_zero() {
        assert_eq!(rms(&vec![0.0; 100]), 0.0);
        assert!(rms(&vec![1.0; 100]) > 0.99);
    }

    #[test]
    fn wav_encoding_produces_a_riff_header_of_the_right_size() {
        let bytes = encode_wav(&vec![0.0; 16_000], 16_000).unwrap();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        // 44-byte canonical header + one second of 16-bit mono at 16 kHz.
        assert_eq!(bytes.len(), 44 + 16_000 * 2);
    }

    #[test]
    fn mixing_two_loud_sources_stays_within_range() {
        let a = vec![1.0f32; 10];
        let b = vec![1.0f32; 10];
        assert!(mix(&a, &b).iter().all(|s| *s <= 1.0 && *s >= -1.0));
    }

    #[test]
    fn mixing_keeps_the_tail_of_the_longer_source() {
        let a = vec![1.0f32; 4];
        let b = vec![1.0f32; 2];
        assert_eq!(mix(&a, &b).len(), 4);
    }
}
