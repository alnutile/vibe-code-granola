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

export interface McpClientConfig {
  name: string;
  enabled: boolean;
  transport: "stdio" | "http";
  command: string;
  args: string[];
  url: string;
}

export interface McpSettings {
  serverEnabled: boolean;
  serverPort: number;
  clients: McpClientConfig[];
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
