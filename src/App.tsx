import { useCallback, useEffect, useState } from "react";
import * as api from "./lib/api";
import { subscribe } from "./lib/events";
import type { Folder, MeetingListItem, ModelStatus, RecordingState } from "./lib/types";
import Rail, { type View } from "./components/Rail";
import MeetingList from "./components/MeetingList";
import MeetingView from "./components/MeetingView";
import SettingsView from "./components/SettingsView";
import SkillsView from "./components/SkillsView";
import { PlusIcon } from "./components/Icons";
import "./App.css";

export default function App() {
  const [meetings, setMeetings] = useState<MeetingListItem[]>([]);
  const [folders, setFolders] = useState<Folder[]>([]);
  const [folderFilter, setFolderFilter] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [view, setView] = useState<View>("notes");
  const [chatOpen, setChatOpen] = useState(true);
  const [modelStatus, setModelStatus] = useState<ModelStatus | null>(null);
  const [recording, setRecording] = useState<RecordingState>({
    recording: false,
    meetingId: null,
  });
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const [m, f] = await Promise.all([api.listMeetings(), api.listFolders()]);
      setMeetings(m);
      setFolders(f);
    } catch (e) {
      setError(api.errorMessage(e));
    }
  }, []);

  const refreshModelStatus = useCallback(() => {
    void api.getModelStatus().then(setModelStatus).catch(() => {});
  }, []);

  useEffect(() => {
    void refresh();
    refreshModelStatus();
    // Recording may already be running if the window was closed and reopened.
    void api
      .getRecordingState()
      .then((s) => {
        setRecording(s);
        if (s.meetingId) setSelectedId((cur) => cur ?? s.meetingId);
      })
      .catch(() => {});
  }, [refresh, refreshModelStatus]);

  // Status and title changes matter for every meeting in the list, not just the
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
              m.id === e.meetingId ? { ...m, status: e.status as MeetingListItem["status"] } : m,
            ),
          );
          // A finished meeting has a fresh snippet and duration to pick up.
          if (e.status === "done") void refresh();
        },
        onUpdated: () => void refresh(),
        onNotes: () => void refresh(),
      }),
    [refresh],
  );

  // Skills and Settings take the full stage, so the chat aside stands down.
  const chatVisible = chatOpen && view === "notes";

  const newMeeting = async () => {
    try {
      const m = await api.createMeeting({ folderId: folderFilter });
      setSelectedId(m.id);
      setView("notes");
      await refresh();

      // The design starts capturing straight away — this is a recorder, and the
      // button is one click from a live conversation. If permission isn't
      // granted the meeting still exists, unrecorded, with the reason shown.
      try {
        await api.startRecording(m.id);
      } catch (e) {
        setError(api.errorMessage(e));
      }
    } catch (e) {
      setError(api.errorMessage(e));
    }
  };

  const newFolder = async () => {
    const name = window.prompt("Folder name");
    if (!name?.trim()) return;
    try {
      await api.createFolder(name.trim());
      await refresh();
    } catch (e) {
      setError(api.errorMessage(e));
    }
  };

  const removeMeeting = async (id: string) => {
    try {
      await api.deleteMeeting(id);
      if (selectedId === id) setSelectedId(null);
      await refresh();
    } catch (e) {
      setError(api.errorMessage(e));
    }
  };

  const visible =
    folderFilter === null ? meetings : meetings.filter((m) => m.folderId === folderFilter);

  return (
    <div className="app">
      <Rail
        folders={folders}
        meetings={meetings}
        activeFolder={folderFilter}
        modelStatus={modelStatus}
        view={view}
        onPickFolder={(id) => {
          setFolderFilter(id);
          setView("notes");
        }}
        onNewFolder={newFolder}
        onNewMeeting={newMeeting}
        onPickView={setView}
      />

      {view === "notes" && (
        <MeetingList
          meetings={visible}
          activeId={selectedId}
          recordingId={recording.meetingId}
          onSelect={(id) => {
            setSelectedId(id);
            setView("notes");
          }}
          onDelete={removeMeeting}
        />
      )}

      {error && (
        <div
          className="banner banner-error"
          style={{ position: "fixed", bottom: 16, right: 16, zIndex: 10, maxWidth: 460, margin: 0 }}
        >
          <span>{error}</span>
          <button className="btn btn-ghost" onClick={() => setError(null)}>
            Dismiss
          </button>
        </div>
      )}

      {view === "settings" ? (
        <SettingsView onSaved={refreshModelStatus} onError={setError} />
      ) : view === "skills" ? (
        <SkillsView onError={setError} />
      ) : selectedId ? (
        <MeetingView
          key={selectedId}
          meetingId={selectedId}
          folders={folders}
          isRecording={recording.meetingId === selectedId}
          anotherRecording={recording.recording && recording.meetingId !== selectedId}
          chatOpen={chatVisible}
          onToggleChat={() => setChatOpen((c) => !c)}
          onChanged={refresh}
        />
      ) : (
        <section className="stage">
          <div className="blank">
            <h2>Nothing selected</h2>
            <p>
              Start a meeting and Amble records your microphone and everything playing through your
              speakers, transcribes as it goes, and writes it up when you stop.
            </p>
            <button className="btn btn-primary" onClick={newMeeting}>
              <PlusIcon />
              New meeting
            </button>
            {modelStatus?.needsSetup && (
              <button className="btn btn-ghost" onClick={() => setView("settings")}>
                Finish setting up your models first
              </button>
            )}
          </div>
        </section>
      )}
    </div>
  );
}
