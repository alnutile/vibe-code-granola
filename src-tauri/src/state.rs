//! Process-wide state, built once at startup and shared by every command.

use crate::audio::{AudioBuffers, Recorder};
use crate::db::Db;
use crate::error::{AppError, Result};
use crate::llm::LlmClient;
use crate::settings::{Paths, SecretStore, Settings};
use crate::stt::SttClient;
use parking_lot::{Mutex, RwLock};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// The meeting currently being recorded. At most one at a time — recording two
/// meetings at once would mean capturing the same audio twice.
pub struct ActiveMeeting {
    pub meeting_id: String,
    pub recorder: Option<Recorder>,
    pub buffers: Arc<AudioBuffers>,
    /// Flipped on stop; the transcription and suggestion loops watch it.
    pub stop: Arc<AtomicBool>,
}

pub struct AppState {
    pub db: Db,
    pub paths: Paths,
    /// `RwLock` because settings are read on every model call and written only
    /// from the Settings screen.
    pub settings: RwLock<Settings>,
    pub secrets: SecretStore,
    pub active: Mutex<Option<ActiveMeeting>>,
}

impl AppState {
    pub fn new() -> Result<Self> {
        let paths = Paths::resolve()?;
        let settings = Settings::load(&paths.config);
        let db = Db::open(&paths.db)?;

        tracing::info!(root = ?paths.root, "app data directory ready");

        Ok(Self {
            db,
            paths,
            settings: RwLock::new(settings),
            secrets: SecretStore::new(),
            active: Mutex::new(None),
        })
    }

    pub fn settings_snapshot(&self) -> Settings {
        self.settings.read().clone()
    }

    /// Build a chat client from current settings. Constructed per request rather
    /// than cached, so changing provider or model in Settings takes effect on the
    /// very next call with no restart and no invalidation logic.
    pub fn llm(&self) -> Result<LlmClient> {
        LlmClient::from_settings(&self.settings_snapshot(), &self.secrets)
    }

    pub fn stt(&self) -> Result<SttClient> {
        SttClient::from_settings(&self.settings_snapshot(), &self.secrets)
    }

    pub fn is_recording(&self) -> bool {
        self.active.lock().is_some()
    }

    pub fn active_meeting_id(&self) -> Option<String> {
        self.active.lock().as_ref().map(|a| a.meeting_id.clone())
    }

    pub fn require_meeting(&self, id: &str) -> Result<crate::db::Meeting> {
        self.db
            .get_meeting(id)?
            .ok_or_else(|| AppError::NotFound(format!("meeting {id}")))
    }
}
