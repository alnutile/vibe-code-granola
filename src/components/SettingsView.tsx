import { useEffect, useState } from "react";
import * as api from "../lib/api";
import {
  SECRET_LABELS,
  type LlmProvider,
  type McpStatus,
  type ModelInfo,
  type PermissionStatus,
  type Settings,
  type SttProvider,
} from "../lib/types";

interface Props {
  onClose: () => void;
}

export default function SettingsView({ onClose }: Props) {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [secretsSet, setSecretsSet] = useState<string[]>([]);
  const [dataDir, setDataDir] = useState("");
  const [defaults, setDefaults] = useState<{
    llm: Record<string, string>;
    stt: Record<string, string>;
  } | null>(null);
  const [permissions, setPermissions] = useState<PermissionStatus | null>(null);
  const [mcp, setMcp] = useState<McpStatus | null>(null);
  const [models, setModels] = useState<ModelInfo[] | null>(null);
  const [secretDrafts, setSecretDrafts] = useState<Record<string, string>>({});
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  const reload = async () => {
    try {
      const [view, d, p, m] = await Promise.all([
        api.getSettings(),
        api.providerDefaults(),
        api.getPermissions(),
        api.mcpStatus(),
      ]);
      setSettings(view.settings);
      setSecretsSet(view.secretsSet);
      setDataDir(view.dataDir);
      setDefaults(d);
      setPermissions(p);
      setMcp(m);
    } catch (e) {
      setError(api.errorMessage(e));
    }
  };

  useEffect(() => {
    void reload();
  }, []);

  const save = async (next: Settings) => {
    setSettings(next);
    setSaving(true);
    try {
      await api.saveSettings(next);
      setError(null);
    } catch (e) {
      setError(api.errorMessage(e));
    } finally {
      setSaving(false);
    }
  };

  const run = async (label: string, fn: () => Promise<string>) => {
    setMessage(`${label}…`);
    setError(null);
    try {
      setMessage(await fn());
    } catch (e) {
      setMessage(null);
      setError(api.errorMessage(e));
    }
  };

  const saveSecret = async (key: string) => {
    const value = secretDrafts[key] ?? "";
    try {
      await api.setSecret(key, value);
      setSecretDrafts((prev) => ({ ...prev, [key]: "" }));
      setMessage(value.trim() ? `${SECRET_LABELS[key]} saved to your Keychain.` : `${SECRET_LABELS[key]} cleared.`);
      await reload();
    } catch (e) {
      setError(api.errorMessage(e));
    }
  };

  if (!settings || !defaults) {
    return <div className="pane-loading">{error ?? "Loading settings…"}</div>;
  }

  const { llm, stt, capture } = settings;

  return (
    <div className="settings">
      <header className="settings-header">
        <h1>Settings</h1>
        <div>
          {saving && <span className="muted small">Saving…</span>}
          <button className="btn" onClick={onClose}>
            Done
          </button>
        </div>
      </header>

      {message && (
        <div className="banner banner-ok">
          <span>{message}</span>
          <button onClick={() => setMessage(null)}>Dismiss</button>
        </div>
      )}
      {error && (
        <div className="banner banner-error">
          <span>{error}</span>
          <button onClick={() => setError(null)}>Dismiss</button>
        </div>
      )}

      {/* ------------------------------------------------------ permissions */}
      <section className="card">
        <h2>Recording permission</h2>
        <p className="muted small">
          macOS treats "record what your speakers are playing" as a screen-recording capability, so
          this app needs Screen Recording permission even though it never captures the picture.
        </p>
        <div className="perm-row">
          <span className={`badge ${permissions?.screenRecording ? "ok" : "warn"}`}>
            {permissions?.screenRecording ? "Granted" : "Not granted"}
          </span>
          <span className="muted small">{permissions?.detail}</span>
        </div>
        <div className="row">
          <button className="btn btn-small" onClick={() => void api.openPrivacySettings("screen")}>
            Open Screen Recording settings
          </button>
          <button
            className="btn btn-small"
            onClick={() => void api.openPrivacySettings("microphone")}
          >
            Open Microphone settings
          </button>
          <button className="btn btn-small" onClick={() => void api.getPermissions().then(setPermissions)}>
            Re-check
          </button>
        </div>
      </section>

      {/* -------------------------------------------------------- chat model */}
      <section className="card">
        <h2>Chat, notes & suggestions</h2>
        <p className="muted small">
          Any endpoint that speaks the OpenAI chat API. Point it at OpenRouter for hosted models, or
          at Ollama / LM Studio to keep everything on this machine.
        </p>

        <div className="grid">
          <label>
            Provider
            <select
              value={llm.provider}
              onChange={(e) => {
                const provider = e.target.value as LlmProvider;
                void save({
                  ...settings,
                  llm: { ...llm, provider, baseUrl: defaults.llm[provider] ?? llm.baseUrl },
                });
                setModels(null);
              }}
            >
              <option value="openrouter">OpenRouter</option>
              <option value="openai">OpenAI</option>
              <option value="ollama">Ollama (local)</option>
              <option value="lmstudio">LM Studio (local)</option>
              <option value="custom">Custom</option>
            </select>
          </label>

          <label>
            Base URL
            <input
              value={llm.baseUrl}
              onChange={(e) => setSettings({ ...settings, llm: { ...llm, baseUrl: e.target.value } })}
              onBlur={() => void save(settings)}
            />
          </label>

          <label>
            Model
            <input
              list="llm-models"
              value={llm.model}
              placeholder="e.g. anthropic/claude-sonnet-4.5"
              onChange={(e) => setSettings({ ...settings, llm: { ...llm, model: e.target.value } })}
              onBlur={() => void save(settings)}
            />
            <datalist id="llm-models">
              {(models ?? []).map((m) => (
                <option key={m.id} value={m.id}>
                  {m.name}
                </option>
              ))}
            </datalist>
          </label>

          <label>
            Temperature
            <input
              type="number"
              min={0}
              max={2}
              step={0.1}
              value={llm.temperature}
              onChange={(e) =>
                setSettings({ ...settings, llm: { ...llm, temperature: Number(e.target.value) } })
              }
              onBlur={() => void save(settings)}
            />
          </label>
        </div>

        <div className="row">
          <button
            className="btn btn-small"
            onClick={() => void run("Testing", () => api.testLlm())}
          >
            Test connection
          </button>
          <button
            className="btn btn-small"
            onClick={() =>
              void api
                .listModels()
                .then((m) => {
                  setModels(m);
                  setMessage(`Loaded ${m.length} models — the Model box now autocompletes.`);
                })
                .catch((e) => setError(api.errorMessage(e)))
            }
          >
            Load model list
          </button>
        </div>
      </section>

      {/* ------------------------------------------------------ transcription */}
      <section className="card">
        <h2>Transcription</h2>
        <p className="muted small">
          Run Whisper locally and compare it against a hosted model without changing anything else —
          the rest of the app can't tell the difference.
        </p>

        <div className="grid">
          <label>
            Provider
            <select
              value={stt.provider}
              onChange={(e) => {
                const provider = e.target.value as SttProvider;
                void save({
                  ...settings,
                  stt: { ...stt, provider, baseUrl: defaults.stt[provider] ?? stt.baseUrl },
                });
              }}
            >
              <option value="openai">OpenAI</option>
              <option value="openai_compatible">Local, OpenAI-compatible (speaches, LM Studio…)</option>
              <option value="whisper_cpp">whisper.cpp server</option>
            </select>
          </label>

          <label>
            Base URL
            <input
              value={stt.baseUrl}
              onChange={(e) => setSettings({ ...settings, stt: { ...stt, baseUrl: e.target.value } })}
              onBlur={() => void save(settings)}
            />
          </label>

          <label>
            Model
            <input
              value={stt.model}
              placeholder="e.g. gpt-4o-mini-transcribe, or Systran/faster-whisper-small"
              onChange={(e) => setSettings({ ...settings, stt: { ...stt, model: e.target.value } })}
              onBlur={() => void save(settings)}
            />
          </label>

          <label>
            Language
            <input
              value={stt.language ?? ""}
              placeholder="en — blank to auto-detect"
              onChange={(e) =>
                setSettings({ ...settings, stt: { ...stt, language: e.target.value || null } })
              }
              onBlur={() => void save(settings)}
            />
          </label>
        </div>

        <div className="row">
          <button className="btn btn-small" onClick={() => void run("Testing", () => api.testStt())}>
            Test connection
          </button>
        </div>
      </section>

      {/* ------------------------------------------------------------ secrets */}
      <section className="card">
        <h2>API keys</h2>
        <p className="muted small">
          Stored in your macOS Keychain, never in this repo and never in a config file. Once saved, a
          key can be replaced or cleared but not read back out through this screen.
        </p>

        {Object.entries(SECRET_LABELS).map(([key, label]) => (
          <div className="secret-row" key={key}>
            <label>
              {label}
              <span className={`badge ${secretsSet.includes(key) ? "ok" : ""}`}>
                {secretsSet.includes(key) ? "Configured" : "Not set"}
              </span>
            </label>
            <div className="row">
              <input
                type="password"
                placeholder={secretsSet.includes(key) ? "Enter a new key to replace it" : "Paste your key"}
                value={secretDrafts[key] ?? ""}
                onChange={(e) => setSecretDrafts((prev) => ({ ...prev, [key]: e.target.value }))}
              />
              <button className="btn btn-small" onClick={() => void saveSecret(key)}>
                Save
              </button>
              {secretsSet.includes(key) && (
                <button
                  className="btn btn-small danger"
                  onClick={() =>
                    void api
                      .clearSecret(key)
                      .then(reload)
                      .then(() => setMessage(`${label} removed from your Keychain.`))
                  }
                >
                  Remove
                </button>
              )}
            </div>
          </div>
        ))}
      </section>

      {/* ---------------------------------------------------------- recording */}
      <section className="card">
        <h2>Recording</h2>

        <Toggle
          label="Capture system audio"
          hint="Everything you hear: the call, a browser tab, a video."
          checked={capture.captureSystemAudio}
          onChange={(v) =>
            void save({ ...settings, capture: { ...capture, captureSystemAudio: v } })
          }
        />
        <Toggle
          label="Capture microphone"
          hint="Your own voice."
          checked={capture.captureMicrophone}
          onChange={(v) => void save({ ...settings, capture: { ...capture, captureMicrophone: v } })}
        />
        <Toggle
          label="Label speakers"
          hint="Transcribe your mic and system audio separately so lines are attributed to You vs Them. Costs one extra transcription call per chunk."
          checked={capture.transcribeSeparately}
          onChange={(v) =>
            void save({ ...settings, capture: { ...capture, transcribeSeparately: v } })
          }
        />
        <Toggle
          label="Live suggestions"
          hint="Ask the chat model for ideas while the meeting runs."
          checked={capture.suggestionsEnabled}
          onChange={(v) =>
            void save({ ...settings, capture: { ...capture, suggestionsEnabled: v } })
          }
        />
        <Toggle
          label="Write up automatically when I stop"
          checked={capture.autoGenerateNotes}
          onChange={(v) => void save({ ...settings, capture: { ...capture, autoGenerateNotes: v } })}
        />
        <Toggle
          label="Name untitled meetings automatically"
          checked={capture.autoTitle}
          onChange={(v) => void save({ ...settings, capture: { ...capture, autoTitle: v } })}
        />
        <Toggle
          label="Keep audio files"
          hint="Off by default — meeting audio adds up fast, and the transcript is what you reread."
          checked={capture.keepAudio}
          onChange={(v) => void save({ ...settings, capture: { ...capture, keepAudio: v } })}
        />

        <div className="grid">
          <label>
            Minimum chunk (seconds)
            <input
              type="number"
              min={5}
              max={60}
              value={capture.chunkMinSecs}
              onChange={(e) =>
                setSettings({
                  ...settings,
                  capture: { ...capture, chunkMinSecs: Number(e.target.value) },
                })
              }
              onBlur={() => void save(settings)}
            />
          </label>
          <label>
            Maximum chunk (seconds)
            <input
              type="number"
              min={10}
              max={120}
              value={capture.chunkMaxSecs}
              onChange={(e) =>
                setSettings({
                  ...settings,
                  capture: { ...capture, chunkMaxSecs: Number(e.target.value) },
                })
              }
              onBlur={() => void save(settings)}
            />
          </label>
          <label>
            Suggestion interval (seconds)
            <input
              type="number"
              min={30}
              max={600}
              value={capture.suggestionIntervalSecs}
              onChange={(e) =>
                setSettings({
                  ...settings,
                  capture: { ...capture, suggestionIntervalSecs: Number(e.target.value) },
                })
              }
              onBlur={() => void save(settings)}
            />
          </label>
          <label>
            Silence threshold (RMS)
            <input
              type="number"
              min={0}
              max={0.2}
              step={0.005}
              value={capture.silenceRms}
              onChange={(e) =>
                setSettings({
                  ...settings,
                  capture: { ...capture, silenceRms: Number(e.target.value) },
                })
              }
              onBlur={() => void save(settings)}
            />
          </label>
        </div>
        <p className="muted small">
          Chunks are cut at a natural pause once they pass the minimum, or forced at the maximum.
          Longer chunks give the transcription model more context; shorter ones keep the live
          transcript closer to the conversation.
        </p>
      </section>

      {/* ---------------------------------------------------------------- MCP */}
      <section className="card">
        <h2>MCP</h2>
        <p className="muted small">{mcp?.note}</p>

        <h3 className="sub">Tools this app exposes</h3>
        <ul className="tool-list">
          {(mcp?.exposedTools ?? []).map((t) => (
            <li key={t.name}>
              <code>{t.name}</code>
              <span className="muted small"> — {t.description}</span>
            </li>
          ))}
        </ul>

        <div className="row">
          <button
            className="btn btn-small"
            onClick={() =>
              void api
                .mcpToolCall("list_meetings", { limit: 5 })
                .then((r) => setMessage(JSON.stringify(r, null, 2)))
                .catch((e) => setError(api.errorMessage(e)))
            }
          >
            Try list_meetings
          </button>
        </div>
      </section>

      <section className="card">
        <h2>Data</h2>
        <p className="muted small">
          Meetings, transcripts and notes live in a SQLite database here. Nothing is synced anywhere.
        </p>
        <code className="path">{dataDir}</code>
      </section>
    </div>
  );
}

function Toggle({
  label,
  hint,
  checked,
  onChange,
}: {
  label: string;
  hint?: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <label className="toggle">
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      <span>
        {label}
        {hint && <span className="muted small block">{hint}</span>}
      </span>
    </label>
  );
}
