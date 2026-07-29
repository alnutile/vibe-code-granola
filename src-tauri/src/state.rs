//! Process-wide state, built once at startup and shared by every command.

use crate::audio::{AudioBuffers, Recorder};
use crate::db::Db;
use crate::error::{AppError, Result};
use crate::llm::LlmClient;
use crate::settings::{Paths, SecretStore, Settings};
use crate::stt::SttClient;
use parking_lot::{Mutex, RwLock};
use std::sync::atomic::{AtomicBool, Ordering};
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
    /// The local MCP endpoint, when running. A tokio mutex because shutting the
    /// server down awaits the socket closing.
    pub mcp_server: tokio::sync::Mutex<Option<crate::mcp::server::ServerHandle>>,
    /// Mirrors whether `mcp_server` is `Some`, readable without taking the lock
    /// — the settings screen asks on every render.
    mcp_listening: AtomicBool,
}

impl AppState {
    pub fn new() -> Result<Self> {
        let paths = Paths::resolve()?;
        let mut settings = Settings::load(&paths.config);
        // Mint the MCP token on first run and persist it, so the config the
        // Claude settings tab shows stays the same across restarts.
        if settings.ensure_server_token() {
            settings.save(&paths.config)?;
        }
        let db = Db::open(&paths.db)?;

        let secrets = SecretStore::new();

        // Adopt API keys saved under the app's previous identity — on its own
        // thread, never on the startup path.
        //
        // Reading a Keychain item created by a differently-signed binary makes
        // macOS put up a *blocking* permission dialog. Doing that inline hangs
        // the app behind a prompt the user has no context for, with no window on
        // screen to explain it. Backgrounded, the window opens immediately and
        // the prompt is answerable at leisure; declining costs nothing but
        // re-pasting a key.
        {
            let secrets = secrets.clone();
            std::thread::spawn(move || match secrets.migrate_from_legacy() {
                0 => tracing::debug!("no keys to adopt from the previous app identity"),
                n => tracing::info!(keys = n, "adopted keys from the previous app identity"),
            });
        }

        tracing::info!(root = ?paths.root, "app data directory ready");

        Ok(Self {
            db,
            paths,
            settings: RwLock::new(settings),
            secrets,
            active: Mutex::new(None),
            mcp_server: tokio::sync::Mutex::new(None),
            mcp_listening: AtomicBool::new(false),
        })
    }

    pub fn mcp_listening(&self) -> bool {
        self.mcp_listening.load(Ordering::Relaxed)
    }

    /// Start the local MCP endpoint, replacing any listener already running.
    ///
    /// The old one is stopped first and awaited, so restarting on a new port
    /// cannot race the previous socket still holding it.
    pub async fn start_mcp(self: &Arc<Self>, port: u16) -> Result<()> {
        self.stop_mcp().await;

        let handle = crate::mcp::server::start(Arc::clone(self), port).await?;
        *self.mcp_server.lock().await = Some(handle);
        self.mcp_listening.store(true, Ordering::Relaxed);
        Ok(())
    }

    pub async fn stop_mcp(&self) {
        let existing = self.mcp_server.lock().await.take();
        if let Some(handle) = existing {
            handle.stop().await;
            tracing::info!("MCP server stopped");
        }
        self.mcp_listening.store(false, Ordering::Relaxed);
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
