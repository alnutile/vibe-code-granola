//! Speech-to-text over HTTP, hosted or local.
//!
//! Two request shapes cover the field:
//!
//! * **OpenAI-compatible** — `POST {base}/audio/transcriptions`. This is OpenAI
//!   itself, plus speaches, faster-whisper-server, and most others. Note that
//!   "serves an OpenAI-compatible API" does not imply this route exists: LM
//!   Studio and Ollama expose chat and embeddings only.
//! * **whisper.cpp** — `POST {base}/inference`. The odd one out: whisper.cpp's
//!   bundled server predates the OpenAI convention and never adopted it.
//!
//! Both return `{"text": "..."}`, so everything downstream is identical. That's
//! what makes "run Whisper on your laptop, then compare it against OpenAI" a
//! settings change rather than a rewrite.

use crate::error::{AppError, Result};
use crate::settings::{SecretStore, Settings};
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Deserialize)]
struct TranscriptionResponse {
    text: String,
}

pub struct SttClient {
    http: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
    model: String,
    language: Option<String>,
    provider: String,
}

impl SttClient {
    pub fn from_settings(settings: &Settings, secrets: &SecretStore) -> Result<Self> {
        let provider = settings.stt.provider.clone();

        // Unlike chat, a missing STT key is not fatal at construction: local
        // servers are usually unauthenticated, and requiring a key up front
        // would block the most interesting configuration this app supports.
        let api_key = match settings.stt_secret_key() {
            Some(key) => secrets.get(key)?,
            None => None,
        };

        if provider == "openai" && api_key.is_none() {
            return Err(AppError::Config(
                "OpenAI transcription needs an API key. Add one in Settings → Models, \
                 or switch to a local transcription server."
                    .into(),
            ));
        }

        Ok(Self {
            http: reqwest::Client::builder()
                // Local CPU Whisper on a long chunk is genuinely slow; don't cut
                // it off just because a hosted API would have answered by now.
                .timeout(Duration::from_secs(240))
                .build()?,
            base_url: crate::llm::normalize_base_url(&settings.stt.base_url),
            api_key,
            model: settings.stt.model.clone(),
            language: settings
                .stt
                .language
                .clone()
                .filter(|l| !l.trim().is_empty()),
            provider,
        })
    }

    fn is_whisper_cpp(&self) -> bool {
        self.provider == "whisper_cpp"
    }

    /// Transcribe one WAV chunk. Returns the text with surrounding whitespace
    /// trimmed; an empty string means the chunk held no speech.
    pub async fn transcribe_wav(&self, wav_bytes: Vec<u8>, filename: &str) -> Result<String> {
        let part = reqwest::multipart::Part::bytes(wav_bytes)
            .file_name(filename.to_string())
            .mime_str("audio/wav")
            .map_err(|e| AppError::Audio(format!("could not build upload: {e}")))?;

        let mut form = reqwest::multipart::Form::new()
            .part("file", part)
            .text("response_format", "json");

        let path = if self.is_whisper_cpp() {
            // whisper.cpp's server picks its model at launch, so `model` is
            // meaningless here — sending it is harmless but noisy.
            "/inference"
        } else {
            form = form.text("model", self.model.clone());
            "/audio/transcriptions"
        };

        if let Some(lang) = &self.language {
            form = form.text("language", lang.clone());
        }

        let mut req = self
            .http
            .post(format!("{}{}", self.base_url, path))
            .multipart(form);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }

        let res = req.send().await?;

        if !res.status().is_success() {
            let status = res.status().as_u16();
            let body = res.text().await.unwrap_or_default();
            return Err(AppError::Config(explain_failure(
                &self.provider,
                &self.base_url,
                path,
                status,
                &body,
            )));
        }

        // Most servers honour `response_format=json`, but a few return bare text.
        // Fall back rather than failing on a response we can plainly read.
        let raw = res.text().await?;
        let text = match serde_json::from_str::<TranscriptionResponse>(&raw) {
            Ok(parsed) => parsed.text,
            Err(_) => raw,
        };

        Ok(clean_transcript(&text))
    }

    /// Verify the transcription endpoint is reachable and configured, by sending
    /// a short synthetic tone. Cheap, and exercises the real request path
    /// including auth and multipart handling.
    pub async fn test_connection(&self) -> Result<String> {
        let silence = crate::audio::wav::encode_wav(&vec![0.0f32; 16_000], 16_000)?;
        self.transcribe_wav(silence, "test.wav").await?;
        Ok(format!("{} · {} reachable", self.provider, self.model))
    }
}

/// Turn a failed transcription request into something the user can act on.
///
/// The raw status is nearly useless here. The common failure is pointing this at
/// a server that does not transcribe at all — LM Studio and Ollama serve chat and
/// embeddings only, and answer a multipart upload with "unsupported media type"
/// or a 404, which reads like a bug in this app rather than a misconfiguration.
fn explain_failure(provider: &str, base_url: &str, path: &str, status: u16, body: &str) -> String {
    let url = format!("{base_url}{path}");
    let looks_like_wrong_server = status == 415
        || status == 404
        || status == 405
        || body.contains("unsupported_media_type")
        || body.contains("Unexpected endpoint");

    if looks_like_wrong_server {
        return format!(
            "{url} did not accept an audio upload (HTTP {status}).\n\n\
             That endpoint probably isn't a transcription server. LM Studio and Ollama \
             serve chat only — they have no /audio/transcriptions route, so no speech \
             model loaded in them can be used here.\n\n\
             For local transcription use whisper.cpp's server (choose the \"whisper.cpp\" \
             provider), or an OpenAI-compatible one such as speaches. Check too that the \
             endpoint is the API root — e.g. http://127.0.0.1:8000/v1, not a path ending \
             in /chat.\n\n\
             The server said: {}",
            truncate_body(body)
        );
    }

    format!(
        "{provider} transcription failed at {url} (HTTP {status}): {}",
        truncate_body(body)
    )
}

fn truncate_body(body: &str) -> String {
    let b = body.trim();
    if b.chars().count() <= 300 {
        return b.to_string();
    }
    format!("{}…", b.chars().take(300).collect::<String>())
}

/// Whisper-family models emit a small set of stock hallucinations on silence —
/// subtitle-credit boilerplate baked in from their training data. Dropping a
/// chunk whose entire content is one of these keeps the transcript honest.
const SILENCE_ARTIFACTS: &[&str] = &[
    "you",
    "thank you.",
    "thanks for watching!",
    "thank you for watching.",
    "thank you for watching!",
    "subscribe to my channel",
    "please subscribe",
    "bye.",
    ".",
    "[blank_audio]",
    "[silence]",
    "[music]",
    "(upbeat music)",
];

fn clean_transcript(text: &str) -> String {
    let trimmed = text.trim();
    let normalized = trimmed.to_lowercase();
    if SILENCE_ARTIFACTS.contains(&normalized.as_str()) {
        return String::new();
    }
    trimmed.to_string()
}

/// Prefilled endpoint for a provider when switching in Settings.
pub fn default_base_url(provider: &str) -> &'static str {
    match provider {
        "openai" => "https://api.openai.com/v1",
        // speaches / faster-whisper-server default port
        "openai_compatible" => "http://127.0.0.1:8000/v1",
        // whisper.cpp `server` default port
        "whisper_cpp" => "http://127.0.0.1:8080",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn a_chat_only_server_is_named_as_the_likely_cause() {
        let msg = explain_failure(
            "openai_compatible",
            "http://localhost:1234/v1",
            "/audio/transcriptions",
            415,
            r#"{"error":{"code":"unsupported_media_type"}}"#,
        );
        assert!(msg.contains("isn't a transcription server"), "{msg}");
        assert!(msg.contains("LM Studio"), "the actual culprit should be named");
        assert!(msg.contains("whisper.cpp"), "and a working alternative offered");
    }

    #[test]
    fn a_genuine_provider_error_is_reported_plainly() {
        let msg = explain_failure("openai", "https://api.openai.com/v1", "/audio/transcriptions", 500, "boom");
        assert!(msg.contains("HTTP 500"));
        assert!(!msg.contains("isn't a transcription server"));
    }

    #[test]
    fn stock_silence_hallucinations_are_dropped() {
        assert_eq!(clean_transcript("Thanks for watching!"), "");
        assert_eq!(clean_transcript("  [BLANK_AUDIO] "), "");
        assert_eq!(clean_transcript("you"), "");
    }

    #[test]
    fn real_speech_survives_cleaning() {
        assert_eq!(
            clean_transcript("  Thank you for walking me through the budget.  "),
            "Thank you for walking me through the budget."
        );
    }
}
