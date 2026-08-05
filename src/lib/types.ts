// Mirrors the Rust types in `src-tauri/src/db/models.rs` and `settings/mod.rs`.
// The Rust side serializes with `rename_all = "camelCase"`, so these line up
// field for field. Keep them in step — there is no codegen here on purpose.

export interface Folder {
  id: string;
  name: string;
  parentId: string | null;
  createdAt: string;
}

export interface Template {
  id: string;
  name: string;
  prompt: string;
  isBuiltin: boolean;
  createdAt: string;
}

export type MeetingStatus = "idle" | "recording" | "processing" | "done" | "error";

export interface Meeting {
  id: string;
  folderId: string | null;
  templateId: string | null;
  title: string;
  prompt: string;
  status: MeetingStatus;
  audioDir: string | null;
  startedAt: string | null;
  endedAt: string | null;
  createdAt: string;
  updatedAt: string;
}

/** A meeting plus the two values the sidebar cards show, derived server-side. */
export interface MeetingListItem extends Meeting {
  /** First line of the generated summary; null until a write-up exists. */
  snippet: string | null;
  /** Recorded length; null while recording or if never started. */
  durationSecs: number | null;
}

// -------------------------------------------------------------- written notes

export interface ActionItem {
  text: string;
  /** Null when the transcript didn't make the owner clear. */
  who: string | null;
  done: boolean;
}

export interface NoteSection {
  heading: string;
  /** Markdown. */
  body: string;
}

/** The generated write-up. Stored as JSON so the UI can render real structure. */
export interface RenderedNotes {
  summary: string;
  points: string[];
  actions: ActionItem[];
  sections: NoteSection[];
}

export interface MeetingNotes {
  rendered: RenderedNotes | null;
  user: string;
}

/** An image attached to a meeting's notes. The bytes live on disk; the UI loads
 *  them on demand via `noteImageData`. `reference` is the `amble://image/<id>`
 *  token that gets embedded in the note text and that MCP resolves. */
export interface NoteImage {
  id: string;
  meetingId: string;
  name: string;
  mime: string;
  sizeBytes: number;
  reference: string;
  createdAt: string;
}

export interface ModelStatus {
  label: string;
  local: boolean;
  needsSetup: boolean;
}

/** `mic` is you, `system` is everyone else, `mixed` is both on one track. */
export type SegmentSource = "mic" | "system" | "mixed";

export interface Segment {
  id: string;
  meetingId: string;
  source: SegmentSource;
  text: string;
  startMs: number;
  endMs: number;
  createdAt: string;
}

export interface Note {
  id: string;
  meetingId: string;
  /** `user` — what you typed. `ai` — the generated write-up. */
  kind: "user" | "ai";
  content: string;
  createdAt: string;
  updatedAt: string;
}

export interface Suggestion {
  id: string;
  meetingId: string;
  content: string;
  atMs: number;
  createdAt: string;
}

export interface ChatMessage {
  id: string;
  meetingId: string;
  role: "user" | "assistant";
  content: string;
  createdAt: string;
}

export interface SearchHit {
  meetingId: string;
  meetingTitle: string;
  segmentId: string;
  text: string;
  startMs: number;
  source: SegmentSource;
}

// ------------------------------------------------------------------ settings

export type LlmProvider = "openrouter" | "openai" | "ollama" | "lmstudio" | "custom";
export type SttProvider = "openai" | "openai_compatible" | "whisper_cpp";

export interface LlmSettings {
  provider: LlmProvider;
  baseUrl: string;
  model: string;
  temperature: number;
  maxTokens: number | null;
}

export interface SttSettings {
  provider: SttProvider;
  baseUrl: string;
  model: string;
  language: string | null;
}

export interface CaptureSettings {
  captureSystemAudio: boolean;
  captureMicrophone: boolean;
  transcribeSeparately: boolean;
  chunkMinSecs: number;
  chunkMaxSecs: number;
  silenceRms: number;
  keepAudio: boolean;
  suggestionsEnabled: boolean;
  suggestionIntervalSecs: number;
  autoGenerateNotes: boolean;
  autoTitle: boolean;
}

export type McpTransport = "stdio" | "SSE" | "HTTP";

export interface McpConnection {
  id: string;
  name: string;
  enabled: boolean;
  transport: McpTransport;
  /** A URL for SSE/HTTP, or the command line for stdio. */
  target: string;
}

export interface McpSettings {
  serverEnabled: boolean;
  serverPort: number;
  serverToken: string;
  /** Deny-list, so tools added later are exposed unless turned off. */
  disabledTools: string[];
  connections: McpConnection[];
}

export interface ToolToggle {
  name: string;
  description: string;
  enabled: boolean;
}

export interface ClaudeAccess {
  host: string;
  port: number;
  token: string;
  /** For claude_desktop_config.json — goes via the mcp-remote stdio bridge. */
  desktopConfig: string;
  /** For Claude Code, which speaks HTTP with headers natively. */
  cliCommand: string;
  /** Accepting connections right now. */
  listening: boolean;
  /** Switched on by the user — may be true while `listening` is false if the
   *  port was already taken. */
  enabled: boolean;
  tools: ToolToggle[];
}

// ------------------------------------------------------------------- skills

export type SkillKind = "live" | "post";

export interface Skill {
  id: string;
  name: string;
  kind: SkillKind;
  /** Where a post-skill's output is meant to go. Empty for none. */
  target: string;
  prompt: string;
  enabled: boolean;
  isBuiltin: boolean;
  createdAt: string;
}

export interface SkillRun {
  id: string;
  meetingId: string;
  skillId: string;
  skillName: string;
  target: string;
  status: "running" | "done" | "error";
  message: string;
  output: string;
  createdAt: string;
}

export interface Settings {
  llm: LlmSettings;
  stt: SttSettings;
  capture: CaptureSettings;
  mcp: McpSettings;
}

export interface SettingsView {
  settings: Settings;
  /** Which API keys exist in the Keychain. Values are never sent to the UI. */
  secretsSet: string[];
  dataDir: string;
}

export interface PermissionStatus {
  screenRecording: boolean;
  microphonePrompted: boolean;
  detail: string;
}

export interface ModelInfo {
  id: string;
  name: string;
  description: string | null;
  contextLength: number | null;
}

export interface RecordingState {
  recording: boolean;
  meetingId: string | null;
}

export interface ToolDef {
  name: string;
  description: string;
  inputSchema: unknown;
}

export interface McpClientStatus {
  name: string;
  enabled: boolean;
  transport: string;
  connected: boolean;
}

export interface McpStatus {
  serverEnabled: boolean;
  serverRunning: boolean;
  serverPort: number;
  exposedTools: ToolDef[];
  clients: McpClientStatus[];
  note: string;
}

export const SECRET_LABELS: Record<string, string> = {
  openrouter_api_key: "OpenRouter API key",
  openai_api_key: "OpenAI API key",
  custom_llm_api_key: "Custom provider API key",
  stt_api_key: "Transcription API key",
};
