//! The structured shape of a generated write-up.
//!
//! The UI renders a summary paragraph, numbered key points, and tickable action
//! items with owners — none of which can be driven off a markdown blob without
//! parsing prose back into structure. So the model is asked for JSON directly and
//! it is stored that way, in `notes.content` for `kind = 'ai'`.
//!
//! Models are unreliable about returning bare JSON, so [`parse`] is deliberately
//! forgiving, and falls back to treating the whole reply as a summary rather than
//! failing. Losing the structure is a worse day than losing the notes.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderedNotes {
    pub summary: String,
    #[serde(default)]
    pub points: Vec<String>,
    #[serde(default)]
    pub actions: Vec<ActionItem>,
    /// Template-specific extras — "Objections", "Signals observed", and so on.
    /// Body is markdown so a template can ask for whatever shape it needs
    /// without the schema growing a field per template.
    #[serde(default)]
    pub sections: Vec<NoteSection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionItem {
    pub text: String,
    /// Owner. `None` when the transcript doesn't say who took it on — the prompt
    /// asks the model to leave it out rather than guess.
    #[serde(default)]
    pub who: Option<String>,
    /// Ticked in the UI. Lives here rather than in its own table because it is
    /// meaningless apart from the action it belongs to.
    #[serde(default)]
    pub done: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteSection {
    pub heading: String,
    pub body: String,
}

impl RenderedNotes {
    /// Everything the model produced, as plain text. Used to give the chat and
    /// title prompts the notes as context without teaching them the schema.
    pub fn to_plain_text(&self) -> String {
        let mut out = String::new();
        if !self.summary.trim().is_empty() {
            out.push_str(self.summary.trim());
            out.push_str("\n\n");
        }
        if !self.points.is_empty() {
            out.push_str("Key points:\n");
            for p in &self.points {
                out.push_str(&format!("- {p}\n"));
            }
            out.push('\n');
        }
        if !self.actions.is_empty() {
            out.push_str("Action items:\n");
            for a in &self.actions {
                match &a.who {
                    Some(who) => out.push_str(&format!("- {} ({})\n", a.text, who)),
                    None => out.push_str(&format!("- {}\n", a.text)),
                }
            }
            out.push('\n');
        }
        for s in &self.sections {
            out.push_str(&format!("{}:\n{}\n\n", s.heading, s.body));
        }
        out.trim().to_string()
    }

    /// First line or so, for the meeting list.
    pub fn snippet(&self, max_chars: usize) -> String {
        let text = if self.summary.trim().is_empty() {
            self.points.first().cloned().unwrap_or_default()
        } else {
            self.summary.clone()
        };
        truncate_on_word(text.trim(), max_chars)
    }
}

/// Parse a model reply into notes.
///
/// Handles bare JSON, JSON in a ``` fence, and JSON with prose either side. If
/// none of that yields an object, the whole reply becomes the summary so the
/// user still sees what the model wrote.
pub fn parse(reply: &str) -> RenderedNotes {
    if let Some(json) = extract_json_object(reply) {
        if let Ok(notes) = serde_json::from_str::<RenderedNotes>(json) {
            // A JSON object that parsed but carries nothing useful is worse than
            // the raw text — treat it as a failure.
            if !notes.summary.trim().is_empty()
                || !notes.points.is_empty()
                || !notes.actions.is_empty()
                || !notes.sections.is_empty()
            {
                return notes;
            }
        }
    }

    RenderedNotes {
        summary: reply.trim().to_string(),
        ..Default::default()
    }
}

/// The widest span from the first `{` to the last `}`. Cheap, and correct for
/// every real failure mode — a fenced block, or a "Here are your notes:"
/// preamble — without needing a full JSON scanner.
fn extract_json_object(s: &str) -> Option<&str> {
    let start = s.find('{')?;
    let end = s.rfind('}')?;
    if end <= start {
        return None;
    }
    Some(&s[start..=end])
}

fn truncate_on_word(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max).collect();
    match cut.rsplit_once(' ') {
        Some((head, _)) => format!("{}…", head.trim_end_matches(['.', ',', ';'])),
        None => format!("{cut}…"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bare_json() {
        let n = parse(
            r#"{"summary":"We shipped it.","points":["one","two"],
                "actions":[{"text":"Do the thing","who":"Rae"}]}"#,
        );
        assert_eq!(n.summary, "We shipped it.");
        assert_eq!(n.points.len(), 2);
        assert_eq!(n.actions[0].who.as_deref(), Some("Rae"));
        assert!(!n.actions[0].done, "actions start unticked");
    }

    #[test]
    fn parses_json_inside_a_code_fence() {
        let n = parse("Here you go:\n```json\n{\"summary\":\"Fenced.\"}\n```\nHope that helps!");
        assert_eq!(n.summary, "Fenced.");
    }

    #[test]
    fn an_action_without_an_owner_is_allowed() {
        let n = parse(r#"{"summary":"s","actions":[{"text":"Unowned"}]}"#);
        assert_eq!(n.actions[0].who, None);
    }

    #[test]
    fn unparseable_output_becomes_the_summary() {
        let n = parse("The model just wrote prose about the meeting.");
        assert_eq!(n.summary, "The model just wrote prose about the meeting.");
        assert!(n.points.is_empty());
    }

    #[test]
    fn an_empty_json_object_falls_back_rather_than_showing_nothing() {
        let n = parse("{}");
        assert_eq!(n.summary, "{}");
    }

    #[test]
    fn unknown_fields_do_not_break_parsing() {
        let n = parse(r#"{"summary":"s","mood":"upbeat","points":["p"]}"#);
        assert_eq!(n.summary, "s");
        assert_eq!(n.points, vec!["p"]);
    }

    #[test]
    fn plain_text_rendering_covers_every_part() {
        let n = parse(
            r#"{"summary":"Sum.","points":["P1"],"actions":[{"text":"A1","who":"Dana"}],
                "sections":[{"heading":"Objections","body":"- too slow"}]}"#,
        );
        let text = n.to_plain_text();
        assert!(text.contains("Sum."));
        assert!(text.contains("- P1"));
        assert!(text.contains("- A1 (Dana)"));
        assert!(text.contains("Objections"));
        assert!(text.contains("- too slow"));
    }

    #[test]
    fn snippet_truncates_on_a_word_boundary() {
        let n = RenderedNotes {
            summary: "The team agreed to rebuild the pricing page around three plans".into(),
            ..Default::default()
        };
        let s = n.snippet(30);
        assert!(s.ends_with('…'));
        assert!(s.len() <= 34, "got {s:?}");
        assert!(!s.contains("  "));
    }

    #[test]
    fn snippet_falls_back_to_the_first_key_point() {
        let n = RenderedNotes {
            summary: "   ".into(),
            points: vec!["Cabinetry is 60% of both quotes.".into()],
            ..Default::default()
        };
        assert_eq!(n.snippet(80), "Cabinetry is 60% of both quotes.");
    }
}
