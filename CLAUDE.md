# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A macOS-only Tauri 2 desktop app (Rust backend + React/TS frontend): a local-first meeting
recorder that captures system audio *and* microphone, transcribes as it records, suggests ideas
mid-meeting, writes the meeting up on stop, and lets you chat with the result. Every model is
swappable behind an OpenAI-compatible interface, hosted or on `localhost`.

Requires **macOS 15+** (ScreenCaptureKit gained separate microphone capture in 15.0).

## Commands

```bash
npm install
npm run tauri dev                                    # run the app (Vite + cargo)
npm run tauri build                                  # .app / .dmg

cargo test --manifest-path src-tauri/Cargo.toml      # all Rust tests
cargo test --manifest-path src-tauri/Cargo.toml resample   # filter by substring
npx tsc --noEmit                                     # typecheck the frontend

RUST_LOG=vibecode_granola_lib=debug npm run tauri dev      # recording-loop internals
```

Tests are in-module `#[cfg(test)]` blocks. `Db::open_in_memory()` is a test-only constructor —
use it for anything touching the database.

## Architecture

### The recording pipeline

The whole app hangs off one data flow. Follow it before changing anything in `audio/` or `meeting/`:

```
audio/macos.rs      one SCStream, two output handlers (Audio, Microphone)
      ↓             tagged by source, downmixed to mono @ 48kHz
audio/mod.rs        AudioBuffers — two locked Vec<f32>, written from SCK dispatch queues
      ↓             drained every 400ms by the transcription loop
audio/chunker.rs    cuts at a silence frame past chunk_min_secs, forced at chunk_max_secs
      ↓
audio/wav.rs        resample 48k→16k, encode 16-bit mono WAV
      ↓
stt/                POST to the configured transcription endpoint
      ↓
db/                 transcript_segments row + FTS5 index
      ↓
meeting/events.rs   emitted to the UI
```

`meeting::transcription_loop` is the spine. It also owns the shutdown protocol: `meeting::stop`
sets the atomic flag, then tears down the recorder — the loop still holds the buffers, so audio
captured up to that instant survives, gets flushed through the chunker, and then `finalize`
generates the write-up and title.

### The mic/system separation invariant

ScreenCaptureKit tags each buffer with its origin, and that distinction is load-bearing all the
way through: `Source::{Mic,System}` → `segment.source` (`"mic"`/`"system"`/`"mixed"`) →
`db::render_transcript` labelling lines `You:`/`Them:` → the same mapping again in
`mcp::tools::dispatch`. Speaker attribution comes from this, not from a diarization model.
`capture.transcribe_separately = false` collapses both into one `"mixed"` track.

If you add a source or change a label, all four places must move together.

### Model providers

`llm/` and `stt/` both speak the OpenAI shape, which is why switching providers is base URL +
model string with no code path change. `stt/` carries exactly one branch for whisper.cpp, which
serves `/inference` instead of `/audio/transcriptions`.

Clients are constructed **per request** from `AppState::llm()` / `AppState::stt()`, never cached.
That is deliberate: a settings change takes effect on the next call with no restart and no
invalidation logic. Don't add a cache.

### State and concurrency

`AppState` (in `state.rs`) is `manage`d as `Arc<AppState>`; commands take
`State<'_, Arc<AppState>>`. At most one `ActiveMeeting` exists at a time. Settings sit behind an
`RwLock` (read on every model call), the DB behind a single `Mutex<Connection>` — meeting
workloads are a few writes per chunk, so one writer is simpler than a pool and avoids SQLite
write-lock contention.

### Secrets

API keys live in the macOS Keychain via `settings/secrets.rs` and are **never** returned to the
frontend — `settings_get` reports only which keys exist. `Settings::llm_secret_key()` /
`stt_secret_key()` map the selected provider to its Keychain entry; local providers return
`None` and need no key. Preferences go to `config.json`; secrets never do.

## Cross-boundary contracts

Three pairs of files must be edited together — nothing enforces these at compile time:

| Rust | TypeScript |
|---|---|
| `db/models.rs`, `settings/mod.rs` (serde `rename_all = "camelCase"`) | `src/lib/types.ts` |
| `meeting/events.rs` event name constants | `src/lib/events.ts` `EVENT` |
| `commands/mod.rs` + the `invoke_handler!` list in `lib.rs` | `src/lib/api.ts` |

Adding a Tauri command means all three of: the `#[tauri::command]` fn, its entry in
`lib.rs`'s `generate_handler!`, and a wrapper in `api.ts`. Forgetting the middle one fails only
at runtime.

## Things that will bite you

- **`src-tauri/build.rs` adds `-rpath /usr/lib/swift`.** ScreenCaptureKit reaches Apple's
  frameworks through a Swift bridge; without this every binary dies at startup with
  `Library not loaded: @rpath/libswift_Concurrency.dylib`. It is in `build.rs` and **not**
  `.cargo/config.toml` on purpose — cargo reads that file relative to the *current working
  directory*, so building from the repo root would silently produce an unlaunchable binary.
- **`error::Result<T>` shadows `std::result::Result`.** Inside `error.rs` itself (and any impl
  returning a foreign trait's Result) you must write `std::result::Result` explicitly.
- **Screen Recording permission gates system audio**, not just video — macOS classifies it that
  way. Granting it requires an app **restart** to take effect. The app asks for a 2×2 @ 1fps
  video stream it discards purely because SCK has no audio-only mode.
- **All prompt text lives in `prompts.rs`.** Don't inline prompt strings elsewhere.
- **MCP has no transport yet.** `mcp/tools.rs` is fully implemented, unit-tested, read-only, and
  callable in-process via the `mcp_tool_call` command. Adding stdio/HTTP means adapting
  `tools::dispatch` — the tools themselves need no further work.

## Design references

Design mockups live in `designs/` as self-contained HTML exports. The current UI in
`src/components/` is intentionally plain scaffolding driven by CSS variables in `src/App.css`,
built to be restyled rather than to be final.
