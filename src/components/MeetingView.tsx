import { useEffect, useRef, useState } from "react";
import * as api from "../lib/api";
import { formatClock, formatDuration, formatWhen, subscribe } from "../lib/events";
import type {
  Folder,
  Meeting,
  MeetingNotes,
  NoteImage,
  RenderedNotes,
  Segment,
  Skill,
  SkillRun,
  Suggestion,
  Template,
} from "../lib/types";
import ChatAside from "./ChatAside";
import RenderedPane from "./RenderedPane";
import TranscriptPane from "./TranscriptPane";
import { ChatIcon, MicIcon, StopIcon } from "./Icons";

interface Props {
  meetingId: string;
  folders: Folder[];
  isRecording: boolean;
  /** A different meeting holds the recorder — this one can't start. */
  anotherRecording: boolean;
  chatOpen: boolean;
  onToggleChat: () => void;
  onChanged: () => void;
}

type Tab = "rendered" | "raw" | "transcript";

const WAVE_BARS = 14;
// Fixed offsets, not random: the design's staggered look, but stable across
// renders so the bars don't resync every time state changes.
const WAVE_DELAYS = [0, 0.08, 0.31, 0.17, 0.44, 0.22, 0.51, 0.12, 0.37, 0.6, 0.05, 0.28, 0.47, 0.19];

export default function MeetingView(props: Props) {
  const { meetingId } = props;

  const [meeting, setMeeting] = useState<Meeting | null>(null);
  const [templates, setTemplates] = useState<Template[]>([]);
  const [segments, setSegments] = useState<Segment[]>([]);
  const [notes, setNotes] = useState<MeetingNotes>({ rendered: null, user: "" });
  const [suggestions, setSuggestions] = useState<Suggestion[]>([]);
  const [allSkills, setAllSkills] = useState<Skill[]>([]);
  const [attached, setAttached] = useState<Skill[]>([]);
  const [runs, setRuns] = useState<SkillRun[]>([]);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [level, setLevel] = useState(0);
  const [elapsed, setElapsed] = useState(0);
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [tab, setTab] = useState<Tab>("rendered");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let alive = true;
    void (async () => {
      try {
        const [m, t, s, n, g, sk, att, r] = await Promise.all([
          api.getMeeting(meetingId),
          api.listTemplates(),
          api.listSegments(meetingId),
          api.getNotes(meetingId),
          api.listSuggestions(meetingId),
          api.listSkills(),
          api.meetingSkills(meetingId),
          api.skillRuns(meetingId),
        ]);
        if (!alive) return;
        setMeeting(m);
        setTemplates(t);
        setSegments(s);
        setNotes(n);
        setSuggestions(g);
        setAllSkills(sk);
        setAttached(att);
        setRuns(r);
        // A meeting with no write-up yet opens on the notes you can actually
        // type into, rather than an empty rendered page.
        if (!n.rendered) setTab(m.status === "recording" ? "raw" : "rendered");
      } catch (e) {
        if (alive) setError(api.errorMessage(e));
      }
    })();
    return () => {
      alive = false;
    };
  }, [meetingId]);

  // Every handler filters by meeting id: events broadcast to the whole window,
  // and a meeting recording in the background must not write into this view.
  useEffect(
    () =>
      subscribe({
        onSegment: (s) => s.meetingId === meetingId && setSegments((p) => [...p, s]),
        onSuggestion: (s) => s.meetingId === meetingId && setSuggestions((p) => [...p, s]),
        onLevels: (l) => {
          if (l.meetingId === meetingId) setLevel(Math.max(l.mic, l.system));
        },
        onNotes: (n) => {
          if (n.meetingId !== meetingId) return;
          setNotes((p) => ({ ...p, rendered: n.notes }));
          setTab("rendered");
        },
        onRuns: (e) => {
          if (e.meetingId === meetingId) setRuns(e.runs);
        },
        onStatus: (e) => {
          if (e.meetingId !== meetingId) return;
          setStatus(e.message ?? null);
          setMeeting((p) => (p ? { ...p, status: e.status as Meeting["status"] } : p));
          if (e.status === "done") {
            void api.getMeeting(meetingId).then(setMeeting).catch(() => {});
            void api.getNotes(meetingId).then(setNotes).catch(() => {});
          }
        },
      }),
    [meetingId],
  );

  // Recording timer. Anchored to the meeting's own start time so reopening the
  // window mid-meeting shows the true elapsed time, not time-since-mounted.
  useEffect(() => {
    if (!props.isRecording) {
      setElapsed(0);
      return;
    }
    const started = meeting?.startedAt ? new Date(meeting.startedAt).getTime() : Date.now();
    const tick = () => setElapsed((Date.now() - started) / 1000);
    tick();
    const id = window.setInterval(tick, 1000);
    return () => window.clearInterval(id);
  }, [props.isRecording, meeting?.startedAt]);

  const patch = async (p: Parameters<typeof api.updateMeeting>[1]) => {
    if (!meeting) return;
    setMeeting({ ...meeting, ...p } as Meeting);
    try {
      await api.updateMeeting(meetingId, p);
      props.onChanged();
    } catch (e) {
      setError(api.errorMessage(e));
    }
  };

  const attach = async (skillId: string) => {
    setPickerOpen(false);
    try {
      await api.attachSkill(meetingId, skillId);
      setAttached(await api.meetingSkills(meetingId));
    } catch (e) {
      setError(api.errorMessage(e));
    }
  };

  const detach = async (skillId: string) => {
    setAttached((prev) => prev.filter((s) => s.id !== skillId));
    try {
      await api.detachSkill(meetingId, skillId);
    } catch (e) {
      setError(api.errorMessage(e));
      setAttached(await api.meetingSkills(meetingId));
    }
  };

  const toggleRecording = async () => {
    setError(null);
    setBusy(true);
    try {
      if (props.isRecording) await api.stopRecording(meetingId);
      else {
        await api.startRecording(meetingId);
        setTab("raw");
      }
    } catch (e) {
      setError(api.errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  if (!meeting) {
    return (
      <section className="stage">
        <div className="blank">
          <p>{error ?? "Loading…"}</p>
        </div>
      </section>
    );
  }

  const folderName = props.folders.find((f) => f.id === meeting.folderId)?.name;
  const duration = formatDuration(
    meeting.startedAt && meeting.endedAt
      ? (new Date(meeting.endedAt).getTime() - new Date(meeting.startedAt).getTime()) / 1000
      : null,
  );

  // RMS is small even for loud speech; scale it into a visible amplitude and
  // hold a floor so the bars read as "listening" rather than "dead".
  const amp = Math.min(1, Math.max(0.12, level * 5));

  const attachedIds = new Set(attached.map((s) => s.id));
  const pickable = allSkills.filter((s) => !attachedIds.has(s.id));

  return (
    <>
      <section className="stage">
        <div className="stage-head">
          <div className="stage-head-row">
            <div className="stage-head-main">
              <input
                className="stage-title"
                value={meeting.title}
                placeholder="Untitled meeting"
                onChange={(e) => setMeeting({ ...meeting, title: e.target.value })}
                onBlur={(e) => void patch({ title: e.target.value })}
              />

              <div className="stage-meta">
                <select
                  className="input"
                  value={meeting.folderId ?? ""}
                  onChange={(e) => void patch({ folderId: e.target.value || null })}
                  title="Folder"
                >
                  <option value="">No folder</option>
                  {props.folders.map((f) => (
                    <option key={f.id} value={f.id}>
                      {f.name}
                    </option>
                  ))}
                </select>

                <span>
                  {formatWhen(meeting.startedAt ?? meeting.createdAt)}
                  {duration && ` · ${duration}`}
                </span>

                <select
                  className="input"
                  value={meeting.templateId ?? ""}
                  onChange={(e) => void patch({ templateId: e.target.value || null })}
                  title="Write-up style"
                >
                  <option value="">General meeting</option>
                  {templates.map((t) => (
                    <option key={t.id} value={t.id}>
                      {t.name}
                    </option>
                  ))}
                </select>

                {status && <span className="tag tag-accent-2">{status}</span>}
                {folderName === undefined && meeting.folderId && (
                  <span className="tag tag-neutral">Unfiled</span>
                )}
              </div>
            </div>

            {!props.chatOpen && (
              <button className="btn btn-secondary" onClick={props.onToggleChat}>
                <ChatIcon />
                Ask
              </button>
            )}
          </div>

          {props.isRecording ? (
            <div className="rec-bar">
              <span className="rec-dot" />
              <span className="rec-timer">{formatClock(elapsed)}</span>
              <div className="wave">
                {Array.from({ length: WAVE_BARS }, (_, i) => (
                  <span
                    key={i}
                    className="wave-bar"
                    style={
                      {
                        animationDelay: `${WAVE_DELAYS[i]}s`,
                        "--amp": amp,
                      } as React.CSSProperties
                    }
                  />
                ))}
              </div>
              <button className="btn btn-primary" onClick={toggleRecording} disabled={busy}>
                <StopIcon />
                Stop &amp; render
              </button>
            </div>
          ) : (
            <div className="idle-bar">
              <button
                className="btn btn-secondary"
                onClick={toggleRecording}
                disabled={busy || props.anotherRecording}
                title={
                  props.anotherRecording
                    ? "Another meeting is recording. Stop that one first."
                    : undefined
                }
              >
                <MicIcon />
                {segments.length > 0 ? "Record more" : "Start recording"}
              </button>
              <span>
                {props.anotherRecording
                  ? "Another meeting is recording."
                  : "Audio stays on device. Type alongside it — the model fills the gaps afterwards."}
              </span>
            </div>
          )}

          <div className="skills-bar">
            <div className="row">
              <span className="skills-label">Skills in this meeting</span>
              {attached.map((s) => (
                <span className="tag tag-accent skill-chip" key={s.id}>
                  {s.name}
                  <span className="skill-chip-when">{s.kind === "live" ? "live" : "after"}</span>
                  <button
                    aria-label={`Remove ${s.name}`}
                    onClick={() => void detach(s.id)}
                    className="skill-chip-x"
                  >
                    ×
                  </button>
                </span>
              ))}
              {pickable.length > 0 && (
                <button
                  className="tag tag-outline"
                  style={{ cursor: "pointer" }}
                  onClick={() => setPickerOpen((o) => !o)}
                >
                  + Add skill
                </button>
              )}
            </div>

            {pickerOpen && (
              <div className="skill-picker elev-sm">
                {pickable.map((s) => (
                  <button key={s.id} className="skill-pick" onClick={() => void attach(s.id)}>
                    <span className="grow">{s.name}</span>
                    <span className="tag tag-neutral">
                      {s.kind === "live" ? "During" : "After"}
                    </span>
                  </button>
                ))}
              </div>
            )}
          </div>

          <div className="seg" style={{ alignSelf: "flex-start" }}>
            {(
              [
                ["rendered", "Rendered"],
                ["raw", "My notes"],
                ["transcript", `Transcript${segments.length ? ` (${segments.length})` : ""}`],
              ] as [Tab, string][]
            ).map(([key, label]) => (
              <label className="seg-opt" key={key}>
                <input
                  type="radio"
                  name="amble-tab"
                  checked={tab === key}
                  onChange={() => setTab(key)}
                />
                {label}
              </label>
            ))}
          </div>
        </div>

        {error && (
          <div className="banner banner-error">
            <span>{error}</span>
            <button className="btn btn-ghost" onClick={() => setError(null)}>
              Dismiss
            </button>
          </div>
        )}

        <div className="doc">
          {tab === "rendered" && (
            <RenderedPane
              meetingId={meetingId}
              notes={notes.rendered}
              runs={runs}
              hasPostSkills={attached.some((s) => s.kind === "post")}
              hasTranscript={segments.length > 0}
              onNotes={(n: RenderedNotes) => setNotes((p) => ({ ...p, rendered: n }))}
              onError={setError}
            />
          )}

          {tab === "raw" && (
            <RawNotes
              meetingId={meetingId}
              value={notes.user}
              onChange={(v) => setNotes((p) => ({ ...p, user: v }))}
              onError={setError}
            />
          )}

          {tab === "transcript" && (
            <TranscriptPane segments={segments} isRecording={props.isRecording} />
          )}
        </div>
      </section>

      {props.chatOpen && (
        <ChatAside
          meetingId={meetingId}
          suggestions={suggestions}
          isRecording={props.isRecording}
          hasTranscript={segments.length > 0}
          onClose={props.onToggleChat}
          onError={setError}
        />
      )}
    </>
  );
}

/** The "My notes" tab: a textarea that autosaves, plus image attachments you can
 *  reference from the text. Images are stored on disk and surfaced over MCP. */
function RawNotes({
  meetingId,
  value,
  onChange,
  onError,
}: {
  meetingId: string;
  value: string;
  onChange: (v: string) => void;
  onError: (msg: string) => void;
}) {
  const timer = useRef<number | null>(null);
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);
  const fileInput = useRef<HTMLInputElement | null>(null);
  const [saving, setSaving] = useState(false);
  const [images, setImages] = useState<NoteImage[]>([]);
  // Data URLs for rendering, loaded on demand and cached by image id.
  const [thumbs, setThumbs] = useState<Record<string, string>>({});
  const [uploading, setUploading] = useState(false);
  const [dragging, setDragging] = useState(false);

  useEffect(() => {
    let alive = true;
    void (async () => {
      try {
        const imgs = await api.listNoteImages(meetingId);
        if (!alive) return;
        setImages(imgs);
        for (const img of imgs) void loadThumb(img.id);
      } catch (e) {
        if (alive) onError(api.errorMessage(e));
      }
    })();
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [meetingId]);

  const loadThumb = async (id: string) => {
    try {
      const url = await api.noteImageData(id);
      setThumbs((p) => ({ ...p, [id]: url }));
    } catch {
      // A missing file just leaves a placeholder; not worth an error banner.
    }
  };

  const edit = (v: string) => {
    onChange(v);
    if (timer.current) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      setSaving(true);
      api
        .saveNote(meetingId, v)
        .catch((e) => onError(api.errorMessage(e)))
        .finally(() => setSaving(false));
    }, 700);
  };

  useEffect(
    () => () => {
      if (timer.current) window.clearTimeout(timer.current);
    },
    [],
  );

  const fileToBase64 = (file: File) =>
    new Promise<string>((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(String(reader.result)); // a data: URL
      reader.onerror = () => reject(reader.error);
      reader.readAsDataURL(file);
    });

  const addFiles = async (files: File[]) => {
    const pics = files.filter((f) => f.type.startsWith("image/"));
    if (pics.length === 0) return;
    setUploading(true);
    try {
      for (const file of pics) {
        const data = await fileToBase64(file);
        const name = file.name?.trim() || "Pasted image";
        const img = await api.addNoteImage(meetingId, name, file.type, data);
        setImages((p) => [...p, img]);
        void loadThumb(img.id);
      }
    } catch (e) {
      onError(api.errorMessage(e));
    } finally {
      setUploading(false);
    }
  };

  /** Drop the image's markdown reference in at the cursor (or the end). */
  const insertReference = (img: NoteImage) => {
    const md = `![${img.name}](${img.reference})`;
    const el = textareaRef.current;
    const at = el ? el.selectionStart : value.length;
    const needsNlBefore = at > 0 && value[at - 1] !== "\n";
    const snippet = `${needsNlBefore ? "\n" : ""}${md}\n`;
    const next = value.slice(0, at) + snippet + value.slice(at);
    edit(next);
    // Restore focus and place the caret after what we inserted.
    requestAnimationFrame(() => {
      if (!el) return;
      el.focus();
      const pos = at + snippet.length;
      el.setSelectionRange(pos, pos);
    });
  };

  const rename = async (img: NoteImage, name: string) => {
    const trimmed = name.trim();
    setImages((p) => p.map((i) => (i.id === img.id ? { ...i, name: trimmed || i.name } : i)));
    if (!trimmed || trimmed === img.name) return;
    try {
      await api.renameNoteImage(img.id, trimmed);
    } catch (e) {
      onError(api.errorMessage(e));
    }
  };

  const remove = async (img: NoteImage) => {
    setImages((p) => p.filter((i) => i.id !== img.id));
    setThumbs((p) => {
      const next = { ...p };
      delete next[img.id];
      return next;
    });
    try {
      await api.deleteNoteImage(img.id);
    } catch (e) {
      onError(api.errorMessage(e));
      setImages(await api.listNoteImages(meetingId));
    }
  };

  return (
    <div className="doc-section">
      <div className="doc-hint">
        {saving
          ? "Saving…"
          : uploading
            ? "Adding image…"
            : "Yours, untouched. The model reads these but never overwrites them."}
      </div>

      <textarea
        ref={textareaRef}
        className="input"
        style={{ minHeight: 300, lineHeight: 1.7, fontSize: 15 }}
        value={value}
        placeholder="Type as you go. Whatever you write here is treated as higher-signal than the transcript when the write-up is generated. Paste or drop an image to attach it."
        onChange={(e) => edit(e.target.value)}
        onPaste={(e) => {
          const files = Array.from(e.clipboardData.files);
          if (files.some((f) => f.type.startsWith("image/"))) {
            e.preventDefault();
            void addFiles(files);
          }
        }}
        onDragOver={(e) => {
          e.preventDefault();
          setDragging(true);
        }}
        onDragLeave={() => setDragging(false)}
        onDrop={(e) => {
          e.preventDefault();
          setDragging(false);
          void addFiles(Array.from(e.dataTransfer.files));
        }}
      />

      <div className="images-bar">
        <div className="row" style={{ justifyContent: "space-between" }}>
          <span className="skills-label">
            Images {images.length > 0 && `(${images.length})`}
          </span>
          <button className="btn btn-ghost" onClick={() => fileInput.current?.click()}>
            + Add image
          </button>
          <input
            ref={fileInput}
            type="file"
            accept="image/*"
            multiple
            hidden
            onChange={(e) => {
              void addFiles(Array.from(e.target.files ?? []));
              e.target.value = "";
            }}
          />
        </div>

        {images.length === 0 ? (
          <p className="doc-hint" style={{ marginTop: 4 }}>
            {dragging
              ? "Drop to attach."
              : "Paste, drop, or add an image. Insert its reference to point at it from your notes — Claude can fetch it over MCP."}
          </p>
        ) : (
          <div className="image-grid">
            {images.map((img) => (
              <figure className="image-card elev-sm" key={img.id}>
                {thumbs[img.id] ? (
                  <img className="image-thumb" src={thumbs[img.id]} alt={img.name} />
                ) : (
                  <div className="image-thumb image-thumb-empty">…</div>
                )}
                <figcaption>
                  <input
                    className="input image-name"
                    value={img.name}
                    aria-label="Image name"
                    onChange={(e) =>
                      setImages((p) =>
                        p.map((i) => (i.id === img.id ? { ...i, name: e.target.value } : i)),
                      )
                    }
                    onBlur={(e) => void rename(img, e.target.value)}
                  />
                  <div className="row image-actions">
                    <button
                      className="btn btn-ghost"
                      title="Insert a reference to this image into your notes"
                      onClick={() => insertReference(img)}
                    >
                      Insert
                    </button>
                    <button
                      className="btn btn-ghost"
                      title="Copy the amble://image reference"
                      onClick={() => void navigator.clipboard?.writeText(img.reference)}
                    >
                      Copy ref
                    </button>
                    <button
                      className="btn btn-ghost image-remove"
                      aria-label={`Remove ${img.name}`}
                      onClick={() => void remove(img)}
                    >
                      ×
                    </button>
                  </div>
                </figcaption>
              </figure>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
