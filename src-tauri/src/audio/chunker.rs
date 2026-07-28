//! Deciding where to cut the audio stream into transcribable pieces.
//!
//! Naive fixed-length chunking slices words in half, and Whisper cannot recover
//! the missing halves — you get "…walk me through the bud" / "get for next
//! quarter". So instead: once a chunk is long enough to be worth sending, wait
//! for a natural pause and cut there. If no pause arrives, cut anyway at the max
//! length so the live transcript never falls far behind the conversation.

use super::wav::rms;

/// Silence is evaluated over 20 ms frames — long enough to be stable, short
/// enough to land the cut inside an actual gap between words.
const FRAME_MS: usize = 20;

#[derive(Debug, Clone)]
pub struct Chunk {
    pub samples: Vec<f32>,
    pub start_ms: i64,
    pub end_ms: i64,
}

pub struct Chunker {
    sample_rate: u32,
    min_samples: usize,
    max_samples: usize,
    silence_rms: f32,
    buf: Vec<f32>,
    /// Samples already emitted, which is what makes chunk timestamps line up
    /// with wall-clock position in the meeting.
    emitted: u64,
}

impl Chunker {
    pub fn new(sample_rate: u32, min_secs: u32, max_secs: u32, silence_rms: f32) -> Self {
        let min_samples = (sample_rate as usize) * min_secs.max(1) as usize;
        let max_samples = (sample_rate as usize) * max_secs.max(min_secs.max(1) + 1) as usize;
        Self {
            sample_rate,
            min_samples,
            max_samples,
            silence_rms,
            buf: Vec::with_capacity(max_samples),
            emitted: 0,
        }
    }

    pub fn push(&mut self, samples: &[f32]) {
        self.buf.extend_from_slice(samples);
    }

    pub fn buffered_secs(&self) -> f32 {
        self.buf.len() as f32 / self.sample_rate as f32
    }

    /// Emit a chunk if one is ready, otherwise keep accumulating.
    pub fn take_ready(&mut self) -> Option<Chunk> {
        if self.buf.len() < self.min_samples {
            return None;
        }

        let cut = if self.buf.len() >= self.max_samples {
            // Out of patience — cut mid-sentence rather than lag the transcript.
            self.max_samples
        } else {
            self.find_silence_cut()?
        };

        Some(self.emit(cut))
    }

    /// Emit whatever is left, regardless of length. Called when recording stops.
    pub fn flush(&mut self) -> Option<Chunk> {
        if self.buf.is_empty() {
            return None;
        }
        Some(self.emit(self.buf.len()))
    }

    /// Look for a quiet frame past the minimum length, and cut at its end so the
    /// silence trails the speech rather than opening the next chunk.
    fn find_silence_cut(&self) -> Option<usize> {
        let frame = (self.sample_rate as usize * FRAME_MS) / 1000;
        if frame == 0 {
            return None;
        }

        let mut pos = self.min_samples;
        while pos + frame <= self.buf.len() {
            if rms(&self.buf[pos..pos + frame]) < self.silence_rms {
                return Some(pos + frame);
            }
            pos += frame;
        }
        None
    }

    fn emit(&mut self, cut: usize) -> Chunk {
        let cut = cut.min(self.buf.len());
        let samples: Vec<f32> = self.buf.drain(..cut).collect();

        let start_ms = self.samples_to_ms(self.emitted);
        self.emitted += samples.len() as u64;
        let end_ms = self.samples_to_ms(self.emitted);

        Chunk { samples, start_ms, end_ms }
    }

    fn samples_to_ms(&self, samples: u64) -> i64 {
        (samples * 1000 / self.sample_rate as u64) as i64
    }
}

impl Chunk {
    /// Whether this chunk is quiet enough that transcribing it would only buy
    /// hallucinated filler. Skipping these is the single biggest saving on a
    /// call where one side does most of the talking.
    pub fn is_silent(&self, threshold: f32) -> bool {
        rms(&self.samples) < threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 16_000;

    fn loud(secs: f32) -> Vec<f32> {
        // Alternating sign keeps RMS high without being a DC offset.
        (0..(RATE as f32 * secs) as usize)
            .map(|i| if i % 2 == 0 { 0.5 } else { -0.5 })
            .collect()
    }

    fn quiet(secs: f32) -> Vec<f32> {
        vec![0.0; (RATE as f32 * secs) as usize]
    }

    #[test]
    fn nothing_is_emitted_below_the_minimum_length() {
        let mut c = Chunker::new(RATE, 10, 30, 0.01);
        c.push(&loud(5.0));
        assert!(c.take_ready().is_none());
    }

    #[test]
    fn a_pause_after_the_minimum_triggers_a_cut() {
        let mut c = Chunker::new(RATE, 10, 30, 0.01);
        c.push(&loud(11.0));
        c.push(&quiet(0.5));
        c.push(&loud(2.0));

        let chunk = c.take_ready().expect("a pause past the minimum should cut");
        assert_eq!(chunk.start_ms, 0);
        // The cut lands in the pause, not at the far end of the buffer.
        assert!(chunk.end_ms >= 11_000 && chunk.end_ms <= 11_600, "cut at {}ms", chunk.end_ms);
    }

    #[test]
    fn continuous_speech_is_cut_at_the_maximum() {
        let mut c = Chunker::new(RATE, 10, 20, 0.01);
        c.push(&loud(25.0));

        let chunk = c.take_ready().expect("the max length should force a cut");
        assert_eq!(chunk.samples.len(), RATE as usize * 20);
        assert_eq!(chunk.end_ms, 20_000);
    }

    #[test]
    fn timestamps_are_contiguous_across_chunks() {
        let mut c = Chunker::new(RATE, 5, 10, 0.01);
        c.push(&loud(30.0));

        let a = c.take_ready().unwrap();
        let b = c.take_ready().unwrap();
        assert_eq!(a.end_ms, b.start_ms, "chunks must not overlap or leave gaps");
    }

    #[test]
    fn flush_emits_a_short_final_chunk() {
        let mut c = Chunker::new(RATE, 10, 30, 0.01);
        c.push(&loud(3.0));
        assert!(c.take_ready().is_none());

        let tail = c.flush().expect("flush should not care about the minimum");
        assert_eq!(tail.samples.len(), RATE as usize * 3);
        assert!(c.flush().is_none(), "flush should drain the buffer");
    }

    #[test]
    fn silent_chunks_are_detectable() {
        let mut c = Chunker::new(RATE, 1, 2, 0.01);
        c.push(&quiet(3.0));
        let chunk = c.take_ready().unwrap();
        assert!(chunk.is_silent(0.01));

        let mut c2 = Chunker::new(RATE, 1, 2, 0.01);
        c2.push(&loud(3.0));
        assert!(!c2.take_ready().unwrap().is_silent(0.01));
    }
}
