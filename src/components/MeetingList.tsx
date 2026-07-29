import { useEffect, useState } from "react";
import * as api from "../lib/api";
import { formatDuration, formatWhen } from "../lib/events";
import type { MeetingListItem, SearchHit } from "../lib/types";
import { TrashIcon } from "./Icons";

interface Props {
  meetings: MeetingListItem[];
  activeId: string | null;
  recordingId: string | null;
  onSelect: (id: string) => void;
  onDelete: (id: string) => void;
}

/**
 * The middle column. Typing filters the visible meetings immediately on title
 * and snippet, and in parallel runs a full-text search across every transcript —
 * so a word someone said three weeks ago surfaces even though it appears in no
 * card. Transcript hits are appended below the title matches rather than
 * replacing them.
 */
export default function MeetingList(props: Props) {
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<SearchHit[]>([]);

  useEffect(() => {
    const q = query.trim();
    if (q.length < 2) {
      setHits([]);
      return;
    }
    const timer = window.setTimeout(() => {
      api
        .search(q, 20)
        .then(setHits)
        .catch(() => setHits([]));
    }, 200);
    return () => window.clearTimeout(timer);
  }, [query]);

  const q = query.trim().toLowerCase();
  const visible = q
    ? props.meetings.filter(
        (m) =>
          m.title.toLowerCase().includes(q) || (m.snippet ?? "").toLowerCase().includes(q),
      )
    : props.meetings;

  // Transcript hits for meetings the card list already shows would be noise.
  const shown = new Set(visible.map((m) => m.id));
  const extra = hits.filter((h) => !shown.has(h.meetingId));
  const seen = new Set<string>();
  const extraMeetings = extra.filter((h) =>
    seen.has(h.meetingId) ? false : (seen.add(h.meetingId), true),
  );

  return (
    <section className="list-col">
      <input
        className="input"
        placeholder="Search notes and transcripts"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
      />

      <div className="list-scroll">
        {visible.length === 0 && extraMeetings.length === 0 && (
          <p className="small muted" style={{ padding: "0 var(--space-4)" }}>
            {q ? "Nothing matches that." : "No meetings yet."}
          </p>
        )}

        {visible.map((m) => (
          <Card
            key={m.id}
            meeting={m}
            active={props.activeId === m.id}
            recording={props.recordingId === m.id}
            onSelect={props.onSelect}
            onDelete={props.onDelete}
          />
        ))}

        {extraMeetings.length > 0 && (
          <>
            <div className="rail-group-label" style={{ paddingTop: "var(--space-3)" }}>
              Said in
            </div>
            {extraMeetings.map((h) => (
              <button
                key={h.meetingId}
                className="note-card"
                onClick={() => props.onSelect(h.meetingId)}
              >
                <div className="note-card-title">{h.meetingTitle}</div>
                <div className="note-card-snippet">“{h.text}”</div>
              </button>
            ))}
          </>
        )}
      </div>
    </section>
  );
}

function Card({
  meeting: m,
  active,
  recording,
  onSelect,
  onDelete,
}: {
  meeting: MeetingListItem;
  active: boolean;
  recording: boolean;
  onSelect: (id: string) => void;
  onDelete: (id: string) => void;
}) {
  const when = formatWhen(m.startedAt ?? m.createdAt);
  const duration = recording
    ? "recording"
    : m.status === "processing"
      ? "writing up…"
      : formatDuration(m.durationSecs);

  return (
    <div className="note-card-wrap">
      <button
        className={`note-card ${active ? "is-on" : ""}`}
        onClick={() => onSelect(m.id)}
      >
        <div className="note-card-meta">
          {recording && <span className="rec-dot" style={{ width: 7, height: 7 }} />}
          {when}
          {duration && (
            <>
              <span>·</span>
              {duration}
            </>
          )}
        </div>
        <div className="note-card-title">{m.title}</div>
        {m.snippet && <div className="note-card-snippet">{m.snippet}</div>}
      </button>

      <button
        className="btn btn-ghost btn-icon note-card-del"
        title="Delete meeting"
        onClick={() => {
          if (window.confirm(`Delete “${m.title}”? This cannot be undone.`)) onDelete(m.id);
        }}
      >
        <TrashIcon />
      </button>
    </div>
  );
}
