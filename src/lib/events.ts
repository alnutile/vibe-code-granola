// Events pushed from Rust. Names mirror `src-tauri/src/meeting/events.rs`.

import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { RenderedNotes, Segment, SkillRun, Suggestion } from "./types";

export const EVENT = {
  status: "meeting://status",
  segment: "meeting://segment",
  levels: "meeting://levels",
  suggestion: "meeting://suggestion",
  notes: "meeting://notes",
  updated: "meeting://updated",
  chatDelta: "chat://delta",
  chatDone: "chat://done",
  runs: "meeting://runs",
} as const;

export interface RunsEvent {
  meetingId: string;
  runs: SkillRun[];
}

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
  notes: RenderedNotes;
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
  onRuns?: (e: RunsEvent) => void;
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
    attach<RunsEvent>(EVENT.runs, handlers.onRuns),
  ]);

  return () => {
    cancelled = true;
    unlisteners.forEach((un) => un());
  };
}

/** `123456` → `02:03`. Transcript timestamps and the recording timer. */
export function formatClock(totalSeconds: number): string {
  const s = Math.max(0, Math.floor(totalSeconds));
  return `${String(Math.floor(s / 60)).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`;
}

export const formatMs = (ms: number) => formatClock(ms / 1000);

/** `2550` → `42 min`. Meeting length on the list cards. */
export function formatDuration(secs: number | null): string | null {
  if (secs === null || secs < 0) return null;
  if (secs < 60) return `${Math.round(secs)} sec`;
  const mins = Math.round(secs / 60);
  if (mins < 60) return `${mins} min`;
  const hours = Math.floor(mins / 60);
  const rem = mins % 60;
  return rem ? `${hours}h ${rem}m` : `${hours}h`;
}

/** "Today · 9:30 AM", "Yesterday · 2:00 PM", "Mon · 4:15 PM", else a date. */
export function formatWhen(iso: string | null): string {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";

  const time = d.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });

  const startOfDay = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const days = Math.round((startOfDay(new Date()) - startOfDay(d)) / 86_400_000);

  if (days === 0) return `Today · ${time}`;
  if (days === 1) return `Yesterday · ${time}`;
  if (days > 1 && days < 7) return `${d.toLocaleDateString(undefined, { weekday: "short" })} · ${time}`;
  return `${d.toLocaleDateString(undefined, { month: "short", day: "numeric" })} · ${time}`;
}
