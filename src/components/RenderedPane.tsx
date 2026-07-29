import { useState } from "react";
import Markdown from "react-markdown";
import * as api from "../lib/api";
import type { RenderedNotes, SkillRun } from "../lib/types";

interface Props {
  meetingId: string;
  notes: RenderedNotes | null;
  runs: SkillRun[];
  hasPostSkills: boolean;
  hasTranscript: boolean;
  onNotes: (n: RenderedNotes) => void;
  onError: (msg: string) => void;
}

/** The generated write-up: summary, numbered key points, tickable actions. */
export default function RenderedPane(props: Props) {
  const [generating, setGenerating] = useState(false);
  const [rerunning, setRerunning] = useState(false);

  const rerun = async () => {
    setRerunning(true);
    try {
      await api.runSkillsNow(props.meetingId);
    } catch (e) {
      props.onError(api.errorMessage(e));
    } finally {
      setRerunning(false);
    }
  };

  const generate = async () => {
    setGenerating(true);
    try {
      props.onNotes(await api.generateNotes(props.meetingId));
    } catch (e) {
      props.onError(api.errorMessage(e));
    } finally {
      setGenerating(false);
    }
  };

  const toggle = async (index: number) => {
    // Flip locally first — a checkbox that waits on a round trip feels broken.
    if (props.notes) {
      props.onNotes({
        ...props.notes,
        actions: props.notes.actions.map((a, i) => (i === index ? { ...a, done: !a.done } : a)),
      });
    }
    try {
      props.onNotes(await api.toggleAction(props.meetingId, index));
    } catch (e) {
      props.onError(api.errorMessage(e));
    }
  };

  if (!props.notes) {
    return (
      <div className="doc-empty">
        <span className="tag tag-accent-2">Waiting on audio</span>
        <p>
          Nothing rendered yet. Keep typing in <strong>My notes</strong> while you talk — when you
          stop the recording, the model stitches your notes and the transcript into this page.
        </p>
        {props.hasTranscript && (
          <button className="btn btn-secondary" onClick={generate} disabled={generating}>
            {generating ? "Writing…" : "Render it now"}
          </button>
        )}
      </div>
    );
  }

  const { summary, points, actions, sections } = props.notes;

  return (
    <>
      {summary && <p className="doc-summary">{summary}</p>}

      {points.length > 0 && (
        <div className="doc-section">
          <h3>Key points</h3>
          {points.map((p, i) => (
            <div className="point" key={i}>
              <span className="point-n">{i + 1}</span>
              <span className="point-text">{p}</span>
            </div>
          ))}
        </div>
      )}

      {actions.length > 0 && (
        <div className="doc-section">
          <h3>Action items</h3>
          {actions.map((a, i) => (
            <label className={`action-row ${a.done ? "is-done" : ""}`} key={i}>
              <input type="checkbox" checked={a.done} onChange={() => void toggle(i)} />
              <span className="action-row-text">{a.text}</span>
              {a.who && <span className="tag tag-accent">{a.who}</span>}
            </label>
          ))}
        </div>
      )}

      {sections.map((s, i) => (
        <div className="doc-section" key={i}>
          <h3>{s.heading}</h3>
          <div className="markdown">
            <Markdown>{s.body}</Markdown>
          </div>
        </div>
      ))}

      {(props.runs.length > 0 || props.hasPostSkills) && (
        <div className="runs">
          <div className="runs-kicker">After the meeting</div>

          {props.runs.length === 0 && (
            <p className="small" style={{ margin: 0 }}>
              Your after-skills haven't run for this meeting yet.
            </p>
          )}

          {props.runs.map((r) => (
            <RunRow key={r.id} run={r} />
          ))}

          <div className="row">
            <button className="btn btn-secondary btn-small" onClick={rerun} disabled={rerunning}>
              {rerunning ? "Running…" : props.runs.length ? "Run again" : "Run now"}
            </button>
          </div>
        </div>
      )}

      <div className="row">
        <button className="btn btn-secondary" onClick={generate} disabled={generating}>
          {generating ? "Writing…" : "Regenerate"}
        </button>
        <span className="small muted">
          Regenerating replaces this page. Your own notes are never touched.
        </span>
      </div>
    </>
  );
}

/** One post-skill result. Collapsed to its outcome until you open the output. */
function RunRow({ run }: { run: SkillRun }) {
  const [open, setOpen] = useState(false);
  const status =
    run.status === "done" ? "tag-accent-2" : run.status === "error" ? "tag-accent" : "tag-neutral";

  return (
    <div className="run">
      <div className="run-head">
        <span className="pick-body grow">
          <span className="run-name">{run.skillName}</span>
          <span className="run-msg">{run.message}</span>
        </span>
        {run.target && <span className="tag tag-accent-2">{run.target}</span>}
        <span className={`tag ${status}`}>
          {run.status === "done" ? "Done" : run.status === "error" ? "Failed" : "Running"}
        </span>
        {run.output && (
          <button className="btn btn-ghost btn-small" onClick={() => setOpen((o) => !o)}>
            {open ? "Hide" : "Show"}
          </button>
        )}
      </div>
      {open && run.output && (
        <div className="run-output markdown">
          <Markdown>{run.output}</Markdown>
        </div>
      )}
    </div>
  );
}
