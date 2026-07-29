//! Every prompt the app sends to a model, in one file.
//!
//! Kept together deliberately: prompt wording is the thing you tune most often
//! while using this app, and hunting it across modules is miserable.

/// Note templates seeded on first launch. The user can add their own in Settings.
///
/// Every write-up shares one shape — summary, key points, action items — because
/// that is what the UI renders. A template steers *what goes in* those fields and
/// may request extra `sections` on top; it does not redefine the structure.
pub const BUILTIN_TEMPLATES: &[(&str, &str)] = &[
    (
        "General meeting",
        "Write this up for someone who missed it.\n\
         - summary: what the meeting was about and what was decided.\n\
         - points: the substance, most important first.\n\
         - actions: everything anyone committed to.\n\
         - sections: add \"Decisions\" if anything was actually settled, and \
           \"Open questions\" for anything raised but unresolved. Skip either if empty.",
    ),
    (
        "1:1",
        "Write up this 1:1, plainly and factually. Do not editorialize about the relationship.\n\
         - summary: what this conversation was really about.\n\
         - points: topics covered, and any feedback exchanged in either direction.\n\
         - actions: what each person said they would do.\n\
         - sections: add \"Follow up next time\" for threads left open.",
    ),
    (
        "Sales / discovery call",
        "Write this up from a sales perspective. Quote them directly on anything \
         about budget, timeline, or authority.\n\
         - summary: who they are, and the problem in their words.\n\
         - points: pain points, ordered by how strongly they expressed each.\n\
         - actions: agreed next steps, with dates where stated.\n\
         - sections: add \"Requirements\" (must-have vs nice-to-have, if \
           distinguishable) and \"Objections & concerns\".",
    ),
    (
        "Interview",
        "Write up this interview. Report only what was said — do not infer a \
         hiring recommendation.\n\
         - summary: the candidate's background as they described it.\n\
         - points: signals observed, each tied to concrete evidence from the \
           transcript rather than an impression.\n\
         - actions: anything either side committed to.\n\
         - sections: add \"Their questions for us\" and \"Open concerns to probe \
           in a later round\".",
    ),
    (
        "Standup",
        "Summarize this standup.\n\
         - summary: where the team is overall.\n\
         - points: one per person — what they did, what's next.\n\
         - actions: blockers, owned by whoever needs to clear them. Blockers are \
           the part worth acting on, so put them here rather than burying them in points.",
    ),
    (
        "Raw notes",
        "The user wants everything, not a summary. Preserve detail. Fix \
         transcription errors and drop filler, but do not compress the substance.\n\
         - summary: one line on what this covers.\n\
         - points: leave empty.\n\
         - actions: anything committed to.\n\
         - sections: one per topic discussed, in the order they came up, with the \
           full detail in the body.",
    ),
];

/// Skills seeded on first launch: `(name, kind, target, prompt, enabled)`.
///
/// `live` skills steer the model while the meeting is running; `post` skills run
/// once it stops. Only the ones enabled here attach to new meetings by default —
/// the rest are there to be turned on when they fit.
pub const BUILTIN_SKILLS: &[(&str, &str, &str, &str, bool)] = &[
    (
        "Client call notes",
        "live",
        "",
        "Listen for decisions, owners and dates. Flag anything said twice as a priority. \
         Keep my typed lines as the spine of the note.",
        true,
    ),
    (
        "Sponsor read",
        "live",
        "",
        "Track deliverables, placement, rates and deadlines. Separate what they asked for \
         from what I actually agreed to.",
        false,
    ),
    (
        "Interview transcript",
        "live",
        "",
        "Attribute every line to a speaker. Keep quotes verbatim — no smoothing.",
        false,
    ),
    (
        "Draft the follow-up",
        "post",
        "Gmail draft",
        "Write a five-line follow-up email in my voice: what we decided, what I owe them, \
         what I need back, and by when. No subject line, no signature.",
        true,
    ),
    (
        "Push actions to Linear",
        "post",
        "Linear · MCP",
        "For each action item, produce one issue: a title under 70 characters, the owner as \
         assignee, and a description that states the context from this meeting. Output them \
         as a numbered list, one issue per entry.",
        false,
    ),
    (
        "File to the wiki",
        "post",
        "Notion · MCP",
        "Rewrite the rendered note as a wiki page: a one-paragraph standfirst, then the \
         detail under headings. Assume the reader has no context on this meeting.",
        false,
    ),
];

/// Prepended to every request so the model knows what it is looking at.
/// Transcripts are noisy; saying so up front measurably reduces hallucinated detail.
pub const BASE_SYSTEM: &str = "You are the note-taking assistant inside a local meeting recorder. \
You work from an automatic transcript, which means it contains misheard words, missing \
punctuation, and no reliable speaker labels beyond \"You\" (the user's microphone) and \
\"Them\" (everyone else, captured from system audio).

Rules you always follow:
- Ground every statement in the transcript. If something is unclear, say it is unclear.
- Never invent names, numbers, dates, or commitments that are not present.
- Silently correct obvious transcription errors, but never change meaning to make it read better.
- Write plainly. No preamble, no \"here is your summary\", no closing offer to help.";

/// Live nudges during the meeting. Deliberately terse — this renders in a narrow
/// sidebar while the user is talking to someone, so anything long goes unread.
pub const SUGGESTION_SYSTEM: &str = "You are assisting during a live meeting, in real time.

The user gave you an intent for this meeting. Read the transcript so far and offer at most
three short suggestions that help them meet that intent right now — a question worth asking,
a thread that got dropped, a claim worth checking, a point they meant to raise.

Hard constraints:
- One line each. Under 15 words. No numbering, no preamble.
- Only suggest things that are actionable in the next minute of conversation.
- Do not summarize what was said. They were there.
- If nothing genuinely useful has come up since the last suggestions, reply with exactly: NONE";

/// The end-of-meeting write-up. The meeting's template is spliced in as guidance
/// on what belongs in each field.
///
/// JSON rather than Markdown because the UI renders numbered key points and
/// tickable action items with owners — structure that cannot be recovered from
/// prose without parsing it back out again.
pub const NOTES_SYSTEM: &str = "Write up the meeting below.

Reply with a single JSON object and nothing else — no code fence, no commentary
before or after. The schema is:

{
  \"summary\": \"2-3 sentences: what this meeting was about and what came out of it.\",
  \"points\": [\"One key point per string. Plain sentences, no bullet characters, no numbering.\"],
  \"actions\": [{ \"text\": \"What needs doing\", \"who\": \"Owner's name\" }],
  \"sections\": [{ \"heading\": \"Section name\", \"body\": \"Markdown body\" }]
}

Rules:
- Omit `who` entirely when the transcript does not make the owner clear. Never guess a name.
- `points` and `actions` are flat lists of strings/objects — do not nest markdown lists inside them.
- Leave an array empty rather than padding it with filler. An empty `actions` is a fine
  answer for a meeting where nobody committed to anything.
- Use `sections` only for what the requested format asks for beyond the fields above.
- If the transcript is too short or too garbled to support real notes, say so in `summary`
  and leave the arrays empty.";

/// Chatting with a meeting after (or during) the fact.
pub const CHAT_SYSTEM: &str = "You are answering questions about one specific meeting.

You have its transcript and any notes below. Answer only from that material. When the
transcript does not contain the answer, say so plainly — do not reason from general
knowledge and present it as something from the meeting. Quote the transcript when the
exact wording matters. Keep answers tight unless asked to expand.";

/// Auto-title for meetings the user never named.
pub const TITLE_SYSTEM: &str = "Generate a title for this meeting: 3-6 words, specific to \
what was actually discussed, no quotes, no trailing punctuation. Reply with the title alone \
and nothing else.";

/// A post-meeting skill. The skill's own prompt is supplied as the instruction;
/// this only frames it.
pub const SKILL_POST_SYSTEM: &str = "You are running one saved skill against a meeting that has \
just finished.

The instruction below is the whole job. Produce exactly what it asks for and nothing else — no
preamble, no explanation of what you did, no offer to revise. If the meeting does not contain
enough to carry out the instruction, say so in one line rather than inventing material.";

/// Assemble the system prompt for a request, folding in the meeting's own intent.
pub fn with_meeting_context(base: &str, meeting_prompt: &str) -> String {
    if meeting_prompt.trim().is_empty() {
        format!("{BASE_SYSTEM}\n\n{base}")
    } else {
        format!(
            "{BASE_SYSTEM}\n\n{base}\n\n\
             The user's stated intent for this meeting:\n{}",
            meeting_prompt.trim()
        )
    }
}

/// Fold attached live-skill prompts into an in-meeting system prompt.
///
/// Live skills are additive: several can apply at once ("track deliverables"
/// *and* "keep quotes verbatim"), so they are listed rather than merged.
pub fn with_live_skills(base: String, skills: &[(String, String)]) -> String {
    if skills.is_empty() {
        return base;
    }
    let mut out = base;
    out.push_str("\n\nSkills the user has attached to this meeting. Follow all of them:");
    for (name, prompt) in skills {
        out.push_str(&format!("\n\n- {name}: {}", prompt.trim()));
    }
    out
}
