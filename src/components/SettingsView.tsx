import { useEffect, useState } from "react";
import * as api from "../lib/api";
import {
  SECRET_LABELS,
  type ClaudeAccess,
  type LlmProvider,
  type McpTransport,
  type ModelInfo,
  type PermissionStatus,
  type Settings,
  type SttProvider,
} from "../lib/types";

interface Props {
  /** Settings changed — the rail's status pill needs to catch up. */
  onSaved: () => void;
  onError: (msg: string) => void;
}

type Tab = "models" | "recording" | "connections" | "claude";

/**
 * Provider metadata for the picker cards.
 *
 * Only providers this app can actually talk to are listed. The design mocked up
 * a few others (Deepgram's streaming socket, Anthropic's native API); those need
 * their own client, and offering them here would just fail at request time.
 */
const VOICE: {
  id: SttProvider;
  name: string;
  where: string;
  badge: string;
  key: boolean;
  recommended?: boolean;
}[] = [
  {
    id: "whisper_cpp",
    name: "whisper.cpp",
    where: "Local · fast and accurate on Apple Silicon",
    badge: "On device",
    key: false,
    recommended: true,
  },
  {
    id: "openai_compatible",
    name: "Local, OpenAI-compatible",
    where: "speaches, faster-whisper-server",
    badge: "On device",
    key: false,
  },
  {
    id: "openai",
    name: "OpenAI",
    where: "Cloud · uploads your audio",
    badge: "Sends audio",
    key: true,
  },
];

/// Shown in-app when whisper.cpp is selected — the fastest path from "chosen" to
/// "transcribing", without leaving the window to go and find it.
const WHISPER_SETUP = `brew install whisper-cpp

# ~550MB. Good speed/accuracy balance for meetings on Apple Silicon.
curl -L -o ~/ggml-large-v3-turbo-q5_0.bin \\
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin

whisper-server -m ~/ggml-large-v3-turbo-q5_0.bin --port 8080`;

const BRAIN: { id: LlmProvider; name: string; where: string; badge: string; key: boolean }[] = [
  { id: "ollama", name: "Ollama", where: "Local · 127.0.0.1:11434", badge: "On device", key: false },
  {
    id: "lmstudio",
    name: "LM Studio",
    where: "Local · 127.0.0.1:1234",
    badge: "On device",
    key: false,
  },
  {
    id: "openrouter",
    name: "OpenRouter",
    where: "Cloud · any hosted model",
    badge: "Sends text",
    key: true,
  },
  { id: "openai", name: "OpenAI", where: "Cloud", badge: "Sends text", key: true },
  {
    id: "custom",
    name: "Custom",
    where: "Any OpenAI-compatible endpoint",
    badge: "Depends",
    key: true,
  },
];

export default function SettingsView({ onSaved, onError }: Props) {
  const [tab, setTab] = useState<Tab>("models");
  const [settings, setSettings] = useState<Settings | null>(null);
  const [secretsSet, setSecretsSet] = useState<string[]>([]);
  const [dataDir, setDataDir] = useState("");
  const [defaults, setDefaults] = useState<{
    llm: Record<string, string>;
    stt: Record<string, string>;
  } | null>(null);
  const [permissions, setPermissions] = useState<PermissionStatus | null>(null);
  const [claude, setClaude] = useState<ClaudeAccess | null>(null);
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [newConn, setNewConn] = useState<{ name: string; transport: McpTransport; target: string }>({
    name: "",
    transport: "HTTP",
    target: "",
  });
  const [message, setMessage] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [saving, setSaving] = useState(false);

  const reload = async () => {
    try {
      const [view, d, p, c] = await Promise.all([
        api.getSettings(),
        api.providerDefaults(),
        api.getPermissions(),
        api.claudeAccess(),
      ]);
      setSettings(view.settings);
      setSecretsSet(view.secretsSet);
      setDataDir(view.dataDir);
      setDefaults(d);
      setPermissions(p);
      setClaude(c);
    } catch (e) {
      onError(api.errorMessage(e));
    }
  };

  useEffect(() => {
    void reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const save = async (next: Settings) => {
    setSettings(next);
    setSaving(true);
    try {
      await api.saveSettings(next);
      onSaved();
    } catch (e) {
      onError(api.errorMessage(e));
    } finally {
      setSaving(false);
    }
  };

  const saveSecret = async (key: string) => {
    const value = drafts[key] ?? "";
    try {
      await api.setSecret(key, value);
      setDrafts((p) => ({ ...p, [key]: "" }));
      setMessage(value.trim() ? `${SECRET_LABELS[key]} saved to your Keychain.` : `${SECRET_LABELS[key]} cleared.`);
      await reload();
      onSaved();
    } catch (e) {
      onError(api.errorMessage(e));
    }
  };

  if (!settings || !defaults) {
    return (
      <section className="screen">
        <p className="muted">Loading settings…</p>
      </section>
    );
  }

  const { llm, stt, capture, mcp } = settings;
  const voiceMeta = VOICE.find((v) => v.id === stt.provider);
  const brainMeta = BRAIN.find((b) => b.id === llm.provider);

  return (
    <section className="screen">
      <div className="screen-inner">
        <div className="screen-head">
          <h2 className="grow">Settings</h2>
          {saving && <span className="small muted">Saving…</span>}
        </div>

        <div className="seg" style={{ alignSelf: "flex-start" }}>
          {(
            [
              ["models", "Models"],
              ["recording", "Recording"],
              ["connections", "Connections"],
              ["claude", "Claude access"],
            ] as [Tab, string][]
          ).map(([k, label]) => (
            <label className="seg-opt" key={k}>
              <input
                type="radio"
                name="amble-settings"
                checked={tab === k}
                onChange={() => setTab(k)}
              />
              {label}
            </label>
          ))}
        </div>

        {message && (
          <div className="banner banner-ok" style={{ margin: 0 }}>
            <pre>{message}</pre>
            <button className="btn btn-ghost" onClick={() => setMessage(null)}>
              Dismiss
            </button>
          </div>
        )}

        {/* ═══════════════════════════════════════════════════════ models */}
        {tab === "models" && (
          <>
            <div className="card" style={{ gap: "var(--space-4)" }}>
              <div className="card-kicker">Voice to text</div>
              <p className="screen-lede" style={{ margin: 0 }}>
                Transcribes every recording. Local engines never leave the machine; cloud engines
                upload your audio.
              </p>

              <div className="picker">
                {VOICE.map((p) => (
                  <label className="pick" key={p.id}>
                    <input
                      type="radio"
                      name="amble-voice"
                      checked={stt.provider === p.id}
                      onChange={() =>
                        void save({
                          ...settings,
                          stt: { ...stt, provider: p.id, baseUrl: defaults.stt[p.id] ?? stt.baseUrl },
                        })
                      }
                    />
                    <span className="dot" />
                    <span className="pick-body">
                      <span className="pick-name">
                        {p.name}
                        {p.recommended && (
                          <span className="tag tag-accent-2" style={{ marginLeft: 8 }}>
                            Recommended
                          </span>
                        )}
                      </span>
                      <span className="small muted">{p.where}</span>
                    </span>
                    <span className={`tag ${p.key ? "tag-accent" : "tag-accent-2"}`}>{p.badge}</span>
                  </label>
                ))}
              </div>

              {stt.provider === "whisper_cpp" && (
                <div className="field">
                  <label>Setting up whisper.cpp</label>
                  <pre className="code-block">{WHISPER_SETUP}</pre>
                  <div className="row">
                    <button
                      className="btn btn-secondary btn-small"
                      onClick={() => {
                        void navigator.clipboard.writeText(WHISPER_SETUP);
                        setMessage("Setup commands copied — paste them into a terminal.");
                      }}
                    >
                      Copy commands
                    </button>
                    <span className="small muted">
                      Leave <code>whisper-server</code> running while you record. Everything stays
                      on this Mac — no key, no upload, no per-minute cost.
                    </span>
                  </div>
                </div>
              )}

              <div className="grid">
                <div className="field">
                  <label>Model</label>
                  <input
                    className="input"
                    value={stt.model}
                    placeholder={
                      stt.provider === "whisper_cpp"
                        ? "ignored — whisper-server picks its model at launch"
                        : "gpt-4o-mini-transcribe"
                    }
                    onChange={(e) => setSettings({ ...settings, stt: { ...stt, model: e.target.value } })}
                    onBlur={() => void save(settings)}
                  />
                </div>
                <div className="field">
                  <label>Endpoint</label>
                  <input
                    className="input"
                    value={stt.baseUrl}
                    onChange={(e) => setSettings({ ...settings, stt: { ...stt, baseUrl: e.target.value } })}
                    onBlur={() => void save(settings)}
                  />
                </div>
                <div className="field">
                  <label>Language</label>
                  <input
                    className="input"
                    value={stt.language ?? ""}
                    placeholder="en — blank to auto-detect"
                    onChange={(e) =>
                      setSettings({ ...settings, stt: { ...stt, language: e.target.value || null } })
                    }
                    onBlur={() => void save(settings)}
                  />
                </div>
              </div>

              {voiceMeta?.key && (
                <SecretField
                  keyName={stt.provider === "openai" ? "openai_api_key" : "stt_api_key"}
                  secretsSet={secretsSet}
                  drafts={drafts}
                  setDrafts={setDrafts}
                  onSave={saveSecret}
                />
              )}

              <label className="check">
                <input
                  type="checkbox"
                  checked={capture.keepAudio}
                  onChange={(e) =>
                    void save({ ...settings, capture: { ...capture, keepAudio: e.target.checked } })
                  }
                />
                <span>Keep the audio file on this Mac after transcription</span>
              </label>

              <div className="row">
                <button
                  className="btn btn-secondary"
                  onClick={() =>
                    void api
                      .testStt()
                      .then(setMessage)
                      .catch((e) => onError(api.errorMessage(e)))
                  }
                >
                  Test connection
                </button>
              </div>
            </div>

            <div className="card" style={{ gap: "var(--space-4)" }}>
              <div className="card-kicker">Chat &amp; transforms</div>
              <p className="screen-lede" style={{ margin: 0 }}>
                Renders notes, answers questions in the side panel, and runs your skills.
              </p>

              <div className="picker">
                {BRAIN.map((p) => (
                  <label className="pick" key={p.id}>
                    <input
                      type="radio"
                      name="amble-brain"
                      checked={llm.provider === p.id}
                      onChange={() => {
                        setModels([]);
                        void save({
                          ...settings,
                          llm: { ...llm, provider: p.id, baseUrl: defaults.llm[p.id] ?? llm.baseUrl },
                        });
                      }}
                    />
                    <span className="dot" />
                    <span className="pick-body">
                      <span className="pick-name">{p.name}</span>
                      <span className="small muted">{p.where}</span>
                    </span>
                    <span className={`tag ${p.key ? "tag-accent" : "tag-accent-2"}`}>{p.badge}</span>
                  </label>
                ))}
              </div>

              <div className="grid">
                <div className="field">
                  <label>Model</label>
                  <input
                    className="input"
                    list="llm-models"
                    value={llm.model}
                    placeholder="anthropic/claude-sonnet-4.5"
                    onChange={(e) => setSettings({ ...settings, llm: { ...llm, model: e.target.value } })}
                    onBlur={() => void save(settings)}
                  />
                  <datalist id="llm-models">
                    {models.map((m) => (
                      <option key={m.id} value={m.id}>
                        {m.name}
                      </option>
                    ))}
                  </datalist>
                </div>
                <div className="field">
                  <label>Endpoint</label>
                  <input
                    className="input"
                    value={llm.baseUrl}
                    onChange={(e) => setSettings({ ...settings, llm: { ...llm, baseUrl: e.target.value } })}
                    onBlur={() => void save(settings)}
                  />
                </div>
                <div className="field">
                  <label>Temperature</label>
                  <input
                    className="input"
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
                </div>
              </div>

              {brainMeta?.key && (
                <SecretField
                  keyName={
                    llm.provider === "openrouter"
                      ? "openrouter_api_key"
                      : llm.provider === "openai"
                        ? "openai_api_key"
                        : "custom_llm_api_key"
                  }
                  secretsSet={secretsSet}
                  drafts={drafts}
                  setDrafts={setDrafts}
                  onSave={saveSecret}
                />
              )}

              <div className="row">
                <button
                  className="btn btn-secondary"
                  onClick={() =>
                    void api
                      .testLlm()
                      .then(setMessage)
                      .catch((e) => onError(api.errorMessage(e)))
                  }
                >
                  Test connection
                </button>
                <button
                  className="btn btn-ghost"
                  onClick={() =>
                    void api
                      .listModels()
                      .then((m) => {
                        setModels(m);
                        setMessage(`Loaded ${m.length} models — the Model box now autocompletes.`);
                      })
                      .catch((e) => onError(api.errorMessage(e)))
                  }
                >
                  Load model list
                </button>
              </div>
            </div>
          </>
        )}

        {/* ════════════════════════════════════════════════════ recording */}
        {tab === "recording" && (
          <>
            <div className="card" style={{ gap: "var(--space-3)" }}>
              <div className="card-kicker">Permission</div>
              <p className="screen-lede" style={{ margin: 0 }}>
                macOS treats “record what your speakers are playing” as a screen-recording
                capability, so Amble needs it even though it never captures the picture.
              </p>
              <div className="row">
                <span className={`tag ${permissions?.screenRecording ? "tag-accent-2" : "tag-accent"}`}>
                  {permissions?.screenRecording ? "Granted" : "Not granted"}
                </span>
                <span className="small muted grow">{permissions?.detail}</span>
              </div>
              <div className="row">
                <button className="btn btn-secondary" onClick={() => void api.openPrivacySettings("screen")}>
                  Screen Recording
                </button>
                <button
                  className="btn btn-secondary"
                  onClick={() => void api.openPrivacySettings("microphone")}
                >
                  Microphone
                </button>
                <button
                  className="btn btn-ghost"
                  onClick={() => void api.getPermissions().then(setPermissions)}
                >
                  Re-check
                </button>
              </div>
            </div>

            <div className="card" style={{ gap: "var(--space-2)" }}>
              <div className="card-kicker">Capture</div>
              <Toggle
                label="Capture system audio"
                hint="Everything you hear: the call, a browser tab, a video."
                checked={capture.captureSystemAudio}
                onChange={(v) => void save({ ...settings, capture: { ...capture, captureSystemAudio: v } })}
              />
              <Toggle
                label="Capture microphone"
                hint="Your own voice."
                checked={capture.captureMicrophone}
                onChange={(v) => void save({ ...settings, capture: { ...capture, captureMicrophone: v } })}
              />
              <Toggle
                label="Label speakers"
                hint="Transcribe mic and system audio separately so lines are attributed to You vs Them. One extra transcription call per chunk."
                checked={capture.transcribeSeparately}
                onChange={(v) => void save({ ...settings, capture: { ...capture, transcribeSeparately: v } })}
              />
              <Toggle
                label="Live suggestions"
                hint="Ask the chat model for ideas while the meeting runs, steered by your live skills."
                checked={capture.suggestionsEnabled}
                onChange={(v) => void save({ ...settings, capture: { ...capture, suggestionsEnabled: v } })}
              />
              <Toggle
                label="Render automatically when I stop"
                checked={capture.autoGenerateNotes}
                onChange={(v) => void save({ ...settings, capture: { ...capture, autoGenerateNotes: v } })}
              />
              <Toggle
                label="Name untitled meetings automatically"
                checked={capture.autoTitle}
                onChange={(v) => void save({ ...settings, capture: { ...capture, autoTitle: v } })}
              />

              <div className="grid" style={{ marginTop: "var(--space-2)" }}>
                <NumField
                  label="Minimum chunk (seconds)"
                  value={capture.chunkMinSecs}
                  min={5}
                  max={60}
                  onChange={(v) => setSettings({ ...settings, capture: { ...capture, chunkMinSecs: v } })}
                  onCommit={() => void save(settings)}
                />
                <NumField
                  label="Maximum chunk (seconds)"
                  value={capture.chunkMaxSecs}
                  min={10}
                  max={120}
                  onChange={(v) => setSettings({ ...settings, capture: { ...capture, chunkMaxSecs: v } })}
                  onCommit={() => void save(settings)}
                />
                <NumField
                  label="Suggestion interval (seconds)"
                  value={capture.suggestionIntervalSecs}
                  min={30}
                  max={600}
                  onChange={(v) =>
                    setSettings({ ...settings, capture: { ...capture, suggestionIntervalSecs: v } })
                  }
                  onCommit={() => void save(settings)}
                />
                <NumField
                  label="Silence threshold (RMS)"
                  value={capture.silenceRms}
                  min={0}
                  max={0.2}
                  step={0.005}
                  onChange={(v) => setSettings({ ...settings, capture: { ...capture, silenceRms: v } })}
                  onCommit={() => void save(settings)}
                />
              </div>
              <p className="small muted" style={{ margin: 0 }}>
                Chunks are cut at a natural pause once past the minimum, or forced at the maximum.
              </p>
            </div>

            <div className="card" style={{ gap: "var(--space-2)" }}>
              <div className="card-kicker">Data</div>
              <p className="small muted" style={{ margin: 0 }}>
                Meetings, transcripts and notes live in a SQLite database here. Nothing syncs.
              </p>
              <code className="path">{dataDir}</code>
            </div>
          </>
        )}

        {/* ═══════════════════════════════════════════════════ connections */}
        {tab === "connections" && (
          <>
            <div className="card" style={{ gap: "var(--space-3)" }}>
              <div className="card-kicker">Connected servers</div>
              <p className="screen-lede" style={{ margin: 0 }}>
                MCP servers your after-skills would push into. Amble is the client here.
              </p>

              <div className="banner banner-error" style={{ margin: 0 }}>
                <span>
                  Saved but not dialled — the outbound MCP client has no transport yet, so skill
                  output stays on the meeting rather than being delivered.
                </span>
              </div>

              {mcp.connections.length === 0 && <p className="small muted">Nothing connected yet.</p>}

              {mcp.connections.map((c) => (
                <div className="conn-row" key={c.id}>
                  <span className="pick-body grow">
                    <span className="pick-name">{c.name}</span>
                    <span className="small muted conn-target">{c.target}</span>
                  </span>
                  <span className="tag tag-neutral">{c.transport}</span>
                  <span className="tag tag-neutral">Not connected</span>
                  <input
                    type="checkbox"
                    checked={c.enabled}
                    onChange={(e) =>
                      void api
                        .toggleConnection(c.id, e.target.checked)
                        .then(reload)
                        .catch((err) => onError(api.errorMessage(err)))
                    }
                    style={{ width: 18, height: 18, accentColor: "var(--color-accent)" }}
                  />
                  <button
                    className="btn btn-ghost"
                    onClick={() =>
                      void api
                        .removeConnection(c.id)
                        .then(reload)
                        .catch((err) => onError(api.errorMessage(err)))
                    }
                  >
                    Remove
                  </button>
                </div>
              ))}
            </div>

            <div className="card" style={{ gap: "var(--space-4)" }}>
              <div className="card-kicker">Add a connection</div>
              <div className="field">
                <label>Name</label>
                <input
                  className="input"
                  value={newConn.name}
                  placeholder="e.g. Linear"
                  onChange={(e) => setNewConn({ ...newConn, name: e.target.value })}
                />
              </div>
              <div className="field">
                <label>Transport</label>
                <div className="seg">
                  {(["stdio", "SSE", "HTTP"] as McpTransport[]).map((t) => (
                    <label className="seg-opt" key={t}>
                      <input
                        type="radio"
                        name="amble-transport"
                        checked={newConn.transport === t}
                        onChange={() => setNewConn({ ...newConn, transport: t })}
                      />
                      {t}
                    </label>
                  ))}
                </div>
              </div>
              <div className="field">
                <label>URL or command</label>
                <input
                  className="input"
                  value={newConn.target}
                  placeholder="https://… or npx -y @modelcontextprotocol/server-…"
                  onChange={(e) => setNewConn({ ...newConn, target: e.target.value })}
                />
              </div>
              <button
                className="btn btn-primary"
                style={{ alignSelf: "flex-start" }}
                onClick={() =>
                  void api
                    .addConnection(newConn.name, newConn.transport, newConn.target)
                    .then(() => setNewConn({ name: "", transport: "HTTP", target: "" }))
                    .then(reload)
                    .catch((e) => onError(api.errorMessage(e)))
                }
              >
                Add connection
              </button>
            </div>
          </>
        )}

        {/* ════════════════════════════════════════════════ claude access */}
        {tab === "claude" && claude && (
          <>
            <div className="card" style={{ gap: "var(--space-4)" }}>
              <div className="card-kicker">Local MCP endpoint</div>
              <p className="screen-lede" style={{ margin: 0 }}>
                Amble runs its own MCP server so Claude can read your notes from Claude Desktop or
                Claude Code. It binds to loopback only — nothing is exposed to your network, and
                every tool it offers is read-only.
              </p>

              <div className="conn-row" style={{ cursor: "default" }}>
                <span className="pick-body grow">
                  <span className="pick-name">Run the local MCP server</span>
                  <span className="small muted">
                    Loopback only, and every request must carry the token below.
                  </span>
                </span>
                <span
                  className={`tag ${
                    claude.listening ? "tag-accent-2" : claude.enabled ? "tag-accent" : "tag-neutral"
                  }`}
                >
                  {claude.listening
                    ? `Listening on ${claude.port}`
                    : claude.enabled
                      ? "Port unavailable"
                      : "Off"}
                </span>
                <input
                  type="checkbox"
                  checked={claude.enabled}
                  onChange={(e) =>
                    void api
                      .setMcpServerEnabled(e.target.checked)
                      .then(reload)
                      .catch((err) => {
                        onError(api.errorMessage(err));
                        void reload();
                      })
                  }
                  style={{ width: 18, height: 18, accentColor: "var(--color-accent)" }}
                />
              </div>

              {claude.enabled && !claude.listening && (
                <div className="banner banner-error" style={{ margin: 0 }}>
                  <span>
                    Switched on, but the port isn't available — something else is probably using{" "}
                    {claude.port}. Pick another below.
                  </span>
                </div>
              )}

              <div className="grid">
                <div className="field">
                  <label>Host</label>
                  <input className="input" value={claude.host} readOnly />
                </div>
                <div className="field">
                  <label>Port</label>
                  <input
                    className="input"
                    type="number"
                    value={mcp.serverPort}
                    onChange={(e) =>
                      setSettings({
                        ...settings,
                        mcp: { ...mcp, serverPort: Number(e.target.value) },
                      })
                    }
                    // Rebind after a port change so the socket and the config
                    // the user pasted into Claude agree.
                    onBlur={() =>
                      void save(settings)
                        .then(api.restartMcpServer)
                        .then(reload)
                        .catch((e) => onError(api.errorMessage(e)))
                    }
                  />
                </div>
                <div className="field">
                  <label>Token</label>
                  <input className="input" value={claude.token} readOnly />
                </div>
              </div>

              <div className="row">
                <button
                  className="btn btn-secondary"
                  onClick={() =>
                    void api
                      .regenerateToken()
                      .then(reload)
                      .then(() => setMessage("New token generated. Update your Claude config."))
                      .catch((e) => onError(api.errorMessage(e)))
                  }
                >
                  Regenerate token
                </button>
              </div>

              <div className="field">
                <label>
                  Claude Desktop —{" "}
                  <strong>claude_desktop_config.json</strong> (Settings → Developer → Edit config)
                </label>
                <pre className="code-block">{claude.desktopConfig}</pre>
                <div className="row">
                  <button
                    className="btn btn-primary btn-small"
                    onClick={() => {
                      void navigator.clipboard.writeText(claude.desktopConfig);
                      setCopied(true);
                      window.setTimeout(() => setCopied(false), 1800);
                    }}
                  >
                    {copied ? "Copied" : "Copy config"}
                  </button>
                  <span className="small muted">
                    Desktop can't call an HTTP server with custom headers directly, so this goes
                    through the <code>mcp-remote</code> bridge — which needs Node installed. The
                    token sits in <code>env</code> because Desktop mangles spaces inside an
                    argument.
                  </span>
                </div>
              </div>

              <div className="field">
                <label>Claude Code — run in a terminal</label>
                <pre className="code-block">{claude.cliCommand}</pre>
                <span className="small muted">
                  Claude Code speaks HTTP directly, so it needs no bridge.
                </span>
              </div>

              <p className="small muted" style={{ margin: 0 }}>
                Restart Claude after either one. Amble has to be running for Claude to reach it, and
                regenerating the token invalidates any config you've already pasted.
              </p>
            </div>

            <div className="card" style={{ gap: "var(--space-3)" }}>
              <div className="card-kicker">Tools Claude may use</div>
              {claude.tools.map((t) => (
                <label className="conn-row" key={t.name}>
                  <span className="pick-body grow">
                    <span className="pick-name">
                      <code>{t.name}</code>
                    </span>
                    <span className="small muted">{t.description}</span>
                  </span>
                  <input
                    type="checkbox"
                    checked={t.enabled}
                    onChange={(e) =>
                      void api
                        .setToolEnabled(t.name, e.target.checked)
                        .then(reload)
                        .catch((err) => onError(api.errorMessage(err)))
                    }
                    style={{ width: 18, height: 18, accentColor: "var(--color-accent)" }}
                  />
                </label>
              ))}
              <div className="row">
                <button
                  className="btn btn-ghost"
                  onClick={() =>
                    void api
                      .mcpToolCall("list_meetings", { limit: 5 })
                      .then((r) => setMessage(JSON.stringify(r, null, 2)))
                      .catch((e) => onError(api.errorMessage(e)))
                  }
                >
                  Try list_meetings in-process
                </button>
              </div>
            </div>
          </>
        )}
      </div>
    </section>
  );
}

function SecretField({
  keyName,
  secretsSet,
  drafts,
  setDrafts,
  onSave,
}: {
  keyName: string;
  secretsSet: string[];
  drafts: Record<string, string>;
  setDrafts: (f: (p: Record<string, string>) => Record<string, string>) => void;
  onSave: (key: string) => void;
}) {
  const isSet = secretsSet.includes(keyName);
  return (
    <div className="field">
      <label>
        API key{" "}
        <span className={`tag ${isSet ? "tag-accent-2" : "tag-neutral"}`}>
          {isSet ? "Configured" : "Not set"}
        </span>
      </label>
      <div className="row">
        <input
          className="input grow"
          type="password"
          placeholder={isSet ? "Enter a new key to replace it" : "Stored in the system keychain"}
          value={drafts[keyName] ?? ""}
          onChange={(e) => setDrafts((p) => ({ ...p, [keyName]: e.target.value }))}
        />
        <button className="btn btn-secondary" onClick={() => onSave(keyName)}>
          Save
        </button>
      </div>
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
    <label className="check">
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      <span>
        {label}
        {hint && <span className="small muted block">{hint}</span>}
      </span>
    </label>
  );
}

function NumField({
  label,
  value,
  min,
  max,
  step,
  onChange,
  onCommit,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  onChange: (v: number) => void;
  onCommit: () => void;
}) {
  return (
    <div className="field">
      <label>{label}</label>
      <input
        className="input"
        type="number"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        onBlur={onCommit}
      />
    </div>
  );
}
