//! ScreenCaptureKit capture for macOS.
//!
//! One stream carries both sources. macOS classifies "record what the speakers
//! are playing" as a *screen recording* capability, so system audio needs Screen
//! Recording permission even though we never look at a pixel — and because the
//! API is inherently a video capture, we ask for the smallest, slowest video
//! stream it will give us and throw every frame away.

use super::{AudioBuffers, PermissionStatus, Source, CAPTURE_SAMPLE_RATE};
use crate::error::{AppError, Result};
use crate::settings::CaptureSettings;
use screencapturekit::cm::CMSampleBufferExt;
use screencapturekit::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Video we are obliged to receive but do not want. 2×2 at 1 fps is the cheapest
/// configuration ScreenCaptureKit reliably accepts.
const DUMMY_WIDTH: u32 = 2;
const DUMMY_HEIGHT: u32 = 2;

/// Receives sample buffers on ScreenCaptureKit's dispatch queues.
///
/// Must be `Send + Sync`: Apple may call this concurrently from arbitrary
/// threads, and does.
struct AudioHandler {
    buffers: Arc<AudioBuffers>,
    /// Set the first time microphone audio actually arrives, which is the only
    /// reliable signal that the mic permission prompt was accepted.
    mic_seen: Arc<AtomicBool>,
}

impl SCStreamOutputTrait for AudioHandler {
    fn did_output_sample_buffer(&self, sample: CMSampleBuffer, of_type: SCStreamOutputType) {
        let source = match of_type {
            SCStreamOutputType::Audio => Source::System,
            SCStreamOutputType::Microphone => Source::Mic,
            // Video frames: we asked for the smallest stream possible and have
            // no use for them.
            SCStreamOutputType::Screen => return,
        };

        let Some(list) = sample.audio_buffer_list() else {
            return;
        };

        // ScreenCaptureKit delivers non-interleaved 32-bit float: one buffer per
        // channel. Collect the planes, then average them to mono.
        let planes: Vec<Vec<f32>> = list
            .iter()
            .map(|buf| bytes_to_f32(buf.data()))
            .filter(|p| !p.is_empty())
            .collect();

        if planes.is_empty() {
            return;
        }

        if source == Source::Mic {
            self.mic_seen.store(true, Ordering::Relaxed);
        }

        self.buffers.push(source, &super::wav::downmix(&planes));
    }
}

/// Reinterpret a byte slice as little-endian `f32` samples.
///
/// `chunks_exact` rather than `chunks`: a trailing partial sample would be
/// garbage, and silently zero-padding it introduces a click.
fn bytes_to_f32(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

pub struct MacRecorder {
    stream: SCStream,
    mic_seen: Arc<AtomicBool>,
}

impl MacRecorder {
    pub fn start(settings: &CaptureSettings, buffers: Arc<AudioBuffers>) -> Result<Self> {
        if !settings.capture_system_audio && !settings.capture_microphone {
            return Err(AppError::Audio(
                "Both audio sources are turned off. Enable the microphone or system audio \
                 in Settings → Recording."
                    .into(),
            ));
        }

        let content = SCShareableContent::get().map_err(|e| {
            AppError::Audio(format!(
                "Screen Recording permission is required to capture audio. \
                 Grant it in System Settings → Privacy & Security → Screen & System Audio Recording, \
                 then restart the app. ({e})"
            ))
        })?;

        let displays = content.displays();
        let display = displays
            .first()
            .ok_or_else(|| AppError::Audio("No display found to attach the audio stream to.".into()))?;

        let filter = SCContentFilter::create()
            .with_display(display)
            .with_excluding_windows(&[])
            .build();

        let mut config = SCStreamConfiguration::new()
            .with_width(DUMMY_WIDTH)
            .with_height(DUMMY_HEIGHT)
            .with_shows_cursor(false)
            // 1 fps. We cannot turn video off entirely, so we make it negligible.
            .with_minimum_frame_interval(&CMTime::new(1, 1))
            .with_sample_rate(CAPTURE_SAMPLE_RATE as i32)
            .with_channel_count(2)
            .with_captures_audio(settings.capture_system_audio)
            // Never record ourselves — without this, any sound the app makes
            // would loop back into the recording.
            .with_excludes_current_process_audio(true);

        if settings.capture_microphone {
            config = config.with_captures_microphone(true);
        }

        let mic_seen = Arc::new(AtomicBool::new(false));
        let mut stream = SCStream::new(&filter, &config);

        // One handler serves both output types; it routes on the tag macOS
        // attaches to each buffer.
        if settings.capture_system_audio {
            stream.add_output_handler(
                AudioHandler {
                    buffers: Arc::clone(&buffers),
                    mic_seen: Arc::clone(&mic_seen),
                },
                SCStreamOutputType::Audio,
            );
        }

        if settings.capture_microphone {
            stream.add_output_handler(
                AudioHandler {
                    buffers: Arc::clone(&buffers),
                    mic_seen: Arc::clone(&mic_seen),
                },
                SCStreamOutputType::Microphone,
            );
        }

        stream
            .start_capture()
            .map_err(|e| AppError::Audio(format!("Could not start audio capture: {e}")))?;

        tracing::info!(
            system = settings.capture_system_audio,
            mic = settings.capture_microphone,
            "capture started"
        );

        Ok(Self { stream, mic_seen })
    }

    pub fn stop(self) -> Result<()> {
        self.stream
            .stop_capture()
            .map_err(|e| AppError::Audio(format!("Could not stop audio capture: {e}")))?;
        tracing::info!(mic_delivered = self.mic_seen.load(Ordering::Relaxed), "capture stopped");
        Ok(())
    }
}

/// Probe Screen Recording permission by asking for shareable content — the call
/// fails outright when permission is missing, and there is no lighter check that
/// doesn't lie.
pub fn permission_status() -> PermissionStatus {
    match SCShareableContent::get() {
        Ok(content) if content.displays().is_empty() => PermissionStatus {
            screen_recording: false,
            microphone_prompted: false,
            detail: "No displays are available to capture from.".into(),
        },
        Ok(_) => PermissionStatus {
            screen_recording: true,
            microphone_prompted: false,
            detail: "Ready to record. macOS will ask for microphone access the first time \
                     you start a meeting."
                .into(),
        },
        Err(_) => PermissionStatus {
            screen_recording: false,
            microphone_prompted: false,
            detail: "Screen Recording permission is needed to capture system audio. \
                     Open System Settings → Privacy & Security → Screen & System Audio Recording, \
                     enable this app, then restart it."
                .into(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_convert_to_little_endian_floats() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1.0f32.to_le_bytes());
        bytes.extend_from_slice(&(-0.5f32).to_le_bytes());
        assert_eq!(bytes_to_f32(&bytes), vec![1.0, -0.5]);
    }

    #[test]
    fn a_trailing_partial_sample_is_discarded() {
        let mut bytes = 1.0f32.to_le_bytes().to_vec();
        bytes.push(0x00); // one stray byte
        assert_eq!(bytes_to_f32(&bytes), vec![1.0]);
    }
}
