import type { Folder, MeetingListItem, ModelStatus } from "../lib/types";
import { PlusIcon } from "./Icons";

export type View = "notes" | "skills" | "settings";

const NAV: [View, string][] = [
  ["notes", "All notes"],
  ["skills", "Skills & prompts"],
  ["settings", "Settings"],
];

interface Props {
  folders: Folder[];
  meetings: MeetingListItem[];
  activeFolder: string | null;
  modelStatus: ModelStatus | null;
  view: View;
  onPickFolder: (id: string | null) => void;
  onNewFolder: () => void;
  onNewMeeting: () => void;
  onPickView: (v: View) => void;
}

/**
 * The left rail: brand, the one primary action, folders, and a status pill.
 *
 * The pill doubles as the way into Settings — the design has no gear icon, and
 * "which models am I running" is exactly what you want to check before opening
 * that screen anyway.
 */
export default function Rail(props: Props) {
  const countFor = (id: string | null) =>
    id === null
      ? props.meetings.length
      : props.meetings.filter((m) => m.folderId === id).length;

  const status = props.modelStatus;

  return (
    <aside className="rail">
      <div className="rail-brand">
        <div className="rail-mark" />
        <h1>Amble</h1>
      </div>

      <button className="btn btn-primary btn-block" onClick={props.onNewMeeting}>
        <PlusIcon />
        New meeting
      </button>

      <div className="rail-group grow">
        <div className="rail-group-label">
          <span>Folders</span>
          <button
            className="btn btn-ghost"
            style={{ padding: "0 4px", fontSize: 13 }}
            onClick={props.onNewFolder}
            title="New folder"
          >
            +
          </button>
        </div>

        <div className="rail-scroll">
          <button
            className={`folder-btn ${props.activeFolder === null ? "is-on" : ""}`}
            onClick={() => props.onPickFolder(null)}
          >
            <span>All</span>
            <span className="count">{countFor(null)}</span>
          </button>

          {props.folders.map((f) => (
            <button
              key={f.id}
              className={`folder-btn ${props.activeFolder === f.id ? "is-on" : ""}`}
              onClick={() => props.onPickFolder(f.id)}
            >
              <span>{f.name}</span>
              <span className="count">{countFor(f.id)}</span>
            </button>
          ))}
        </div>
      </div>

      <div className="rail-group" style={{ marginTop: "auto" }}>
        {NAV.map(([key, name]) => (
          <button
            key={key}
            className={`rail-nav ${props.view === key ? "is-on" : ""}`}
            onClick={() => props.onPickView(key)}
          >
            {name}
          </button>
        ))}
      </div>

      <button
        className={`rail-status ${status?.needsSetup ? "is-warn" : ""}`}
        onClick={() => props.onPickView("settings")}
        title="Open settings"
      >
        <span className="dot" />
        {status?.label ?? "Loading models…"}
      </button>
    </aside>
  );
}
