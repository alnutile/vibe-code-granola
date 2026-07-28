import { useEffect, useState } from "react";
import * as api from "../lib/api";
import { subscribe } from "../lib/events";
import type { Meeting, Note, Segment, Suggestion, Template } from "../lib/types";
import TranscriptPane from "./TranscriptPane";
import NotesPane from "./NotesPane";
import AssistantPane from "./AssistantPane";

interface Props {
  meetingId: string;
  isRecording: boolean;
  /** A different meeting is recording — starting this one would fail. */
  anotherRecording: boolean;
  onChanged: () => void;
}

type Tab = "notes" | "transcript";

export default function MeetingView({ meetingId, isRecording, anotherRecording, onChanged }: Props) {
  const [meeting, setMeeting] = useState<Meeting | null>(null);
  const [templates, setTemplates] = useState<Template[]>([]);
  const [segments, setSegments] = useState<Segment[]>([]);
  const [notes, setNotes] = useState<Note[]>([]);
  const [suggestions, setSuggestions] = useState<Suggestion[]>([]);
  const [levels, setLevels] = useState({ mic: 0, system: 0 });
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [tab, setTab] = useState<Tab>("notes");
  const [busy, setBusy] = useState(false);

  // Load everything for this meeting.
  useEffect(() => {
    let alive = true;
    void (async () => {
      try {
        const [m, t, s, n, g] = await Promise.all([
          api.getMeeting(meetingId),
          api.listTemplates(),
          api.listSegments(meetingId),
          api.listNotes(meetingId),
          api.listSuggestions(meetingId),
        ]);
        if (!alive) return;
        setMeeting(m);
        setTemplates(t);
        setSegments(s);
        setNotes(n);
        setSuggestions(g);
      } catch (e) {
        if (alive) setError(api.errorMessage(e));
      }
    })();
    return () => {
      alive = false;
    };
  }, [meetingId]);

  // Live updates. Every handler filters by meeting id — events are broadcast to
  // the whole window, and a background meeting must not write into this view.
  useEffect(
    () =>
      subscribe({
        onSegment: (s) => {
          if (s.meetingId === meetingId) setSegments((prev) => [...prev, s]);
        },
        onLevels: (l) => {
          if (l.meetingId === meetingId) setLevels({ mic: l.mic, system: l.system });
        },
        onSuggestion: (s) => {
          if (s.meetingId === meetingId) setSuggestions((prev) => [...prev, s]);
        },
        onNotes: (n) => {
          if (n.meetingId !== meetingId) return;
          setNotes((prev) => {
            const others = prev.filter((x) => x.kind !== "ai");
            const existing = prev.find((x) => x.kind === "ai");
            return [
              ...others,
              {
                ...(existing ?? {
                  id: "ai",
                  meetingId,
                  kind: "ai" as const,
                  createdAt: new Date().toISOString(),
                }),
                content: n.content,
                updatedAt: new Date().toISOString(),
              } as Note,
            ];
          });
          setTab("notes");
        },
        onStatus: (e) => {
          if (e.meetingId !== meetingId) return;
          setStatus(e.message ?? null);
          setMeeting((prev) => (prev ? { ...prev, status: e.status as Meeting["status"] } : prev));
          if (e.status === "done") {
            void api.getMeeting(meetingId).then(setMeeting).catch(() => {});
            void api.listNotes(meetingId).then(setNotes).catch(() => {});
          }
        },
      }),
    [meetingId],
  );

  const patch = async (p: Parameters<typeof api.updateMeeting>[1]) => {
    if (!meeting) return;
    setMeeting({ ...meeting, ...p } as Meeting);
    try {
      await api.updateMeeting(meetingId, p);
      onChanged();
    } catch (e) {
      setError(api.errorMessage(e));
    }
  };

  const toggleRecording = async () => {
    setError(null);
    setBusy(true);
    try {
      if (isRecording) await api.stopRecording(meetingId);
      else await api.startRecording(meetingId);
    } catch (e) {
      setError(api.errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  if (!meeting) {
    return <div className="pane-loading">{error ?? "Loading…"}</div>;
  }

  const aiNote = notes.find((n) => n.kind === "ai");
  const userNote = notes.find((n) => n.kind === "user");

  return (
    <div className="meeting">
      <header className="meeting-header">
        <div className="meeting-header-row">
          <input
            className="title-input"
            value={meeting.title}
            placeholder="Untitled meeting"
            onChange={(e) => setMeeting({ ...meeting, title: e.target.value })}
            onBlur={(e) => void patch({ title: e.target.value })}
          />

          <div className="record-controls">
            {isRecording && <LevelMeters mic={levels.mic} system={levels.system} />}
            <button
              className={`btn ${isRecording ? "btn-stop" : "btn-record"}`}
              onClick={toggleRecording}
              disabled={busy || (anotherRecording && !isRecording)}
              title={
                anotherRecording && !isRecording
                  ? "Another meeting is recording. Stop it first."
                  : undefined
              }
            >
              {isRecording ? "■ Stop" : "● Record"}
            </button>
          </div>
        </div>

        <textarea
          className="prompt-input"
          rows={2}
          placeholder="What's this meeting about, and what do you want out of it? This steers the live suggestions and the write-up."
          value={meeting.prompt}
          onChange={(e) => setMeeting({ ...meeting, prompt: e.target.value })}
          onBlur={(e) => void patch({ prompt: e.target.value })}
        />

        <div className="meeting-header-row small">
          <label className="field-inline">
            Write-up style
            <select
              value={meeting.templateId ?? ""}
              onChange={(e) => void patch({ templateId: e.target.value || null })}
            >
              <option value="">General meeting</option>
              {templates.map((t) => (
                <option key={t.id} value={t.id}>
                  {t.name}
                </option>
              ))}
            </select>
          </label>

          {status && <span className="status-pill">{status}</span>}
          {meeting.status === "processing" && !status && (
            <span className="status-pill">Finishing up…</span>
          )}
        </div>
      </header>

      {error && (
        <div className="banner banner-error">
          <span>{error}</span>
          <button onClick={() => setError(null)}>Dismiss</button>
        </div>
      )}

      <div className="meeting-body">
        <section className="meeting-left">
          <div className="tabs">
            <button className={tab === "notes" ? "active" : ""} onClick={() => setTab("notes")}>
              Notes
            </button>
            <button
              className={tab === "transcript" ? "active" : ""}
              onClick={() => setTab("transcript")}
            >
              Transcript {segments.length > 0 && <span className="count">{segments.length}</span>}
            </button>
          </div>

          {tab === "notes" ? (
            <NotesPane
              meetingId={meetingId}
              aiNote={aiNote}
              userNote={userNote}
              hasTranscript={segments.length > 0}
              onUserNoteSaved={(n) =>
                setNotes((prev) => [...prev.filter((x) => x.kind !== "user"), n])
              }
              onError={setError}
            />
          ) : (
            <TranscriptPane segments={segments} isRecording={isRecording} />
          )}
        </section>

        <AssistantPane
          meetingId={meetingId}
          suggestions={suggestions}
          isRecording={isRecording}
          hasTranscript={segments.length > 0}
          onError={setError}
        />
      </div>
    </div>
  );
}

/**
 * Live input levels, so it's obvious at a glance whether audio is actually
 * arriving — the single most common "is this thing on?" moment.
 */
function LevelMeters({ mic, system }: { mic: number; system: number }) {
  return (
    <div className="meters">
      <Meter label="Mic" value={mic} />
      <Meter label="Sys" value={system} />
    </div>
  );
}

function Meter({ label, value }: { label: string; value: number }) {
  // RMS runs small even for loud speech, so scale it into something visible.
  const pct = Math.min(100, Math.round(value * 400));

  return (
    <div className="meter" title={`${label} level`}>
      <span className="meter-label">{label}</span>
      <div className="meter-track">
        <div className="meter-fill" style={{ width: `${pct}%` }} />
      </div>
    </div>
  );
}
