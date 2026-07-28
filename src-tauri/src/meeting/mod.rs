//! The recording loop: audio in, transcript and suggestions out.
//!
//! Two background tasks run for the life of a meeting.
//!
//! * **Transcription** polls the capture buffers, hands complete chunks to the
//!   STT provider, and writes each result to the database and the UI.
//! * **Suggestions** wakes on an interval and asks the chat model whether
//!   anything useful can be said about the conversation so far.
//!
//! Both watch the same stop flag. On stop, transcription drains whatever audio
//! is left, then produces the write-up.

pub mod events;

use crate::audio::chunker::{Chunk, Chunker};
use crate::audio::{wav, AudioBuffers, Recorder, Source, CAPTURE_SAMPLE_RATE};
use crate::db::Segment;
use crate::error::{AppError, Result};
use crate::llm::ChatMsg;
use crate::prompts;
use crate::state::{ActiveMeeting, AppState};
use events::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::AppHandle;

/// How often the transcription loop drains the capture buffers. Frequent enough
/// that level meters feel live, cheap enough to be invisible.
const POLL_INTERVAL: Duration = Duration::from_millis(400);

/// Begin recording into an existing meeting row.
pub fn start(app: AppHandle, state: Arc<AppState>, meeting_id: &str) -> Result<()> {
    if let Some(existing) = state.active_meeting_id() {
        return Err(AppError::Other(format!(
            "Already recording meeting {existing}. Stop it before starting another."
        )));
    }

    let meeting = state.require_meeting(meeting_id)?;
    let settings = state.settings_snapshot();

    let buffers = AudioBuffers::new();
    let recorder = Recorder::start(&settings.capture, Arc::clone(&buffers))?;
    let stop = Arc::new(AtomicBool::new(false));

    let audio_dir = state.paths.recordings.join(&meeting.id);
    if settings.capture.keep_audio {
        std::fs::create_dir_all(&audio_dir)?;
    }
    state
        .db
        .mark_meeting_started(&meeting.id, &audio_dir.to_string_lossy())?;

    *state.active.lock() = Some(ActiveMeeting {
        meeting_id: meeting.id.clone(),
        recorder: Some(recorder),
        buffers: Arc::clone(&buffers),
        stop: Arc::clone(&stop),
    });

    emit_status(&app, &meeting.id, "recording", None);

    tokio::spawn(transcription_loop(
        app.clone(),
        Arc::clone(&state),
        meeting.id.clone(),
        Arc::clone(&buffers),
        Arc::clone(&stop),
    ));

    if settings.capture.suggestions_enabled {
        tokio::spawn(suggestion_loop(
            app,
            state,
            meeting.id.clone(),
            stop,
        ));
    }

    Ok(())
}

/// Stop recording. Returns immediately — the transcription loop finishes the
/// remaining audio and generates notes in the background, reporting progress
/// through status events.
pub fn stop(app: &AppHandle, state: &AppState, meeting_id: &str) -> Result<()> {
    let active = state.active.lock().take();

    let Some(mut active) = active else {
        return Err(AppError::Other("Nothing is being recorded.".into()));
    };

    if active.meeting_id != meeting_id {
        // Put it back — the caller asked about a different meeting.
        let id = active.meeting_id.clone();
        *state.active.lock() = Some(active);
        return Err(AppError::Other(format!("Meeting {id} is the one recording.")));
    }

    // Signal the loops first, then tear down the capture stream. The loop still
    // owns the buffers, so audio captured up to this instant survives and gets
    // transcribed during the flush.
    active.stop.store(true, Ordering::SeqCst);
    if let Some(recorder) = active.recorder.take() {
        recorder.stop()?;
    }

    state.db.update_meeting_status(meeting_id, "processing")?;
    emit_status(app, meeting_id, "processing", Some("Finishing transcription…"));

    Ok(())
}

/// Poll captured audio, cut it into chunks, transcribe, persist, emit.
async fn transcription_loop(
    app: AppHandle,
    state: Arc<AppState>,
    meeting_id: String,
    buffers: Arc<AudioBuffers>,
    stop: Arc<AtomicBool>,
) {
    let settings = state.settings_snapshot();
    let cap = &settings.capture;
    let separately = cap.transcribe_separately;

    let mut mic = Chunker::new(
        CAPTURE_SAMPLE_RATE,
        cap.chunk_min_secs,
        cap.chunk_max_secs,
        cap.silence_rms,
    );
    let mut system = Chunker::new(
        CAPTURE_SAMPLE_RATE,
        cap.chunk_min_secs,
        cap.chunk_max_secs,
        cap.silence_rms,
    );

    loop {
        let stopping = stop.load(Ordering::SeqCst);

        let mic_audio = buffers.drain(Source::Mic);
        let system_audio = buffers.drain(Source::System);

        if separately {
            mic.push(&mic_audio);
            system.push(&system_audio);
        } else {
            // One combined track: cheaper, at the cost of speaker attribution.
            mic.push(&wav::mix(&mic_audio, &system_audio));
        }

        let (mic_level, system_level) = buffers.levels();
        emit_levels(&app, &meeting_id, mic_level, system_level);

        // Collect everything ready this tick, including the final flush.
        let mut ready: Vec<(Source, Chunk)> = Vec::new();
        while let Some(c) = mic.take_ready() {
            ready.push((Source::Mic, c));
        }
        if separately {
            while let Some(c) = system.take_ready() {
                ready.push((Source::System, c));
            }
        }
        if stopping {
            if let Some(c) = mic.flush() {
                ready.push((Source::Mic, c));
            }
            if separately {
                if let Some(c) = system.flush() {
                    ready.push((Source::System, c));
                }
            }
        }

        for (source, chunk) in ready {
            // Transcribing silence buys nothing but hallucinated filler.
            if chunk.is_silent(cap.silence_rms) {
                continue;
            }
            let label = if separately { source.as_str() } else { "mixed" };
            if let Err(e) =
                transcribe_and_store(&app, &state, &meeting_id, label, &chunk, cap.keep_audio).await
            {
                tracing::warn!(error = %e, "chunk transcription failed");
                emit_status(&app, &meeting_id, "recording", Some(&format!("Transcription error: {e}")));
            }
        }

        if stopping {
            break;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }

    finalize(&app, &state, &meeting_id).await;
}

async fn transcribe_and_store(
    app: &AppHandle,
    state: &Arc<AppState>,
    meeting_id: &str,
    source: &str,
    chunk: &Chunk,
    keep_audio: bool,
) -> Result<()> {
    let wav_bytes = wav::prepare_for_stt(&chunk.samples, CAPTURE_SAMPLE_RATE)?;

    if keep_audio {
        let dir = state.paths.recordings.join(meeting_id);
        std::fs::create_dir_all(&dir)?;
        let name = format!("{source}-{:08}.wav", chunk.start_ms);
        std::fs::write(dir.join(name), &wav_bytes)?;
    }

    let text = state
        .stt()?
        .transcribe_wav(wav_bytes, &format!("{source}-{}.wav", chunk.start_ms))
        .await?;

    if text.trim().is_empty() {
        return Ok(());
    }

    let segment = state
        .db
        .add_segment(meeting_id, source, &text, chunk.start_ms, chunk.end_ms)?;
    emit_segment(app, &segment);
    Ok(())
}

/// After the last chunk: title the meeting, write it up, mark it done.
///
/// Each step is best-effort. A model outage at the end of a call must not cost
/// the user the transcript they just recorded, so failures are reported and the
/// meeting still closes cleanly.
async fn finalize(app: &AppHandle, state: &Arc<AppState>, meeting_id: &str) {
    let settings = state.settings_snapshot();

    let has_transcript = state
        .db
        .list_segments(meeting_id)
        .map(|s| !s.is_empty())
        .unwrap_or(false);

    if has_transcript && settings.capture.auto_generate_notes {
        emit_status(app, meeting_id, "processing", Some("Writing up your notes…"));
        if let Err(e) = generate_notes(app, state, meeting_id).await {
            tracing::warn!(error = %e, "note generation failed");
            emit_status(app, meeting_id, "processing", Some(&format!("Notes failed: {e}")));
        }
    }

    if has_transcript && settings.capture.auto_title {
        if let Err(e) = auto_title(app, state, meeting_id).await {
            tracing::warn!(error = %e, "auto-title failed");
        }
    }

    let _ = state.db.mark_meeting_ended(meeting_id, "done");
    emit_status(app, meeting_id, "done", None);
    tracing::info!(meeting_id, "meeting finalized");
}

/// Generate the write-up using the meeting's chosen template.
pub async fn generate_notes(app: &AppHandle, state: &Arc<AppState>, meeting_id: &str) -> Result<String> {
    let meeting = state.require_meeting(meeting_id)?;
    let transcript = state.db.transcript_text(meeting_id)?;

    if transcript.trim().is_empty() {
        return Err(AppError::Other(
            "There is no transcript to write up yet.".into(),
        ));
    }

    let template = match &meeting.template_id {
        Some(id) => state.db.get_template(id)?.map(|t| t.prompt),
        None => None,
    }
    .unwrap_or_else(|| prompts::BUILTIN_TEMPLATES[0].1.to_string());

    let user_notes = state
        .db
        .get_note(meeting_id, "user")?
        .map(|n| n.content)
        .unwrap_or_default();

    let mut user_content = format!("Requested format:\n{template}\n\n---\n\nTranscript:\n{transcript}");
    if !user_notes.trim().is_empty() {
        // The user's own notes carry more signal than the transcript — they wrote
        // down what mattered to them, in their own words.
        user_content.push_str(&format!(
            "\n\n---\n\nThe user's own notes taken during the meeting. Treat these as \
             higher-signal than the transcript and make sure everything here is reflected:\n{user_notes}"
        ));
    }

    let system = prompts::with_meeting_context(prompts::NOTES_SYSTEM, &meeting.prompt);
    let notes = state
        .llm()?
        .complete(&[ChatMsg::system(system), ChatMsg::user(user_content)])
        .await?;

    state.db.upsert_note(meeting_id, "ai", &notes)?;
    emit_notes(app, meeting_id, &notes);
    Ok(notes)
}

/// Name an untitled meeting from its content.
async fn auto_title(app: &AppHandle, state: &Arc<AppState>, meeting_id: &str) -> Result<()> {
    let meeting = state.require_meeting(meeting_id)?;
    if !is_placeholder_title(&meeting.title) {
        return Ok(()); // the user named it; leave it alone
    }

    let transcript = state.db.transcript_text(meeting_id)?;
    let excerpt: String = transcript.chars().take(4000).collect();

    let title = state
        .llm()?
        .complete(&[
            ChatMsg::system(prompts::TITLE_SYSTEM),
            ChatMsg::user(excerpt),
        ])
        .await?;

    let title = title.trim().trim_matches('"').trim();
    if title.is_empty() || title.len() > 120 {
        return Ok(());
    }

    state.db.update_meeting(meeting_id, Some(title), None, None, None)?;
    emit_meeting_updated(app, meeting_id);
    Ok(())
}

/// Titles the app generated itself, which auto-titling is free to replace.
fn is_placeholder_title(title: &str) -> bool {
    let t = title.trim();
    t.is_empty() || t == "New meeting" || t.starts_with("Meeting ")
}

/// Ask the model for live nudges on an interval.
async fn suggestion_loop(
    app: AppHandle,
    state: Arc<AppState>,
    meeting_id: String,
    stop: Arc<AtomicBool>,
) {
    let interval = Duration::from_secs(
        state
            .settings_snapshot()
            .capture
            .suggestion_interval_secs
            .max(30),
    );

    // Nothing useful to say about an empty transcript — wait out one interval
    // before the first attempt.
    let mut last_seen_segments = 0usize;

    loop {
        tokio::time::sleep(interval).await;
        if stop.load(Ordering::SeqCst) {
            break;
        }

        let segments = match state.db.list_segments(&meeting_id) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "could not read transcript for suggestions");
                continue;
            }
        };

        // Don't burn a request re-reading a transcript that hasn't moved.
        if segments.len() <= last_seen_segments {
            continue;
        }
        last_seen_segments = segments.len();

        if let Err(e) = suggest_once(&app, &state, &meeting_id, &segments).await {
            tracing::warn!(error = %e, "suggestion failed");
        }
    }
}

/// One suggestion pass. Also exposed as a command so the user can ask on demand.
pub async fn suggest_once(
    app: &AppHandle,
    state: &Arc<AppState>,
    meeting_id: &str,
    segments: &[Segment],
) -> Result<Option<String>> {
    let meeting = state.require_meeting(meeting_id)?;

    // Only the recent past matters for a live nudge, and a shorter prompt keeps
    // the round trip fast enough to still be relevant when it lands.
    let recent: Vec<Segment> = segments.iter().rev().take(40).rev().cloned().collect();
    let transcript = crate::db::render_transcript(&recent);

    if transcript.trim().is_empty() {
        return Ok(None);
    }

    let system = prompts::with_meeting_context(prompts::SUGGESTION_SYSTEM, &meeting.prompt);
    let reply = state
        .llm()?
        .complete(&[
            ChatMsg::system(system),
            ChatMsg::user(format!("Conversation so far:\n{transcript}")),
        ])
        .await?;

    let reply = reply.trim();
    // The prompt asks for exactly "NONE" when there's nothing worth saying.
    if reply.is_empty() || reply.eq_ignore_ascii_case("none") {
        return Ok(None);
    }

    let at_ms = segments.last().map(|s| s.end_ms).unwrap_or(0);
    let suggestion = state.db.add_suggestion(meeting_id, reply, at_ms)?;
    emit_suggestion(app, &suggestion);
    Ok(Some(reply.to_string()))
}

/// Answer a question about one meeting, streaming the reply to the UI.
pub async fn chat(
    app: &AppHandle,
    state: &Arc<AppState>,
    meeting_id: &str,
    question: &str,
) -> Result<String> {
    let meeting = state.require_meeting(meeting_id)?;
    let transcript = state.db.transcript_text(meeting_id)?;
    let notes = state.db.list_notes(meeting_id)?;

    let mut context = format!("Meeting: {}\n\n", meeting.title);
    if transcript.trim().is_empty() {
        context.push_str("Transcript: (nothing has been transcribed yet)\n");
    } else {
        context.push_str(&format!("Transcript:\n{transcript}\n"));
    }
    for note in &notes {
        let label = if note.kind == "ai" { "Generated notes" } else { "The user's own notes" };
        context.push_str(&format!("\n{label}:\n{}\n", note.content));
    }

    let system = prompts::with_meeting_context(prompts::CHAT_SYSTEM, &meeting.prompt);

    let mut messages = vec![ChatMsg::system(system), ChatMsg::user(context)];

    // Prior turns, so follow-up questions work. Capped because a long chat plus a
    // long transcript is the easiest way to blow a context window.
    let history = state.db.list_chat_messages(meeting_id)?;
    for m in history.iter().rev().take(20).rev() {
        messages.push(ChatMsg {
            role: m.role.clone(),
            content: m.content.clone(),
        });
    }
    messages.push(ChatMsg::user(question));

    state.db.add_chat_message(meeting_id, "user", question)?;

    let meeting_id_owned = meeting_id.to_string();
    let reply = state
        .llm()?
        .stream(&messages, |delta| {
            emit_chat_delta(app, &meeting_id_owned, delta);
        })
        .await?;

    state.db.add_chat_message(meeting_id, "assistant", &reply)?;
    emit_chat_done(app, meeting_id, &reply);
    Ok(reply)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_titles_are_replaceable_but_user_titles_are_not() {
        assert!(is_placeholder_title("New meeting"));
        assert!(is_placeholder_title("Meeting 2026-07-28 14:03"));
        assert!(is_placeholder_title("   "));
        assert!(!is_placeholder_title("Q3 budget review"));
    }
}
