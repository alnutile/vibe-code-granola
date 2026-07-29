// Typed wrappers over the Tauri command surface.
//
// One function per command, so a rename on the Rust side breaks here at compile
// time rather than at runtime in some component.

import { invoke } from "@tauri-apps/api/core";
import type {
  ChatMessage,
  ClaudeAccess,
  Folder,
  McpConnection,
  McpStatus,
  McpTransport,
  Meeting,
  MeetingListItem,
  MeetingNotes,
  ModelInfo,
  ModelStatus,
  Note,
  PermissionStatus,
  RecordingState,
  RenderedNotes,
  SearchHit,
  Segment,
  Settings,
  SettingsView,
  Skill,
  SkillKind,
  SkillRun,
  Suggestion,
  Template,
} from "./types";

// ------------------------------------------------------------------ settings

export const getSettings = () => invoke<SettingsView>("settings_get");
export const getModelStatus = () => invoke<ModelStatus>("model_status");
export const saveSettings = (settings: Settings) => invoke<void>("settings_save", { settings });
export const setSecret = (key: string, value: string) => invoke<void>("secret_set", { key, value });
export const clearSecret = (key: string) => invoke<void>("secret_clear", { key });
export const providerDefaults = () =>
  invoke<{ llm: Record<string, string>; stt: Record<string, string> }>("provider_defaults");
export const testLlm = () => invoke<string>("llm_test");
export const testStt = () => invoke<string>("stt_test");
export const listModels = () => invoke<ModelInfo[]>("llm_models");

// --------------------------------------------------------------- permissions

export const getPermissions = () => invoke<PermissionStatus>("permissions_get");
export const openPrivacySettings = (pane: "screen" | "microphone") =>
  invoke<void>("permissions_open", { pane });

// --------------------------------------------------------- folders/templates

export const listFolders = () => invoke<Folder[]>("folders_list");
export const createFolder = (name: string, parentId?: string | null) =>
  invoke<Folder>("folder_create", { name, parentId: parentId ?? null });
export const renameFolder = (id: string, name: string) => invoke<void>("folder_rename", { id, name });
export const deleteFolder = (id: string) => invoke<void>("folder_delete", { id });

export const listTemplates = () => invoke<Template[]>("templates_list");
export const createTemplate = (name: string, prompt: string) =>
  invoke<Template>("template_create", { name, prompt });
export const deleteTemplate = (id: string) => invoke<void>("template_delete", { id });

// ------------------------------------------------------------------ meetings

/** Every meeting. Folder filtering and per-folder counts are both derived from
 *  this one payload, so the counts stay right while a filter is applied. */
export const listMeetings = () => invoke<MeetingListItem[]>("meetings_list");
export const getMeeting = (id: string) => invoke<Meeting>("meeting_get", { id });
export const createMeeting = (args: {
  title?: string;
  prompt?: string;
  folderId?: string | null;
  templateId?: string | null;
}) => invoke<Meeting>("meeting_create", args);

/**
 * Partial update: omit a field to leave it alone. To clear the folder or
 * template, pass an explicit `null`.
 */
export const updateMeeting = (
  id: string,
  patch: {
    title?: string;
    prompt?: string;
    folderId?: string | null;
    templateId?: string | null;
  },
) => invoke<void>("meeting_update", { id, ...patch });

export const deleteMeeting = (id: string) => invoke<void>("meeting_delete", { id });

// ----------------------------------------------------------------- recording

export const getRecordingState = () => invoke<RecordingState>("recording_state");
export const startRecording = (id: string) => invoke<void>("meeting_start", { id });
export const stopRecording = (id: string) => invoke<void>("meeting_stop", { id });

// ---------------------------------------------------------------- transcript

export const listSegments = (meetingId: string) => invoke<Segment[]>("segments_list", { meetingId });
export const getTranscript = (meetingId: string) => invoke<string>("transcript_text", { meetingId });
export const search = (query: string, limit?: number) =>
  invoke<SearchHit[]>("search", { query, limit: limit ?? null });

// --------------------------------------------------------- notes/suggestions

export const getNotes = (meetingId: string) => invoke<MeetingNotes>("notes_get", { meetingId });
export const saveNote = (meetingId: string, content: string) =>
  invoke<Note>("note_save", { meetingId, content });
export const generateNotes = (meetingId: string) =>
  invoke<RenderedNotes>("notes_generate", { meetingId });
export const toggleAction = (meetingId: string, index: number) =>
  invoke<RenderedNotes>("note_toggle_action", { meetingId, index });

export const listSuggestions = (meetingId: string) =>
  invoke<Suggestion[]>("suggestions_list", { meetingId });
export const suggestNow = (meetingId: string) => invoke<string | null>("suggest_now", { meetingId });

// ---------------------------------------------------------------------- chat

export const chatHistory = (meetingId: string) => invoke<ChatMessage[]>("chat_history", { meetingId });
export const sendChat = (meetingId: string, message: string) =>
  invoke<string>("chat_send", { meetingId, message });
export const clearChat = (meetingId: string) => invoke<void>("chat_clear", { meetingId });

// ----------------------------------------------------------------------- MCP

export const mcpStatus = () => invoke<McpStatus>("mcp_status");
export const mcpToolCall = (name: string, args?: Record<string, unknown>) =>
  invoke<unknown>("mcp_tool_call", { name, args: args ?? {} });

export const claudeAccess = () => invoke<ClaudeAccess>("mcp_claude_access");
export const setMcpServerEnabled = (enabled: boolean) =>
  invoke<boolean>("mcp_server_set_enabled", { enabled });
export const restartMcpServer = () => invoke<boolean>("mcp_server_restart");
export const regenerateToken = () => invoke<string>("mcp_regenerate_token");
export const setToolEnabled = (name: string, enabled: boolean) =>
  invoke<void>("mcp_set_tool_enabled", { name, enabled });

export const addConnection = (name: string, transport: McpTransport, target: string) =>
  invoke<McpConnection>("mcp_add_connection", { name, transport, target });
export const removeConnection = (id: string) => invoke<void>("mcp_remove_connection", { id });
export const toggleConnection = (id: string, enabled: boolean) =>
  invoke<void>("mcp_toggle_connection", { id, enabled });

// --------------------------------------------------------------------- skills

export const listSkills = () => invoke<Skill[]>("skills_list");
export const createSkill = (args: {
  name: string;
  kind: SkillKind;
  target?: string;
  prompt: string;
}) => invoke<Skill>("skill_create", args);
export const updateSkill = (
  id: string,
  patch: {
    name?: string;
    kind?: SkillKind;
    target?: string;
    prompt?: string;
    enabled?: boolean;
  },
) => invoke<void>("skill_update", { id, ...patch });
export const deleteSkill = (id: string) => invoke<void>("skill_delete", { id });

export const meetingSkills = (meetingId: string) => invoke<Skill[]>("meeting_skills", { meetingId });
export const attachSkill = (meetingId: string, skillId: string) =>
  invoke<void>("meeting_attach_skill", { meetingId, skillId });
export const detachSkill = (meetingId: string, skillId: string) =>
  invoke<void>("meeting_detach_skill", { meetingId, skillId });

export const skillRuns = (meetingId: string) => invoke<SkillRun[]>("skill_runs", { meetingId });
export const runSkillsNow = (meetingId: string) => invoke<void>("skills_run_now", { meetingId });

/**
 * Tauri rejects with whatever the command returned, which for us is always the
 * `Display` string of `AppError`. Normalize it so callers can show it directly.
 */
export function errorMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}
