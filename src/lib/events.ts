// Events pushed from Rust. Names mirror `src-tauri/src/meeting/events.rs`.

import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Segment, Suggestion } from "./types";

export const EVENT = {
  status: "meeting://status",
  segment: "meeting://segment",
  levels: "meeting://levels",
  suggestion: "meeting://suggestion",
  notes: "meeting://notes",
  updated: "meeting://updated",
  chatDelta: "chat://delta",
  chatDone: "chat://done",
} as const;

export interface StatusEvent {
  meetingId: string;
  status: string;
  message: string | null;
}

export interface LevelsEvent {
  meetingId: string;
  mic: number;
  system: number;
}

export interface NotesEvent {
  meetingId: string;
  content: string;
}

export interface ChatDeltaEvent {
  meetingId: string;
  delta: string;
}

export interface ChatDoneEvent {
  meetingId: string;
  content: string;
}

type Handlers = {
  onStatus?: (e: StatusEvent) => void;
  onSegment?: (e: Segment) => void;
  onLevels?: (e: LevelsEvent) => void;
  onSuggestion?: (e: Suggestion) => void;
  onNotes?: (e: NotesEvent) => void;
  onUpdated?: (meetingId: string) => void;
  onChatDelta?: (e: ChatDeltaEvent) => void;
  onChatDone?: (e: ChatDoneEvent) => void;
};

/**
 * Subscribe to the recording event stream. Returns a cleanup function.
 *
 * `listen` is async, so a component that unmounts before the subscription
 * resolves would otherwise leak it — hence the `cancelled` guard.
 */
export function subscribe(handlers: Handlers): () => void {
  const unlisteners: UnlistenFn[] = [];
  let cancelled = false;

  const attach = async <T,>(name: string, handler?: (payload: T) => void) => {
    if (!handler) return;
    const un = await listen<T>(name, (event) => handler(event.payload));
    if (cancelled) un();
    else unlisteners.push(un);
  };

  void Promise.all([
    attach<StatusEvent>(EVENT.status, handlers.onStatus),
    attach<Segment>(EVENT.segment, handlers.onSegment),
    attach<LevelsEvent>(EVENT.levels, handlers.onLevels),
    attach<Suggestion>(EVENT.suggestion, handlers.onSuggestion),
    attach<NotesEvent>(EVENT.notes, handlers.onNotes),
    attach<string>(EVENT.updated, handlers.onUpdated),
    attach<ChatDeltaEvent>(EVENT.chatDelta, handlers.onChatDelta),
    attach<ChatDoneEvent>(EVENT.chatDone, handlers.onChatDone),
  ]);

  return () => {
    cancelled = true;
    unlisteners.forEach((un) => un());
  };
}

/** `123456` → `2:03`. Used for transcript timestamps. */
export function formatMs(ms: number): string {
  const total = Math.floor(ms / 1000);
  const m = Math.floor(total / 60);
  const s = total % 60;
  return `${m}:${String(s).padStart(2, "0")}`;
}
