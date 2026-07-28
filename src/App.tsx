import { useCallback, useEffect, useState } from "react";
import * as api from "./lib/api";
import { subscribe } from "./lib/events";
import type { Folder, Meeting, RecordingState } from "./lib/types";
import Sidebar from "./components/Sidebar";
import MeetingView from "./components/MeetingView";
import SettingsView from "./components/SettingsView";
import "./App.css";

type Route = { kind: "meeting"; id: string } | { kind: "settings" } | { kind: "empty" };

export default function App() {
  const [meetings, setMeetings] = useState<Meeting[]>([]);
  const [folders, setFolders] = useState<Folder[]>([]);
  const [folderFilter, setFolderFilter] = useState<string | null>(null);
  const [route, setRoute] = useState<Route>({ kind: "empty" });
  const [recording, setRecording] = useState<RecordingState>({ recording: false, meetingId: null });
  const [error, setError] = useState<string | null>(null);

  const refreshMeetings = useCallback(async () => {
    try {
      const [m, f] = await Promise.all([api.listMeetings(folderFilter), api.listFolders()]);
      setMeetings(m);
      setFolders(f);
    } catch (e) {
      setError(api.errorMessage(e));
    }
  }, [folderFilter]);

  useEffect(() => {
    void refreshMeetings();
  }, [refreshMeetings]);

  // Recording may already be in progress if the window was closed and reopened,
  // so ask rather than assume it isn't.
  useEffect(() => {
    void api
      .getRecordingState()
      .then(setRecording)
      .catch(() => {});
  }, []);

  // The sidebar reflects status and title changes for every meeting, not just the
  // open one, so this subscription lives at the top of the tree.
  useEffect(
    () =>
      subscribe({
        onStatus: (e) => {
          setRecording({
            recording: e.status === "recording",
            meetingId: e.status === "recording" ? e.meetingId : null,
          });
          setMeetings((prev) =>
            prev.map((m) =>
              m.id === e.meetingId ? { ...m, status: e.status as Meeting["status"] } : m,
            ),
          );
        },
        onUpdated: () => void refreshMeetings(),
      }),
    [refreshMeetings],
  );

  const newMeeting = async () => {
    try {
      const m = await api.createMeeting({ folderId: folderFilter });
      await refreshMeetings();
      setRoute({ kind: "meeting", id: m.id });
    } catch (e) {
      setError(api.errorMessage(e));
    }
  };

  const removeMeeting = async (id: string) => {
    try {
      await api.deleteMeeting(id);
      if (route.kind === "meeting" && route.id === id) setRoute({ kind: "empty" });
      await refreshMeetings();
    } catch (e) {
      setError(api.errorMessage(e));
    }
  };

  return (
    <div className="app">
      <Sidebar
        meetings={meetings}
        folders={folders}
        folderFilter={folderFilter}
        activeId={route.kind === "meeting" ? route.id : null}
        recordingId={recording.meetingId}
        settingsOpen={route.kind === "settings"}
        onSelectFolder={setFolderFilter}
        onSelect={(id) => setRoute({ kind: "meeting", id })}
        onNew={newMeeting}
        onDelete={removeMeeting}
        onOpenSettings={() => setRoute({ kind: "settings" })}
        onFoldersChanged={refreshMeetings}
      />

      <main className="main">
        {error && (
          <div className="banner banner-error">
            <span>{error}</span>
            <button onClick={() => setError(null)}>Dismiss</button>
          </div>
        )}

        {route.kind === "settings" && <SettingsView onClose={() => setRoute({ kind: "empty" })} />}

        {route.kind === "meeting" && (
          <MeetingView
            key={route.id}
            meetingId={route.id}
            isRecording={recording.meetingId === route.id}
            anotherRecording={recording.recording && recording.meetingId !== route.id}
            onChanged={refreshMeetings}
          />
        )}

        {route.kind === "empty" && (
          <div className="empty-state">
            <h1>vibecode-granola</h1>
            <p>
              Start a meeting and it records your microphone and everything playing through your
              speakers, transcribes as it goes, and writes it up when you stop.
            </p>
            <button className="btn btn-primary" onClick={newMeeting}>
              New meeting
            </button>
            <p className="hint">
              First run?{" "}
              <button className="link" onClick={() => setRoute({ kind: "settings" })}>
                Open Settings
              </button>{" "}
              to choose your models and grant recording permission.
            </p>
          </div>
        )}
      </main>
    </div>
  );
}
