// Typed wrappers over the Tauri command surface.
//
// One function per command, so a rename on the Rust side breaks here at compile
// time rather than at runtime in some component.

import { invoke } from "@tauri-apps/api/core";
import type {
  ChatMessage,
  Folder,
  McpStatus,
  Meeting,
  ModelInfo,
  Note,
  PermissionStatus,
  RecordingState,
  SearchHit,
  Segment,
  Settings,
  SettingsView,
  Suggestion,
  Template,
} from "./types";

// ------------------------------------------------------------------ settings

export const getSettings = () => invoke<SettingsView>("settings_get");
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

export const listMeetings = (folderId?: string | null) =>
  invoke<Meeting[]>("meetings_list", { folderId: folderId ?? null });
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

export const listNotes = (meetingId: string) => invoke<Note[]>("notes_list", { meetingId });
export const saveNote = (meetingId: string, content: string) =>
  invoke<Note>("note_save", { meetingId, content });
export const generateNotes = (meetingId: string) => invoke<string>("notes_generate", { meetingId });

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

/**
 * Tauri rejects with whatever the command returned, which for us is always the
 * `Display` string of `AppError`. Normalize it so callers can show it directly.
 */
export function errorMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}
