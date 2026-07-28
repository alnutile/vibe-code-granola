import { useEffect, useRef, useState } from "react";
import Markdown from "react-markdown";
import * as api from "../lib/api";
import type { Note } from "../lib/types";

interface Props {
  meetingId: string;
  aiNote?: Note;
  userNote?: Note;
  hasTranscript: boolean;
  onUserNoteSaved: (n: Note) => void;
  onError: (msg: string) => void;
}

export default function NotesPane(props: Props) {
  const [draft, setDraft] = useState(props.userNote?.content ?? "");
  const [saving, setSaving] = useState(false);
  const [generating, setGenerating] = useState(false);
  const saveTimer = useRef<number | null>(null);

  // Adopt the stored note when switching meetings, but never clobber what the
  // user is currently typing.
  useEffect(() => {
    setDraft(props.userNote?.content ?? "");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [props.meetingId]);

  // Autosave. Typing during a meeting shouldn't require remembering to save.
  const onDraftChange = (value: string) => {
    setDraft(value);
    if (saveTimer.current) window.clearTimeout(saveTimer.current);
    saveTimer.current = window.setTimeout(() => {
      setSaving(true);
      api
        .saveNote(props.meetingId, value)
        .then(props.onUserNoteSaved)
        .catch((e) => props.onError(api.errorMessage(e)))
        .finally(() => setSaving(false));
    }, 800);
  };

  useEffect(() => () => void (saveTimer.current && window.clearTimeout(saveTimer.current)), []);

  const generate = async () => {
    setGenerating(true);
    try {
      await api.generateNotes(props.meetingId);
    } catch (e) {
      props.onError(api.errorMessage(e));
    } finally {
      setGenerating(false);
    }
  };

  return (
    <div className="pane notes">
      <div className="notes-section">
        <div className="notes-head">
          <h3>Your notes</h3>
          <span className="muted small">{saving ? "Saving…" : "Saved automatically"}</span>
        </div>
        <textarea
          className="notes-editor"
          placeholder="Type as you go. Whatever you write here is treated as higher-signal than the transcript when the write-up is generated."
          value={draft}
          onChange={(e) => onDraftChange(e.target.value)}
        />
      </div>

      <div className="notes-section grow">
        <div className="notes-head">
          <h3>Write-up</h3>
          <button
            className="btn btn-small"
            onClick={generate}
            disabled={generating || !props.hasTranscript}
            title={props.hasTranscript ? undefined : "There's no transcript to write up yet."}
          >
            {generating ? "Writing…" : props.aiNote ? "Regenerate" : "Generate"}
          </button>
        </div>

        {props.aiNote ? (
          <div className="markdown">
            <Markdown>{props.aiNote.content}</Markdown>
          </div>
        ) : (
          <p className="muted small">
            {props.hasTranscript
              ? "Not written up yet. This happens automatically when you stop recording."
              : "Record the meeting and the write-up appears here when you stop."}
          </p>
        )}
      </div>
    </div>
  );
}
