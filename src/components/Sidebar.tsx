import { useEffect, useState } from "react";
import * as api from "../lib/api";
import type { Folder, Meeting, SearchHit } from "../lib/types";

interface Props {
  meetings: Meeting[];
  folders: Folder[];
  folderFilter: string | null;
  activeId: string | null;
  recordingId: string | null;
  settingsOpen: boolean;
  onSelectFolder: (id: string | null) => void;
  onSelect: (id: string) => void;
  onNew: () => void;
  onDelete: (id: string) => void;
  onOpenSettings: () => void;
  onFoldersChanged: () => void;
}

export default function Sidebar(props: Props) {
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<SearchHit[] | null>(null);

  // Debounced full-text search. Typing in the box swaps the meeting list for
  // matching transcript lines.
  useEffect(() => {
    const q = query.trim();
    if (!q) {
      setHits(null);
      return;
    }
    const timer = setTimeout(() => {
      api
        .search(q)
        .then(setHits)
        .catch(() => setHits([]));
    }, 200);
    return () => clearTimeout(timer);
  }, [query]);

  const addFolder = async () => {
    const name = window.prompt("Folder name");
    if (!name?.trim()) return;
    await api.createFolder(name.trim());
    props.onFoldersChanged();
  };

  return (
    <aside className="sidebar">
      <div className="sidebar-top">
        <button className="btn btn-primary btn-block" onClick={props.onNew}>
          + New meeting
        </button>
        <input
          className="search"
          placeholder="Search transcripts…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
      </div>

      {hits === null ? (
        <>
          <div className="sidebar-section">
            <div className="sidebar-section-head">
              <span>Folders</span>
              <button className="icon-btn" title="New folder" onClick={addFolder}>
                +
              </button>
            </div>
            <button
              className={`folder ${props.folderFilter === null ? "active" : ""}`}
              onClick={() => props.onSelectFolder(null)}
            >
              All meetings
            </button>
            {props.folders.map((f) => (
              <button
                key={f.id}
                className={`folder ${props.folderFilter === f.id ? "active" : ""}`}
                onClick={() => props.onSelectFolder(f.id)}
              >
                {f.name}
              </button>
            ))}
          </div>

          <div className="sidebar-section grow">
            <div className="sidebar-section-head">
              <span>Meetings</span>
            </div>
            {props.meetings.length === 0 && <p className="muted small">Nothing recorded yet.</p>}
            {props.meetings.map((m) => (
              <div
                key={m.id}
                className={`meeting-row ${props.activeId === m.id ? "active" : ""}`}
                onClick={() => props.onSelect(m.id)}
              >
                <div className="meeting-row-main">
                  <div className="meeting-row-title">
                    {props.recordingId === m.id && <span className="rec-dot" aria-label="recording" />}
                    {m.title}
                  </div>
                  <div className="meeting-row-meta">
                    {formatDate(m.startedAt ?? m.createdAt)}
                    {m.status === "processing" && " · writing up…"}
                  </div>
                </div>
                <button
                  className="icon-btn danger"
                  title="Delete meeting"
                  onClick={(e) => {
                    e.stopPropagation();
                    if (window.confirm(`Delete "${m.title}"? This cannot be undone.`)) {
                      props.onDelete(m.id);
                    }
                  }}
                >
                  ×
                </button>
              </div>
            ))}
          </div>
        </>
      ) : (
        <div className="sidebar-section grow">
          <div className="sidebar-section-head">
            <span>{hits.length} result{hits.length === 1 ? "" : "s"}</span>
          </div>
          {hits.map((h) => (
            <div key={h.segmentId} className="hit" onClick={() => props.onSelect(h.meetingId)}>
              <div className="hit-title">{h.meetingTitle}</div>
              <div className="hit-text">{h.text}</div>
            </div>
          ))}
        </div>
      )}

      <button
        className={`settings-btn ${props.settingsOpen ? "active" : ""}`}
        onClick={props.onOpenSettings}
      >
        Settings
      </button>
    </aside>
  );
}

function formatDate(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  return d.toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}
