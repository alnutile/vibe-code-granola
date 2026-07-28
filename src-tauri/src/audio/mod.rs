//! Capturing what you hear and what you say.
//!
//! On macOS both come from a single ScreenCaptureKit stream, which is what makes
//! this app possible without asking anyone to install a virtual audio device.
//! ScreenCaptureKit tags each sample buffer with its origin, so system audio and
//! microphone arrive already separated — and staying separated all the way to the
//! transcript is how lines get attributed to "You" vs "Them" without running a
//! speaker-diarization model.

pub mod chunker;
pub mod wav;

#[cfg(target_os = "macos")]
mod macos;

use crate::error::Result;
use crate::settings::CaptureSettings;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// ScreenCaptureKit's native rate. We ask for it explicitly rather than accepting
/// the default, so downstream resampling maths is never guessing.
pub const CAPTURE_SAMPLE_RATE: u32 = 48_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// Your microphone.
    Mic,
    /// Everything else: the call, the browser tab, the video.
    System,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Mic => "mic",
            Source::System => "system",
        }
    }
}

/// Where capture callbacks deposit audio and the transcription loop picks it up.
///
/// Callbacks arrive on ScreenCaptureKit's dispatch queues — arbitrary threads,
/// potentially concurrently — so this has to be `Sync`, and the locks are held
/// for exactly as long as an `extend_from_slice` takes. Anything slower here
/// would drop audio.
#[derive(Default)]
pub struct AudioBuffers {
    mic: Mutex<Vec<f32>>,
    system: Mutex<Vec<f32>>,
    /// Most recent RMS per source, as `f32` bits — this feeds the level meters,
    /// and a torn read of a VU meter is not worth a lock.
    mic_level: AtomicU32,
    system_level: AtomicU32,
}

impl AudioBuffers {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn push(&self, source: Source, samples: &[f32]) {
        if samples.is_empty() {
            return;
        }
        let level = wav::rms(samples);
        match source {
            Source::Mic => {
                self.mic.lock().extend_from_slice(samples);
                self.mic_level.store(level.to_bits(), Ordering::Relaxed);
            }
            Source::System => {
                self.system.lock().extend_from_slice(samples);
                self.system_level.store(level.to_bits(), Ordering::Relaxed);
            }
        }
    }

    /// Take everything buffered for a source, leaving it empty.
    pub fn drain(&self, source: Source) -> Vec<f32> {
        match source {
            Source::Mic => std::mem::take(&mut *self.mic.lock()),
            Source::System => std::mem::take(&mut *self.system.lock()),
        }
    }

    pub fn levels(&self) -> (f32, f32) {
        (
            f32::from_bits(self.mic_level.load(Ordering::Relaxed)),
            f32::from_bits(self.system_level.load(Ordering::Relaxed)),
        )
    }

    pub fn clear(&self) {
        self.mic.lock().clear();
        self.system.lock().clear();
        self.mic_level.store(0, Ordering::Relaxed);
        self.system_level.store(0, Ordering::Relaxed);
    }
}

/// What macOS will let us record right now.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionStatus {
    /// Screen Recording permission. Required for system audio — macOS treats
    /// "hear what your speakers play" as a screen-recording capability, which
    /// surprises everyone the first time.
    pub screen_recording: bool,
    /// Microphone permission is requested by the OS on first capture rather than
    /// being queryable up front, so this reports whether we have observed a
    /// successful mic capture yet.
    pub microphone_prompted: bool,
    /// Human-readable explanation for the UI when something is missing.
    pub detail: String,
}

/// A live capture session. Dropping it stops the stream.
pub struct Recorder {
    #[cfg(target_os = "macos")]
    inner: macos::MacRecorder,
    #[cfg(not(target_os = "macos"))]
    _unsupported: (),
}

impl Recorder {
    /// Begin capturing into `buffers`.
    ///
    /// Fails if the user has not granted Screen Recording permission, or if both
    /// sources are disabled in settings — there would be nothing to record.
    pub fn start(settings: &CaptureSettings, buffers: Arc<AudioBuffers>) -> Result<Self> {
        #[cfg(target_os = "macos")]
        {
            Ok(Self {
                inner: macos::MacRecorder::start(settings, buffers)?,
            })
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (settings, buffers);
            Err(crate::error::AppError::Audio(
                "audio capture is only implemented for macOS".into(),
            ))
        }
    }

    pub fn stop(self) -> Result<()> {
        #[cfg(target_os = "macos")]
        {
            self.inner.stop()
        }
        #[cfg(not(target_os = "macos"))]
        {
            Ok(())
        }
    }
}

/// Probe current permissions without starting a capture.
pub fn permission_status() -> PermissionStatus {
    #[cfg(target_os = "macos")]
    {
        macos::permission_status()
    }
    #[cfg(not(target_os = "macos"))]
    {
        PermissionStatus {
            screen_recording: false,
            microphone_prompted: false,
            detail: "Audio capture is only implemented for macOS.".into(),
        }
    }
}

/// Open the macOS privacy pane so the user can grant permission without hunting
/// through System Settings.
pub fn open_privacy_settings(pane: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let anchor = match pane {
            "microphone" => "Privacy_Microphone",
            _ => "Privacy_ScreenCapture",
        };
        std::process::Command::new("open")
            .arg(format!(
                "x-apple.systempreferences:com.apple.preference.security?{anchor}"
            ))
            .spawn()?;
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = pane;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffers_keep_sources_separate() {
        let b = AudioBuffers::new();
        b.push(Source::Mic, &[0.1, 0.2]);
        b.push(Source::System, &[0.9]);

        assert_eq!(b.drain(Source::Mic), vec![0.1, 0.2]);
        assert_eq!(b.drain(Source::System), vec![0.9]);
        // Draining empties.
        assert!(b.drain(Source::Mic).is_empty());
    }

    #[test]
    fn levels_track_the_most_recent_push() {
        let b = AudioBuffers::new();
        b.push(Source::Mic, &[1.0, -1.0, 1.0, -1.0]);
        let (mic, system) = b.levels();
        assert!(mic > 0.99, "expected a loud mic level, got {mic}");
        assert_eq!(system, 0.0);
    }

    #[test]
    fn clear_resets_buffers_and_levels() {
        let b = AudioBuffers::new();
        b.push(Source::Mic, &[1.0; 16]);
        b.clear();
        assert!(b.drain(Source::Mic).is_empty());
        assert_eq!(b.levels(), (0.0, 0.0));
    }
}
