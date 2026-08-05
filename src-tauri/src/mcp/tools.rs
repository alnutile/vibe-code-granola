//! The tools this app will expose over MCP.
//!
//! Defined as JSON-Schema tool definitions plus a name-and-arguments dispatcher,
//! which is exactly the shape MCP asks for. Keeping them transport-independent
//! means they are testable now and reusable later — the same `dispatch` will back
//! an MCP server, and could equally back tool-calling in meeting chat.

use crate::db::Db;
use crate::error::{AppError, Result};
use serde::Serialize;
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

/// What a tool hands back. Most tools return structured JSON, which the host sees
/// as pretty-printed text; `get_image` returns ready-made MCP content blocks so an
/// image reaches the model as an image rather than a wall of base64.
#[derive(Debug, Clone)]
pub enum ToolOutput {
    Json(Value),
    Content(Vec<Value>),
}

pub const LIST_MEETINGS: &str = "list_meetings";
pub const GET_MEETING: &str = "get_meeting";
pub const GET_TRANSCRIPT: &str = "get_transcript";
pub const SEARCH_MEETINGS: &str = "search_meetings";
pub const LIST_FOLDERS: &str = "list_folders";
pub const LIST_IMAGES: &str = "list_images";
pub const GET_IMAGE: &str = "get_image";

pub fn definitions() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: LIST_MEETINGS.into(),
            description: "List recorded meetings, newest first. Optionally filter to one folder."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "folderId": { "type": "string", "description": "Only meetings in this folder." },
                    "limit": { "type": "integer", "description": "Maximum meetings to return.", "default": 50 }
                }
            }),
        },
        ToolDef {
            name: GET_MEETING.into(),
            description: "Get one meeting's details, including its generated notes.".into(),
            input_schema: json!({
                "type": "object",
                "properties": { "meetingId": { "type": "string" } },
                "required": ["meetingId"]
            }),
        },
        ToolDef {
            name: GET_TRANSCRIPT.into(),
            description: "Get the full transcript of a meeting as timestamped, speaker-labelled text."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": { "meetingId": { "type": "string" } },
                "required": ["meetingId"]
            }),
        },
        ToolDef {
            name: SEARCH_MEETINGS.into(),
            description: "Full-text search across every transcript. Returns matching lines with \
                          the meeting they came from."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "limit": { "type": "integer", "default": 20 }
                },
                "required": ["query"]
            }),
        },
        ToolDef {
            name: LIST_FOLDERS.into(),
            description: "List folders used to organize meetings.".into(),
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        ToolDef {
            name: LIST_IMAGES.into(),
            description: "List the images the user attached to a meeting's notes. Each has a \
                          `reference` (amble://image/<id>) that appears in the note text; pass its \
                          id to `get_image` to fetch the picture itself."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": { "meetingId": { "type": "string" } },
                "required": ["meetingId"]
            }),
        },
        ToolDef {
            name: GET_IMAGE.into(),
            description: "Fetch one attached image by its id (the tail of its amble://image/<id> \
                          reference) and return it as an image the model can see."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": { "imageId": { "type": "string" } },
                "required": ["imageId"]
            }),
        },
    ]
}

/// Execute a tool call. Every tool is read-only by design: an MCP host reaching
/// into your meeting history should not be able to delete it.
pub fn dispatch(db: &Db, name: &str, args: &Value) -> Result<ToolOutput> {
    match name {
        LIST_MEETINGS => {
            let folder_id = args.get("folderId").and_then(Value::as_str);
            let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(50) as usize;
            let meetings: Vec<Value> = db
                .list_meetings(folder_id)?
                .into_iter()
                .take(limit)
                .map(|m| {
                    json!({
                        "id": m.id,
                        "title": m.title,
                        "prompt": m.prompt,
                        "status": m.status,
                        "startedAt": m.started_at,
                        "endedAt": m.ended_at,
                        "folderId": m.folder_id,
                    })
                })
                .collect();
            Ok(ToolOutput::Json(json!({ "meetings": meetings })))
        }

        GET_MEETING => {
            let id = require_str(args, "meetingId")?;
            let meeting = db
                .get_meeting(id)?
                .ok_or_else(|| AppError::NotFound(format!("meeting {id}")))?;
            // Generated notes are stored as JSON, so flatten them to prose here —
            // an MCP host wants readable notes, not this app's storage schema.
            let notes = db
                .list_notes(id)?
                .iter()
                .map(|n| {
                    let content = if n.kind == "ai" {
                        crate::notes::parse(&n.content).to_plain_text()
                    } else {
                        n.content.clone()
                    };
                    json!({ "kind": n.kind, "content": content })
                })
                .collect::<Vec<_>>();

            // Attached images, so a host reading the notes can resolve any
            // `amble://image/<id>` reference it finds without a second round-trip.
            let images = db.list_note_images(id)?.iter().map(image_meta).collect::<Vec<_>>();

            Ok(ToolOutput::Json(json!({
                "id": meeting.id,
                "title": meeting.title,
                "prompt": meeting.prompt,
                "status": meeting.status,
                "startedAt": meeting.started_at,
                "endedAt": meeting.ended_at,
                "notes": notes,
                "images": images,
            })))
        }

        GET_TRANSCRIPT => {
            let id = require_str(args, "meetingId")?;
            if db.get_meeting(id)?.is_none() {
                return Err(AppError::NotFound(format!("meeting {id}")));
            }
            Ok(ToolOutput::Json(
                json!({ "meetingId": id, "transcript": db.transcript_text(id)? }),
            ))
        }

        SEARCH_MEETINGS => {
            let query = require_str(args, "query")?;
            let limit = args.get("limit").and_then(Value::as_i64).unwrap_or(20);
            let hits: Vec<Value> = db
                .search(query, limit)?
                .into_iter()
                .map(|h| {
                    json!({
                        "meetingId": h.meeting_id,
                        "meetingTitle": h.meeting_title,
                        "text": h.text,
                        "startMs": h.start_ms,
                        "speaker": if h.source == "mic" { "You" } else { "Them" },
                    })
                })
                .collect();
            Ok(ToolOutput::Json(json!({ "results": hits })))
        }

        LIST_FOLDERS => {
            let folders: Vec<Value> = db
                .list_folders()?
                .into_iter()
                .map(|f| json!({ "id": f.id, "name": f.name, "parentId": f.parent_id }))
                .collect();
            Ok(ToolOutput::Json(json!({ "folders": folders })))
        }

        LIST_IMAGES => {
            let id = require_str(args, "meetingId")?;
            if db.get_meeting(id)?.is_none() {
                return Err(AppError::NotFound(format!("meeting {id}")));
            }
            let images: Vec<Value> =
                db.list_note_images(id)?.iter().map(image_meta).collect();
            Ok(ToolOutput::Json(json!({ "images": images })))
        }

        GET_IMAGE => {
            let id = require_str(args, "imageId")?;
            let img = db
                .get_note_image(id)?
                .ok_or_else(|| AppError::NotFound(format!("image {id}")))?;
            // Read the bytes lazily, only when a host actually asks for the
            // picture — the listing tools stay cheap.
            let bytes = std::fs::read(&img.path)?;
            Ok(ToolOutput::Content(vec![json!({
                "type": "image",
                "data": crate::util::base64_encode(&bytes),
                "mimeType": img.mime,
            })]))
        }

        other => Err(AppError::NotFound(format!("unknown tool: {other}"))),
    }
}

/// The host-facing shape of one attached image: enough to reason about and to
/// fetch, never the on-disk path.
fn image_meta(img: &crate::db::NoteImage) -> Value {
    json!({
        "id": img.id,
        "name": img.name,
        "mime": img.mime,
        "reference": img.reference,
        "sizeBytes": img.size_bytes,
        "createdAt": img.created_at,
    })
}

fn require_str<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::Other(format!("missing required argument: {key}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded() -> (Db, String) {
        let db = Db::open_in_memory().unwrap();
        let m = db
            .create_meeting("Roadmap review", "Decide Q4 priorities", None, None)
            .unwrap();
        db.add_segment(&m.id, "mic", "let's lock the roadmap", 0, 2000)
            .unwrap();
        db.add_segment(&m.id, "system", "agreed, shipping in October", 2000, 4000)
            .unwrap();
        db.upsert_note(&m.id, "ai", "## Summary\nRoadmap locked.").unwrap();
        (db, m.id)
    }

    /// Unwrap the JSON payload of a tool that returns structured data.
    fn json(out: ToolOutput) -> Value {
        match out {
            ToolOutput::Json(v) => v,
            ToolOutput::Content(_) => panic!("expected a JSON tool output"),
        }
    }

    #[test]
    fn every_definition_has_a_dispatch_arm() {
        let db = Db::open_in_memory().unwrap();
        for def in definitions() {
            let err = dispatch(&db, &def.name, &json!({"meetingId": "x", "query": "x"}));
            // Missing data is fine here; an "unknown tool" error is not.
            if let Err(AppError::NotFound(msg)) = &err {
                assert!(
                    !msg.starts_with("unknown tool"),
                    "{} is declared but not dispatched",
                    def.name
                );
            }
        }
    }

    #[test]
    fn transcript_tool_returns_speaker_labelled_text() {
        let (db, id) = seeded();
        let out = json(dispatch(&db, GET_TRANSCRIPT, &json!({ "meetingId": id })).unwrap());
        let text = out["transcript"].as_str().unwrap();
        assert!(text.contains("[00:00] You: let's lock the roadmap"));
        assert!(text.contains("[00:02] Them: agreed, shipping in October"));
    }

    #[test]
    fn search_tool_maps_capture_source_to_a_speaker_label() {
        let (db, _) = seeded();
        let out = json(dispatch(&db, SEARCH_MEETINGS, &json!({ "query": "roadmap" })).unwrap());
        let results = out["results"].as_array().unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0]["speaker"], "You");
    }

    #[test]
    fn get_meeting_includes_generated_notes() {
        let (db, id) = seeded();
        let out = json(dispatch(&db, GET_MEETING, &json!({ "meetingId": id })).unwrap());
        assert_eq!(out["notes"][0]["kind"], "ai");
    }

    #[test]
    fn list_images_returns_metadata_with_a_reference() {
        let (db, id) = seeded();
        let img = db
            .add_note_image(&id, "board.png", "image/png", "/tmp/board.png", 42)
            .unwrap();

        let out = json(dispatch(&db, LIST_IMAGES, &json!({ "meetingId": id })).unwrap());
        let images = out["images"].as_array().unwrap();
        assert_eq!(images.len(), 1);
        assert_eq!(images[0]["id"], img.id);
        assert_eq!(images[0]["reference"], format!("amble://image/{}", img.id));
        // The on-disk path is never exposed.
        assert!(images[0].get("path").is_none());
    }

    #[test]
    fn get_meeting_lists_its_attached_images() {
        let (db, id) = seeded();
        db.add_note_image(&id, "a.png", "image/png", "/tmp/a.png", 1)
            .unwrap();
        let out = json(dispatch(&db, GET_MEETING, &json!({ "meetingId": id })).unwrap());
        assert_eq!(out["images"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn get_image_returns_an_image_content_block() {
        let (db, id) = seeded();
        // Write real bytes so the tool has something to read.
        let dir = std::env::temp_dir().join(format!("amble-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pixel.png");
        std::fs::write(&path, b"\x89PNG\r\n\x1a\n").unwrap();
        let img = db
            .add_note_image(&id, "pixel.png", "image/png", &path.to_string_lossy(), 8)
            .unwrap();

        let out = dispatch(&db, GET_IMAGE, &json!({ "imageId": img.id })).unwrap();
        match out {
            ToolOutput::Content(items) => {
                assert_eq!(items[0]["type"], "image");
                assert_eq!(items[0]["mimeType"], "image/png");
                assert_eq!(
                    items[0]["data"],
                    crate::util::base64_encode(b"\x89PNG\r\n\x1a\n")
                );
            }
            ToolOutput::Json(_) => panic!("get_image should return image content"),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn get_image_on_a_missing_id_is_not_found() {
        let db = Db::open_in_memory().unwrap();
        let err = dispatch(&db, GET_IMAGE, &json!({ "imageId": "nope" })).unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    }

    #[test]
    fn a_missing_required_argument_is_reported_by_name() {
        let db = Db::open_in_memory().unwrap();
        let err = dispatch(&db, GET_TRANSCRIPT, &json!({})).unwrap_err();
        assert!(err.to_string().contains("meetingId"));
    }

    #[test]
    fn unknown_tools_are_rejected() {
        let db = Db::open_in_memory().unwrap();
        assert!(dispatch(&db, "delete_everything", &json!({})).is_err());
    }
}
