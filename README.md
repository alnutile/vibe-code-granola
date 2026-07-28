# vibecode-granola

A local-first meeting recorder and AI notepad for macOS. Press record, and it captures
**everything you hear** — the call, a browser tab, a video — **and your microphone**,
transcribes as you go, offers ideas while you're still talking, writes the meeting up
when you stop, and lets you chat with the result afterwards.

Built with [Tauri 2](https://tauri.app) (Rust + React). It is an open-source take on
Granola, with one deliberate difference: **every model is swappable**, including local
ones. Transcription can run against OpenAI or a Whisper server on `localhost`. Chat can
run against OpenRouter, OpenAI, Ollama, or LM Studio. Nothing about the app changes when
you switch — that's the point.

> **Status:** working foundation. Recording, transcription, live suggestions, note
> generation, and chat all run end to end. The MCP tool surface is implemented and
> callable; connecting it to a transport is the next step. See
> [What's not done yet](#whats-not-done-yet).

---

## How it works

```
 ScreenCaptureKit ──┬── system audio ──┐
                    └── microphone ────┤
                                       ▼
                            chunk at natural pauses
                                       │
                                       ▼
                        speech-to-text  (OpenAI | local Whisper)
                                       │
                                       ▼
                            SQLite  (transcript, notes, chat)
                                       │
                    ┌──────────────────┼──────────────────┐
                    ▼                  ▼                  ▼
             live suggestions      write-up            chat
                    └──────────────────┴──────────────────┘
                                       │
                            chat model  (OpenRouter | OpenAI | Ollama | LM Studio)
```

A few decisions worth knowing about:

**System audio comes from ScreenCaptureKit, not a virtual audio device.** No BlackHole, no
aggregate device, no setup. macOS classifies "record what your speakers are playing" as a
*screen-recording* capability, which is why the app asks for Screen Recording permission
despite never reading a pixel. It requests the smallest, slowest video stream macOS will
give it (2×2 at 1 fps) and discards every frame.

**Mic and system audio stay separate all the way to the transcript.** ScreenCaptureKit tags
each buffer with its origin, so lines get attributed to *You* vs *Them* without running a
speaker-diarization model. You can turn this off to halve transcription calls.

**Audio is cut at natural pauses, not on a fixed timer.** Fixed chunking slices words in
half and Whisper cannot recover the missing halves. Instead, once a chunk passes a minimum
length the app waits for a quiet frame and cuts there — falling back to a hard cut at the
maximum so the live transcript never falls far behind. Silent chunks are dropped entirely,
which also avoids paying for the stock hallucinations Whisper emits on silence
("Thanks for watching!").

**Your own notes outrank the transcript.** Anything you type during the meeting is passed
to the write-up as higher-signal than the machine transcript.

---

## Requirements

- **macOS 15 or later.** ScreenCaptureKit gained separate microphone capture in 15.0;
  system audio alone works from 13.0 if you lower `minimumSystemVersion` and disable mic
  capture.
- [Rust](https://rustup.rs) and [Node.js](https://nodejs.org) 20+.
- Xcode Command Line Tools.

## Getting started

```bash
git clone https://github.com/<you>/vibecode-granola
cd vibecode-granola
npm install
npm run tauri dev
```

On first launch:

1. **Grant Screen Recording permission.** Open Settings inside the app and use the button
   there, or go to *System Settings → Privacy & Security → Screen & System Audio Recording*,
   enable the app, and **restart it** — macOS only applies the change on relaunch.
2. **Configure a chat model** in Settings → *Chat, notes & suggestions*, and paste an API
   key if you picked a hosted provider. Hit **Test connection**.
3. **Configure transcription** in Settings → *Transcription*. Same deal.
4. Press **New meeting**, write a line about what you want out of it, and hit **Record**.
   macOS will ask for microphone access the first time.

The meeting prompt is what makes the live suggestions useful — "I want to leave with a
decision on the vendor" produces far better nudges than an empty box.

## Model configuration

| What | Providers | Endpoint used |
|---|---|---|
| Chat, notes, suggestions | OpenRouter, OpenAI, Ollama, LM Studio, custom | `POST {baseUrl}/chat/completions` |
| Transcription | OpenAI, any OpenAI-compatible server, whisper.cpp | `POST {baseUrl}/audio/transcriptions`, or `/inference` for whisper.cpp |

Everything is base-URL plus model string, so anything that speaks these two APIs works
without code changes.

**Fully local setup**, if you want zero network calls:

```bash
# Chat
ollama serve && ollama pull llama3.1
# → Settings: provider "Ollama", base http://localhost:11434/v1, model llama3.1

# Transcription (speaches, or any faster-whisper server)
docker run -p 8000:8000 ghcr.io/speaches-ai/speaches:latest
# → Settings: provider "Local, OpenAI-compatible", base http://127.0.0.1:8000/v1
```

Comparing a local Whisper against a hosted one is a two-field change in Settings — that
comparison was one of the reasons this app is built the way it is.

## Where your data lives

```
~/Library/Application Support/com.vibecode.granola/
├── config.json     preferences (no secrets)
├── granola.db      meetings, transcripts, notes, chat — SQLite
└── recordings/     per-chunk WAVs, only if you enable "keep audio files"
```

- **API keys go to the macOS Keychain**, never to `config.json` and never into this repo.
  The UI can ask *whether* a key is set; it can't read one back.
- **Audio is deleted after transcription** unless you opt in to keeping it.
- Nothing syncs anywhere. The only outbound traffic is to the model provider you choose,
  and that can be `localhost`.

## Project layout

```
src-tauri/src/
├── audio/          ScreenCaptureKit capture, chunking, WAV encoding
│   ├── macos.rs      the single dual-source capture stream
│   ├── chunker.rs    pause detection and cut points
│   └── wav.rs        downmix, resample, encode
├── db/             SQLite schema and queries (+ FTS5 search)
├── llm/            OpenAI-compatible chat, streaming
├── stt/            OpenAI-compatible + whisper.cpp transcription
├── meeting/        the recording loop, suggestions, write-up, chat
├── mcp/            MCP tool surface (implemented; transport pending)
├── settings/       config file + Keychain secrets
├── prompts.rs      every prompt the app sends, in one place
└── commands/       the Tauri command surface

src/                React UI
├── lib/            typed API client, event subscriptions, shared types
└── components/     sidebar, meeting view, transcript, notes, assistant, settings
```

`prompts.rs` is a single file on purpose: prompt wording is what you tune most often, and
hunting it across modules is miserable.

## Development

```bash
npm run tauri dev                                  # run the app
cargo test --manifest-path src-tauri/Cargo.toml    # Rust unit tests
npx tsc --noEmit                                   # typecheck the frontend
npm run tauri build                                # produce a .app / .dmg
```

`RUST_LOG=vibecode_granola_lib=debug npm run tauri dev` turns on the recording loop's
internals.

<details>
<summary><strong>Why <code>src-tauri/.cargo/config.toml</code> exists</strong></summary>

ScreenCaptureKit reaches Apple's frameworks through a Swift bridge, so the binary needs the
Swift runtime. Those dylibs resolve from the dyld shared cache under `/usr/lib/swift`,
which is not on the default rpath — without that file you get
`Library not loaded: @rpath/libswift_Concurrency.dylib` at startup.
</details>

## What's not done yet

- **MCP transport.** The tool surface (`list_meetings`, `get_meeting`, `get_transcript`,
  `search_meetings`, `list_folders`) is implemented, unit-tested, and callable from the
  Settings screen. Wiring it to stdio/HTTP so Claude Desktop can connect is the next step —
  the tools themselves need no further work.
- **MCP client.** Config shape exists in Settings; no outbound connections yet.
- **Nested folders.** The schema supports `parentId`; the UI renders a flat list.
- **Calendar integration**, meeting auto-detection, and audio playback alongside the
  transcript.
- **Code signing / notarization.** Local builds are unsigned; `entitlements.plist` is ready
  for when they aren't.

## License

MIT — see [LICENSE](LICENSE).
