import { useEffect, useState } from "react";
import * as api from "../lib/api";
import type { Skill, SkillKind } from "../lib/types";
import { PlusIcon, TrashIcon } from "./Icons";

interface Props {
  onError: (msg: string) => void;
}

type Draft = {
  id: string | null;
  name: string;
  kind: SkillKind;
  target: string;
  prompt: string;
};

const BLANK: Draft = { id: null, name: "", kind: "live", target: "", prompt: "" };

/**
 * Skills are reusable prompts that attach to meetings. Live ones steer the model
 * while you're still talking; post ones run once the recording stops.
 */
export default function SkillsView({ onError }: Props) {
  const [skills, setSkills] = useState<Skill[]>([]);
  const [draft, setDraft] = useState<Draft | null>(null);

  const reload = () =>
    api
      .listSkills()
      .then(setSkills)
      .catch((e) => onError(api.errorMessage(e)));

  useEffect(() => {
    void reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const save = async () => {
    if (!draft) return;
    if (!draft.name.trim() || !draft.prompt.trim()) {
      onError("A skill needs a name and a prompt.");
      return;
    }
    try {
      if (draft.id) {
        await api.updateSkill(draft.id, {
          name: draft.name,
          kind: draft.kind,
          target: draft.target,
          prompt: draft.prompt,
        });
      } else {
        await api.createSkill({
          name: draft.name,
          kind: draft.kind,
          target: draft.target,
          prompt: draft.prompt,
        });
      }
      setDraft(null);
      await reload();
    } catch (e) {
      onError(api.errorMessage(e));
    }
  };

  const toggle = async (s: Skill) => {
    // Optimistic: a checkbox that waits on a round trip feels broken.
    setSkills((prev) => prev.map((x) => (x.id === s.id ? { ...x, enabled: !x.enabled } : x)));
    try {
      await api.updateSkill(s.id, { enabled: !s.enabled });
    } catch (e) {
      onError(api.errorMessage(e));
      await reload();
    }
  };

  const remove = async (s: Skill) => {
    if (!window.confirm(`Delete “${s.name}”?`)) return;
    try {
      await api.deleteSkill(s.id);
      await reload();
    } catch (e) {
      onError(api.errorMessage(e));
    }
  };

  const live = skills.filter((s) => s.kind === "live");
  const post = skills.filter((s) => s.kind === "post");

  return (
    <section className="screen">
      <div className="screen-inner">
        <div className="screen-head">
          <div className="grow">
            <h2>Skills &amp; prompts</h2>
            <p className="screen-lede">
              Live skills steer the model while you are still talking. After-skills run once you
              stop, and can push the result into a connected system.
            </p>
          </div>
          <button className="btn btn-primary" onClick={() => setDraft({ ...BLANK })}>
            <PlusIcon />
            New skill
          </button>
        </div>

        {draft && (
          <div className="card elev-md" style={{ gap: "var(--space-4)" }}>
            <div className="card-kicker">Skill</div>

            <div className="field">
              <label>Name</label>
              <input
                className="input"
                value={draft.name}
                placeholder="e.g. Board update"
                onChange={(e) => setDraft({ ...draft, name: e.target.value })}
              />
            </div>

            <div className="field">
              <label>Runs</label>
              <div className="seg">
                {(
                  [
                    ["live", "During the meeting"],
                    ["post", "After the meeting"],
                  ] as [SkillKind, string][]
                ).map(([k, label]) => (
                  <label className="seg-opt" key={k}>
                    <input
                      type="radio"
                      name="amble-kind"
                      checked={draft.kind === k}
                      onChange={() => setDraft({ ...draft, kind: k })}
                    />
                    {label}
                  </label>
                ))}
              </div>
            </div>

            {draft.kind === "post" && (
              <div className="field">
                <label>Pushes to</label>
                <input
                  className="input"
                  value={draft.target}
                  placeholder="Linear · MCP, Gmail draft, or leave blank for none"
                  onChange={(e) => setDraft({ ...draft, target: e.target.value })}
                />
                <span className="small muted">
                  Recorded on the result. Delivery needs an MCP connection, which isn't wired up
                  yet — the output is produced and kept with the meeting.
                </span>
              </div>
            )}

            <div className="field">
              <label>Prompt</label>
              <textarea
                className="input"
                style={{ minHeight: 130 }}
                value={draft.prompt}
                placeholder="Tell the model what to listen for and what to produce."
                onChange={(e) => setDraft({ ...draft, prompt: e.target.value })}
              />
            </div>

            <div className="row">
              <button className="btn btn-primary" onClick={save}>
                Save skill
              </button>
              <button className="btn btn-secondary" onClick={() => setDraft(null)}>
                Cancel
              </button>
            </div>
          </div>
        )}

        <SkillGroup
          title="During the meeting"
          hint="Folded into the prompt behind live suggestions."
          skills={live}
          onEdit={setDraft}
          onToggle={toggle}
          onDelete={remove}
        />

        <SkillGroup
          title="After the meeting"
          hint="Run once against the transcript and the rendered note."
          skills={post}
          onEdit={setDraft}
          onToggle={toggle}
          onDelete={remove}
        />

        <p className="small muted">
          Enabled skills attach to every new meeting automatically. You can drop or add them per
          meeting from the note itself.
        </p>
      </div>
    </section>
  );
}

function SkillGroup({
  title,
  hint,
  skills,
  onEdit,
  onToggle,
  onDelete,
}: {
  title: string;
  hint: string;
  skills: Skill[];
  onEdit: (d: Draft) => void;
  onToggle: (s: Skill) => void;
  onDelete: (s: Skill) => void;
}) {
  return (
    <div className="doc-section">
      <h3>{title}</h3>
      <p className="small muted" style={{ margin: 0 }}>
        {hint}
      </p>

      {skills.length === 0 && <p className="small muted">Nothing here yet.</p>}

      {skills.map((s) => (
        <div className="card" style={{ gap: 10 }} key={s.id}>
          <div className="row">
            <span className="skill-name grow">{s.name}</span>
            {s.target && <span className="tag tag-accent-2">{s.target}</span>}
            <button
              className="btn btn-ghost"
              onClick={() =>
                onEdit({
                  id: s.id,
                  name: s.name,
                  kind: s.kind,
                  target: s.target,
                  prompt: s.prompt,
                })
              }
            >
              Edit
            </button>
            <button className="btn btn-ghost btn-icon" title="Delete" onClick={() => onDelete(s)}>
              <TrashIcon />
            </button>
            <input
              type="checkbox"
              checked={s.enabled}
              onChange={() => onToggle(s)}
              title={s.enabled ? "On by default for new meetings" : "Off by default"}
              style={{ width: 18, height: 18, accentColor: "var(--color-accent)" }}
            />
          </div>
          <div className="skill-prompt">{s.prompt}</div>
        </div>
      ))}
    </div>
  );
}
