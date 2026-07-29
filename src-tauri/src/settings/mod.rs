//! Settings split two ways: preferences to a JSON file, secrets to the Keychain.
//!
//! The split is the point. `config.json` is readable, diffable, and safe to share
//! when someone files a bug. API keys never touch it — they go to the macOS
//! Keychain and are only ever read back into memory at request time.

pub mod secrets;

use crate::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub use secrets::{SecretStore, SECRET_KEYS};

/// Where chat, notes, and suggestions come from. Every supported option speaks
/// the OpenAI `/chat/completions` shape, so one client covers all of them.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmSettings {
    /// `openrouter` | `openai` | `ollama` | `lmstudio` | `custom`
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub temperature: f32,
    /// Cap on generated tokens. `None` lets the provider decide.
    pub max_tokens: Option<u32>,
}

impl Default for LlmSettings {
    fn default() -> Self {
        // OpenRouter by default: one key, and you can swap the model string to
        // anything without touching this config again.
        Self {
            provider: "openrouter".into(),
            base_url: "https://openrouter.ai/api/v1".into(),
            model: "anthropic/claude-sonnet-4.5".into(),
            temperature: 0.3,
            max_tokens: None,
        }
    }
}

/// Where the transcript comes from.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SttSettings {
    /// `openai` | `openai_compatible` | `whisper_cpp`
    ///
    /// `openai_compatible` covers speaches, faster-whisper-server and friends —
    /// anything actually serving `/audio/transcriptions`. `whisper_cpp` is the
    /// odd one out: whisper.cpp's own server uses `/inference` instead.
    ///
    /// **LM Studio does not belong here.** It serves chat and embeddings only;
    /// it has no `/audio/transcriptions` route and its chat API rejects audio
    /// content parts, so no speech model loaded in it can transcribe. It is a
    /// fine choice for `LlmSettings::provider`, just not this one.
    pub provider: String,
    pub base_url: String,
    pub model: String,
    /// ISO-639-1 hint, e.g. `en`. `None` lets the model auto-detect.
    pub language: Option<String>,
}

impl Default for SttSettings {
    fn default() -> Self {
        Self {
            provider: "openai".into(),
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-4o-mini-transcribe".into(),
            language: Some("en".into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureSettings {
    /// Everything you hear: the call, the browser tab, the video.
    pub capture_system_audio: bool,
    /// Your own voice.
    pub capture_microphone: bool,
    /// Transcribe mic and system audio as separate streams so lines can be
    /// attributed to "You" vs "Them". Costs one extra STT call per chunk;
    /// turn it off to mix them into a single stream and halve that.
    pub transcribe_separately: bool,
    /// Don't cut a chunk shorter than this, even at a silence boundary.
    pub chunk_min_secs: u32,
    /// Force a cut at this length even mid-sentence, so the transcript keeps up.
    pub chunk_max_secs: u32,
    /// RMS below this counts as silence when hunting for a cut point.
    pub silence_rms: f32,
    /// Keep the per-chunk WAV files after transcription. Off by default —
    /// meeting audio adds up fast, and the transcript is what you actually reread.
    pub keep_audio: bool,
    pub suggestions_enabled: bool,
    pub suggestion_interval_secs: u64,
    /// Generate the write-up automatically when a meeting stops.
    pub auto_generate_notes: bool,
    /// Name untitled meetings from their content once the transcript exists.
    pub auto_title: bool,
}

impl Default for CaptureSettings {
    fn default() -> Self {
        Self {
            capture_system_audio: true,
            capture_microphone: true,
            transcribe_separately: true,
            chunk_min_secs: 12,
            chunk_max_secs: 30,
            silence_rms: 0.01,
            keep_audio: false,
            suggestions_enabled: true,
            suggestion_interval_secs: 90,
            auto_generate_notes: true,
            auto_title: true,
        }
    }
}

/// MCP wiring, in both directions.
///
/// *Outbound* (`connections`) are servers this app would call into, so a
/// post-skill can file an issue or append to a page. *Inbound* is this app's own
/// server, which Claude Desktop / Claude Code connect to. Neither transport is
/// implemented yet — this is the configuration for both, and it persists.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpSettings {
    /// Expose this app's meetings to Claude and other MCP hosts.
    pub server_enabled: bool,
    /// Loopback only — never bound to a routable address.
    pub server_port: u16,
    /// Bearer token a client must present. Generated on first run.
    #[serde(default)]
    pub server_token: String,
    /// Tool names withheld from MCP hosts. Absent means every tool is offered,
    /// so a tool added in a later version is exposed unless explicitly turned
    /// off — the opposite would silently hide new tools from existing configs.
    #[serde(default)]
    pub disabled_tools: Vec<String>,
    /// MCP servers this app connects out to.
    #[serde(default)]
    pub connections: Vec<McpConnection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpConnection {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    /// `stdio` | `SSE` | `HTTP`
    pub transport: String,
    /// A URL for SSE/HTTP, or the command line for stdio.
    pub target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default)]
    pub llm: LlmSettings,
    #[serde(default)]
    pub stt: SttSettings,
    #[serde(default)]
    pub capture: CaptureSettings,
    #[serde(default)]
    pub mcp: McpSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            llm: LlmSettings::default(),
            stt: SttSettings::default(),
            capture: CaptureSettings::default(),
            mcp: McpSettings {
                server_enabled: false,
                server_port: 4577,
                server_token: String::new(),
                disabled_tools: Vec::new(),
                connections: Vec::new(),
            },
        }
    }
}

impl Settings {
    /// Missing or corrupt config falls back to defaults rather than failing to
    /// launch — a bad config file should never lock you out of the app that
    /// wrote it. A corrupt file is preserved as `config.json.bak` first.
    pub fn load(path: &Path) -> Self {
        let Ok(raw) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        match serde_json::from_str(&raw) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("config.json is unreadable ({e}); starting from defaults");
                let _ = std::fs::write(path.with_extension("json.bak"), &raw);
                Self::default()
            }
        }
    }

    /// Mint an access token if there isn't one. Called after load so the Claude
    /// settings tab always has a real token to show, without the UI having to
    /// ask for one first.
    pub fn ensure_server_token(&mut self) -> bool {
        if !self.mcp.server_token.is_empty() {
            return false;
        }
        self.mcp.server_token = new_server_token();
        true
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    /// Which Keychain entry holds the key for the current chat provider.
    /// Local providers need no key at all.
    pub fn llm_secret_key(&self) -> Option<&'static str> {
        match self.llm.provider.as_str() {
            "openrouter" => Some(secrets::OPENROUTER_API_KEY),
            "openai" => Some(secrets::OPENAI_API_KEY),
            "custom" => Some(secrets::CUSTOM_LLM_API_KEY),
            _ => None, // ollama, lmstudio: local, unauthenticated
        }
    }

    /// Which Keychain entry holds the key for the current STT provider.
    pub fn stt_secret_key(&self) -> Option<&'static str> {
        match self.stt.provider.as_str() {
            "openai" => Some(secrets::OPENAI_API_KEY),
            // Local transcription servers are typically unauthenticated, but a
            // hosted OpenAI-compatible one may not be — allow a key either way.
            "openai_compatible" | "whisper_cpp" => Some(secrets::STT_API_KEY),
            _ => None,
        }
    }
}

/// A bearer token for the local MCP endpoint.
///
/// From `Uuid::new_v4`, which is already a CSPRNG draw — this only reshapes it
/// into something recognisable in a config file.
pub fn new_server_token() -> String {
    let raw = uuid::Uuid::new_v4().simple().to_string();
    format!("amb_lcl_{}", &raw[..20])
}

/// Resolved on startup and handed around as part of app state.
#[derive(Debug, Clone)]
pub struct Paths {
    pub root: PathBuf,
    pub config: PathBuf,
    pub db: PathBuf,
    pub recordings: PathBuf,
}

impl Paths {
    /// `~/Library/Application Support/com.vibecode.granola` on macOS.
    pub fn resolve() -> Result<Self> {
        let root = dirs_data_dir()?.join("com.vibecode.granola");
        std::fs::create_dir_all(&root)?;
        let recordings = root.join("recordings");
        std::fs::create_dir_all(&recordings)?;
        Ok(Self {
            config: root.join("config.json"),
            db: root.join("granola.db"),
            recordings,
            root,
        })
    }
}

fn dirs_data_dir() -> Result<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME")
            .map_err(|_| AppError::Config("HOME is not set".into()))?;
        Ok(PathBuf::from(home).join("Library/Application Support"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        // The app is macOS-only in practice (ScreenCaptureKit), but keeping this
        // compiling elsewhere means `cargo test` works on CI runners.
        std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".local/share")))
            .map_err(|_| AppError::Config("no data directory available".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unreadable_config_falls_back_to_defaults() {
        let dir = std::env::temp_dir().join(format!("vg-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(&path, "{ this is not json").unwrap();

        let s = Settings::load(&path);
        assert_eq!(s.llm.provider, "openrouter");
        assert!(
            path.with_extension("json.bak").exists(),
            "the corrupt file should be preserved, not silently dropped"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn settings_survive_a_save_load_round_trip() {
        let dir = std::env::temp_dir().join(format!("vg-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");

        let mut s = Settings::default();
        s.llm.model = "openai/gpt-5".into();
        s.capture.chunk_max_secs = 45;
        s.save(&path).unwrap();

        let loaded = Settings::load(&path);
        assert_eq!(loaded.llm.model, "openai/gpt-5");
        assert_eq!(loaded.capture.chunk_max_secs, 45);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn local_llm_providers_need_no_api_key() {
        let mut s = Settings::default();
        s.llm.provider = "ollama".into();
        assert!(s.llm_secret_key().is_none());

        s.llm.provider = "openrouter".into();
        assert_eq!(s.llm_secret_key(), Some(secrets::OPENROUTER_API_KEY));
    }
}
